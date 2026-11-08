//! Sessions of the normal windows.
//!
//! The main window keeps its tabs in `session.json`, as before. Every other normal window
//! (`window-N`, opened with Ctrl+N) keeps its own in `windows\window-N.json` while it is open,
//! so the windows that were open when Zenith quit come back at the next start. Closing such a
//! window deletes its file and remembers its last session in memory for "Reopen closed window"
//! (Zen's Ctrl+Shift+N). Incognito windows never save anything.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::storage;

/// Labels of the normal windows other than the main one start with this.
pub const LABEL_PREFIX: &str = "window-";

/// How many closed windows "Reopen closed window" remembers.
const MAX_CLOSED: usize = 10;

/// `window-N`: one of the other normal windows.
pub fn is_extra_window(label: &str) -> bool {
    label.strip_prefix(LABEL_PREFIX).is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// What a window's UI gets from `session_load` when it starts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Start {
    /// The session to restore (the main window reads its file instead when this is `None`).
    pub data: Option<String>,
    /// Whether to restore its regular tabs: `None` leaves it to the user's setting.
    pub keep: Option<bool>,
    /// The tabs came from another window (or an earlier run): they need new ids.
    pub fresh: bool,
}

/// A closed normal window, newest last.
#[derive(Clone, Debug, PartialEq)]
pub enum Closed {
    /// The main window: its session is still in `session.json`.
    Main,
    Extra(String),
}

#[derive(Default)]
struct State {
    /// Sessions handed to windows that are being created, by label.
    pending: HashMap<String, Start>,
    /// The last session each open extra window saved.
    latest: HashMap<String, String>,
    closed: Vec<Closed>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
static RESTORED: AtomicBool = AtomicBool::new(false);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

fn dir() -> PathBuf {
    crate::host::persistent_paths().roaming.join("windows")
}

fn file(label: &str) -> Option<PathBuf> {
    is_extra_window(label).then(|| dir().join(format!("{label}.json")))
}

/// Gives the window about to be created as `label` what to start with.
pub fn set_pending(label: &str, start: Start) {
    with(|s| s.pending.insert(label.to_string(), start));
}

/// What window `label` starts with (once).
pub fn take_pending(label: &str) -> Option<Start> {
    with(|s| s.pending.remove(label))
}

/// Saves an extra window's session (in memory and on disk).
pub fn save(label: &str, data: String) -> Result<(), String> {
    let path = file(label).ok_or("not a browser window")?;
    storage::write_atomic(&path, data.as_bytes()).map_err(|e| e.to_string())?;
    with(|s| s.latest.insert(label.to_string(), data));
    Ok(())
}

/// A normal window closed (not because Zenith quit): it can be reopened, and an extra window's
/// file goes away so it doesn't come back at the next start.
pub fn window_closed(label: &str) {
    if label == crate::host::MAIN {
        with(|s| push_closed(&mut s.closed, Closed::Main));
    } else if let Some(path) = file(label) {
        let _ = std::fs::remove_file(path);
        with(|s| {
            if let Some(data) = s.latest.remove(label) {
                push_closed(&mut s.closed, Closed::Extra(data));
            }
        });
    }
}

fn push_closed(list: &mut Vec<Closed>, c: Closed) {
    if c == Closed::Main {
        list.retain(|x| *x != Closed::Main);
    }
    list.push(c);
    if list.len() > MAX_CLOSED {
        list.remove(0);
    }
}

/// The most recently closed window that can be reopened. The main window is skipped while it is
/// open (`main_open`).
pub fn pop_closed(main_open: bool) -> Option<Closed> {
    with(|s| take_closed(&mut s.closed, main_open))
}

fn take_closed(list: &mut Vec<Closed>, main_open: bool) -> Option<Closed> {
    let i = list.iter().rposition(|c| !(main_open && *c == Closed::Main))?;
    Some(list.remove(i))
}

/// Runs once (for the main window at startup): the sessions of the extra windows that were open
/// when Zenith last quit. Their files are removed: the reopened windows write new ones. With
/// `keep` false they are only removed.
pub fn take_saved(keep: bool) -> Vec<String> {
    if RESTORED.swap(true, Ordering::SeqCst) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(dir()) else { return Vec::new() };
    let mut files: Vec<(u64, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            let n = stem.strip_prefix(LABEL_PREFIX)?.parse::<u64>().ok()?;
            (path.extension()? == "json").then_some((n, path))
        })
        .collect();
    files.sort();
    let mut out = Vec::new();
    for (_, path) in files {
        if keep {
            if let Some(data) = storage::read_text(&path) {
                out.push(data);
            }
        }
        let _ = std::fs::remove_file(&path);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_window_labels() {
        assert!(is_extra_window("window-1"));
        assert!(is_extra_window("window-42"));
        assert!(!is_extra_window("window-"));
        assert!(!is_extra_window("window-1/../x"));
        assert!(!is_extra_window("main"));
        assert!(!is_extra_window("incognito-1"));
    }

    #[test]
    fn closed_windows_reopen_newest_first() {
        let mut list = Vec::new();
        push_closed(&mut list, Closed::Extra("a".into()));
        push_closed(&mut list, Closed::Main);
        push_closed(&mut list, Closed::Extra("b".into()));
        assert_eq!(take_closed(&mut list, false), Some(Closed::Extra("b".into())));
        // The main window can't be reopened while it is open; the one before it can.
        assert_eq!(take_closed(&mut list, true), Some(Closed::Extra("a".into())));
        assert_eq!(take_closed(&mut list, true), None);
        assert_eq!(take_closed(&mut list, false), Some(Closed::Main));
        assert_eq!(take_closed(&mut list, false), None);
    }

    #[test]
    fn the_main_window_is_listed_once_and_the_list_is_capped() {
        let mut list = Vec::new();
        push_closed(&mut list, Closed::Main);
        push_closed(&mut list, Closed::Main);
        assert_eq!(list, vec![Closed::Main]);
        for i in 0..20 {
            push_closed(&mut list, Closed::Extra(i.to_string()));
        }
        assert_eq!(list.len(), MAX_CLOSED);
        assert_eq!(list.last(), Some(&Closed::Extra("19".into())));
    }
}
