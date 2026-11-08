//! Process-wide services (app handle, profile paths, database), window labels, event emitting
//! and the tab-to-window map.

use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock, RwLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::storage::{Db, Paths};

// ---------------------------------------------------------------------------------------------
// Process-wide services

pub(super) static APP: OnceLock<AppHandle> = OnceLock::new();
pub(super) static PATHS: OnceLock<Paths> = OnceLock::new();
pub(super) static DB: OnceLock<Db> = OnceLock::new();

pub fn app() -> &'static AppHandle {
    APP.get().expect("app handle initialised")
}
pub fn paths() -> &'static Paths {
    PATHS.get().expect("paths initialised")
}
pub fn db() -> &'static Db {
    DB.get().expect("database initialised")
}
/// The user's profile. Incognito windows run in the same process and keep their pages'
/// data in WebView2's in-memory InPrivate profile, so there is only one profile on disk.
pub fn persistent_paths() -> &'static Paths {
    paths()
}
/// Bookmarks are shared with incognito windows, as in other browsers; history is not.
pub fn bookmarks_db() -> &'static Db {
    db()
}

/// Label of the normal browser window.
pub const MAIN: &str = "main";

/// Incognito windows are labelled `incognito-N`.
pub fn is_private_window(label: &str) -> bool {
    label.starts_with(crate::incognito::LABEL_PREFIX)
}

/// The main window or one of the other normal windows (`window-N`, see `session`).
pub fn is_normal_window(label: &str) -> bool {
    label == MAIN || crate::session::is_extra_window(label)
}

/// The normal window to send app-wide requests to (links from other apps, extension pages):
/// the main window, else another normal one.
pub fn primary_window() -> String {
    let windows = app().webview_windows();
    if windows.contains_key(MAIN) {
        return MAIN.to_string();
    }
    let mut extra: Vec<&String> = windows.keys().filter(|l| crate::session::is_extra_window(l)).collect();
    extra.sort();
    extra.first().map(|l| l.to_string()).unwrap_or_else(|| MAIN.to_string())
}

/// Emits an app-wide event (downloads, updates) to every window's chrome UI.
pub fn emit<S: Serialize + Clone>(event: &str, payload: S) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, payload);
    }
}

/// Emits an event to one window's chrome UI.
pub fn emit_to_window<S: Serialize + Clone>(label: &str, event: &str, payload: S) {
    if let Some(app) = APP.get() {
        let _ = app.emit_to(label, event, payload);
    }
}

/// Which window each tab id belongs to. Readable from any thread (events are emitted from
/// worker threads too). Entries are tiny and tab ids are unique, so they are never removed:
/// a sleeping tab woken later keeps its window.
static TAB_WINDOWS: LazyLock<RwLock<HashMap<String, String>>> = LazyLock::new(Default::default);

pub(crate) fn set_tab_window(id: &str, label: &str) {
    if let Ok(mut m) = TAB_WINDOWS.write() {
        m.insert(id.to_string(), label.to_string());
    }
}

/// The window a tab lives in (the main window for unknown ids).
pub fn window_of_tab(id: &str) -> String {
    TAB_WINDOWS
        .read()
        .ok()
        .and_then(|m| m.get(id).cloned())
        .unwrap_or_else(|| MAIN.to_string())
}

/// True for tabs of an incognito window.
pub fn tab_is_private(id: &str) -> bool {
    is_private_window(&window_of_tab(id))
}

/// Emits a tab's event to the window the tab lives in.
pub fn emit_tab<S: Serialize + Clone>(tab_id: &str, event: &str, payload: S) {
    emit_to_window(&window_of_tab(tab_id), event, payload);
}
