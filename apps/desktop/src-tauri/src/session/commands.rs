//! Commands for windows and their sessions: opening another normal window, reopening a closed
//! one, restoring the windows of the last run, and loading and saving each window's tabs.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::app::command::{blocking, private, CmdResult};
use crate::host::{self, paths};
use crate::storage;

/// Opens another normal window (Ctrl+N). `session` is the calling window's snapshot: the new
/// window gets its spaces, essentials and pinned tabs, but not its regular tabs.
#[tauri::command]
pub async fn window_open(app: AppHandle, session: Option<String>) -> CmdResult<()> {
    let start = super::Start { data: session, keep: Some(false), fresh: true };
    std::thread::spawn(move || {
        if let Err(e) = crate::app::window::open_normal_window(&app, start) {
            eprintln!("[zenith] could not open a window: {e}");
        }
    });
    Ok(())
}

/// Reopens the most recently closed normal window (Ctrl+Shift+N); false when there is none.
#[tauri::command]
pub async fn window_undo_close(app: AppHandle) -> CmdResult<bool> {
    let main_open = app.get_webview_window(host::MAIN).is_some();
    let Some(closed) = super::pop_closed(main_open) else { return Ok(false) };
    std::thread::spawn(move || {
        let opened = match closed {
            super::Closed::Main => crate::app::window::reopen_main_window(&app),
            super::Closed::Extra(data) => {
                crate::app::window::open_normal_window(&app, super::Start { data: Some(data), keep: Some(true), fresh: true })
            }
        };
        if let Err(e) = opened {
            eprintln!("[zenith] could not reopen a window: {e}");
        }
    });
    Ok(true)
}

/// The main window's UI calls this once at startup: the other windows that were open when Zenith
/// quit come back (`keep`, the "restore previous session" setting) or are forgotten.
#[tauri::command]
pub async fn windows_restore(app: AppHandle, window: WebviewWindow, keep: bool) -> CmdResult<()> {
    if window.label() != host::MAIN {
        return Ok(());
    }
    let saved = blocking(move || super::take_saved(keep)).await?;
    if saved.is_empty() {
        return Ok(());
    }
    std::thread::spawn(move || {
        for data in saved {
            let start = super::Start { data: Some(data), keep: Some(true), fresh: true };
            if let Err(e) = crate::app::window::open_normal_window(&app, start) {
                eprintln!("[zenith] could not restore a window: {e}");
            }
        }
    });
    Ok(())
}

static SESSION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize)]
pub struct SessionStart {
    data: Option<String>,
    /// Whether to restore the regular tabs too; `None`: the user's setting decides.
    keep: Option<bool>,
    /// The tabs need new ids (they came from another window or an earlier run).
    fresh: bool,
}

/// A window's tabs. The main window's are in `session.json`; another normal window gets what it
/// was opened with (see `session`). Incognito windows always start empty.
#[tauri::command]
pub async fn session_load(window: WebviewWindow) -> CmdResult<SessionStart> {
    let empty = SessionStart { data: None, keep: None, fresh: false };
    if private(&window) {
        return Ok(empty);
    }
    let label = window.label().to_string();
    let start = super::take_pending(&label);
    if label == host::MAIN {
        let data = blocking(|| storage::read_text(&paths().session())).await?;
        return Ok(SessionStart { data, keep: start.and_then(|s| s.keep), fresh: false });
    }
    Ok(start.map(|s| SessionStart { data: s.data, keep: s.keep, fresh: s.fresh }).unwrap_or(empty))
}

/// Incognito windows never write a session.
#[tauri::command]
pub async fn session_save(window: WebviewWindow, data: String) -> CmdResult<()> {
    if private(&window) {
        return Ok(());
    }
    let label = window.label().to_string();
    blocking(move || {
        let _guard = SESSION_LOCK.lock();
        if label == host::MAIN {
            storage::write_atomic(&paths().session(), data.as_bytes()).map_err(|e| e.to_string())
        } else {
            super::save(&label, data)
        }
    })
    .await?
}
