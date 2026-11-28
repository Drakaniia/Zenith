//! Open tabs of a Chromium browser's last session, read from its SNSS session file.
//!
//! SNSS: "SNSS" + i32 version, then records of [u16 size][u8 command][size - 1 bytes]. Only a few
//! commands are read; the parser stops at the first malformed record and never panics.
//!
//! shortcut: Firefox and Zen are not read (recovery.jsonlz4 needs an lz4 decoder, which is not a
//! dependency), add one when Firefox tab import is wanted.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{is_web, scan, Engine};

/// Most tabs taken from one session.
const TAB_LIMIT: usize = 100;

const SET_TAB_WINDOW: u8 = 0;
const UPDATE_TAB_NAVIGATION: u8 = 6;
const SET_SELECTED_NAVIGATION_INDEX: u8 = 7;
const TAB_CLOSED: u8 = 16;
const WINDOW_CLOSED: u8 = 17;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ImportedTab {
    pub url: String,
    pub title: String,
}

/// The newest `Sessions/Session_<timestamp>` file, or the legacy `Current Session`.
pub fn session_file(profile: &Path) -> Option<PathBuf> {
    let newest = fs::read_dir(profile.join("Sessions")).ok().and_then(|rd| {
        rd.flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let stamp: u64 = name.strip_prefix("Session_")?.parse().ok()?;
                Some((stamp, e.path()))
            })
            .max_by_key(|(stamp, _)| *stamp)
            .map(|(_, path)| path)
    });
    newest.or_else(|| Some(profile.join("Current Session")).filter(|p| p.is_file()))
}

/// The open tabs of a profile found by `detect`.
pub fn read(id: &str) -> Result<Vec<ImportedTab>, String> {
    let found = scan()
        .into_iter()
        .find(|f| f.source.id == id)
        .ok_or_else(|| "That browser profile is no longer available.".to_string())?;
    let name = &found.source.name;
    let file = (found.engine == Engine::Chromium).then(|| session_file(&found.dir)).flatten();
    // Read in place: reading never changes the file, and a copy would hit the same share lock.
    let bytes = file
        .and_then(|f| fs::read(f).ok())
        .ok_or_else(|| format!("Couldn't read {name}'s open tabs."))?;
    Ok(parse_snss(&bytes))
}

#[derive(Default)]
struct TabState {
    window: i32,
    selected: Option<i32>,
    navs: HashMap<i32, ImportedTab>,
    last: i32,
}

/// The selected page of every tab still open at the end of the session, in first-seen order.
pub fn parse_snss(data: &[u8]) -> Vec<ImportedTab> {
    if data.len() < 8 || &data[..4] != b"SNSS" || !matches!(i32_at(data, 4), Some(1 | 3)) {
        return Vec::new(); // versions 2 and 4 are encrypted
    }
    let mut tabs: HashMap<i32, TabState> = HashMap::new();
    let mut order: Vec<i32> = Vec::new();
    let mut closed_windows: Vec<i32> = Vec::new();
    let mut at = 8;
    while let Some(size) = data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) {
        let Some(rec) = data.get(at + 2..at + 2 + size).filter(|r| !r.is_empty()) else { break };
        at += 2 + size;
        let (cmd, body) = (rec[0], &rec[1..]);
        let pair = || Some((i32_at(body, 0)?, i32_at(body, 4)?));
        match cmd {
            SET_TAB_WINDOW => {
                let Some((window, id)) = pair() else { break };
                tab(&mut tabs, &mut order, id).window = window;
            }
            UPDATE_TAB_NAVIGATION => {
                let Some((id, index, nav)) = navigation(body) else { break };
                let t = tab(&mut tabs, &mut order, id);
                t.last = index;
                t.navs.insert(index, nav);
            }
            SET_SELECTED_NAVIGATION_INDEX => {
                let Some((id, index)) = pair() else { break };
                tab(&mut tabs, &mut order, id).selected = Some(index);
            }
            TAB_CLOSED => {
                let Some(id) = i32_at(body, 0) else { break };
                tabs.remove(&id);
            }
            WINDOW_CLOSED => {
                let Some(id) = i32_at(body, 0) else { break };
                closed_windows.push(id);
            }
            _ => {}
        }
    }
    order
        .iter()
        .filter_map(|id| tabs.remove(id))
        .filter(|t| !closed_windows.contains(&t.window))
        .filter_map(|mut t| t.navs.remove(&t.selected.unwrap_or(t.last)).or_else(|| t.navs.remove(&t.last)))
        .filter(|nav| is_web(&nav.url))
        .take(TAB_LIMIT)
        .collect()
}

fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// A pickled UpdateTabNavigation: u32 pickle size, i32 tab id, i32 index, string url,
/// string16 title, … (each field padded to 4 bytes).
fn navigation(body: &[u8]) -> Option<(i32, i32, ImportedTab)> {
    let id = i32_at(body, 4)?;
    let index = i32_at(body, 8)?;
    let url_len = usize::try_from(i32_at(body, 12)?).ok()?;
    let url = String::from_utf8_lossy(body.get(16..16 + url_len)?).into_owned();
    let at = 16 + url_len.div_ceil(4) * 4;
    let chars = usize::try_from(i32_at(body, at)?).ok()?;
    let raw = body.get(at + 4..at + 4 + chars * 2)?;
    let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    Some((id, index, ImportedTab { url, title: String::from_utf16_lossy(&units) }))
}

/// The tab's state, remembering the order tabs were first seen in.
fn tab<'a>(tabs: &'a mut HashMap<i32, TabState>, order: &mut Vec<i32>, id: i32) -> &'a mut TabState {
    if !tabs.contains_key(&id) {
        order.push(id);
    }
    tabs.entry(id).or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(out: &mut Vec<u8>, cmd: u8, body: &[u8]) {
        out.extend_from_slice(&((body.len() + 1) as u16).to_le_bytes());
        out.push(cmd);
        out.extend_from_slice(body);
    }

    fn ints(v: &[i32]) -> Vec<u8> {
        v.iter().flat_map(|n| n.to_le_bytes()).collect()
    }

    fn nav(tab: i32, index: i32, url: &str, title: &str) -> Vec<u8> {
        let mut p = ints(&[0, tab, index, url.len() as i32]);
        p.extend_from_slice(url.as_bytes());
        p.resize(p.len().div_ceil(4) * 4, 0);
        let t: Vec<u16> = title.encode_utf16().collect();
        p.extend(ints(&[t.len() as i32]));
        p.extend(t.iter().flat_map(|u| u.to_le_bytes()));
        p.resize(p.len().div_ceil(4) * 4, 0);
        p.extend(ints(&[0, 0])); // fields after the title are ignored
        let size = (p.len() - 4) as i32;
        p[..4].copy_from_slice(&size.to_le_bytes());
        p
    }

    #[test]
    fn reads_selected_navigation_of_open_tabs() {
        let mut f = b"SNSS".to_vec();
        f.extend(ints(&[3]));
        record(&mut f, SET_TAB_WINDOW, &ints(&[1, 10]));
        record(&mut f, UPDATE_TAB_NAVIGATION, &nav(10, 0, "https://a.example/", "A"));
        record(&mut f, UPDATE_TAB_NAVIGATION, &nav(10, 1, "https://a2.example/", "A two"));
        record(&mut f, SET_SELECTED_NAVIGATION_INDEX, &ints(&[10, 0]));
        record(&mut f, UPDATE_TAB_NAVIGATION, &nav(11, 0, "https://closed.example/", "Gone"));
        record(&mut f, TAB_CLOSED, &ints(&[11, 0, 0, 0]));
        record(&mut f, UPDATE_TAB_NAVIGATION, &nav(12, 0, "chrome://settings/", "Settings"));
        record(&mut f, SET_TAB_WINDOW, &ints(&[2, 13]));
        record(&mut f, UPDATE_TAB_NAVIGATION, &nav(13, 0, "https://w2.example/", "W2"));
        record(&mut f, WINDOW_CLOSED, &ints(&[2, 0, 0, 0]));
        record(&mut f, UPDATE_TAB_NAVIGATION, &nav(14, 2, "http://b.example/ü", "Bé"));
        let good = f.len();
        record(&mut f, UPDATE_TAB_NAVIGATION, &nav(15, 0, "https://late.example/", "Late"));
        f.truncate(f.len() - 3); // a record cut off mid-write

        let tabs = parse_snss(&f);
        assert_eq!(
            tabs,
            [
                ImportedTab { url: "https://a.example/".into(), title: "A".into() },
                ImportedTab { url: "http://b.example/ü".into(), title: "Bé".into() },
            ]
        );
        assert_eq!(parse_snss(&f[..good]), tabs);
        assert!(parse_snss(b"SNSS\x02\0\0\0").is_empty(), "encrypted versions are skipped");
        assert!(parse_snss(b"junk").is_empty());
        // Every prefix parses without panicking.
        for n in 0..f.len() {
            parse_snss(&f[..n]);
        }
    }
}
