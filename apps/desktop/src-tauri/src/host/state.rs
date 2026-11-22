//! Process-wide services (app handle, profile paths, database), window labels, event emitting
//! through CEF trusted UI browsers, and the tab-to-window map.

use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock, RwLock};

use serde::Serialize;

use crate::app::runtime::{post_ui, AppHandle};
use crate::storage::{Db, Paths};

pub(super) static APP: OnceLock<AppHandle> = OnceLock::new();
pub(super) static PATHS: OnceLock<Paths> = OnceLock::new();
pub(super) static DB: OnceLock<Db> = OnceLock::new();

pub fn app() -> &'static AppHandle {
    APP.get().unwrap_or(&STATIC_APP)
}
static STATIC_APP: AppHandle = AppHandle;

pub fn paths() -> &'static Paths {
    PATHS.get().expect("paths initialised")
}
pub fn db() -> &'static Db {
    DB.get().expect("database initialised")
}
pub fn persistent_paths() -> &'static Paths {
    paths()
}
pub fn bookmarks_db() -> &'static Db {
    db()
}

pub const MAIN: &str = "main";

pub fn is_private_window(label: &str) -> bool {
    label.starts_with(crate::incognito::LABEL_PREFIX)
}

pub fn is_normal_window(label: &str) -> bool {
    label == MAIN || crate::session::is_extra_window(label)
}

pub fn window_labels() -> Vec<String> {
    app().windows().keys().cloned().collect()
}

pub fn primary_window() -> String {
    let windows = app().windows();
    if windows.contains_key(MAIN) {
        return MAIN.to_string();
    }
    let mut extra: Vec<String> = windows.keys().filter(|l| crate::session::is_extra_window(l)).cloned().collect();
    extra.sort();
    extra.into_iter().next().unwrap_or_else(|| MAIN.to_string())
}

/// Emits an app-wide event to every window's trusted chrome UI.
pub fn emit<S: Serialize + Clone>(event: &str, payload: S) {
    let Ok(val) = serde_json::to_value(payload) else { return };
    let ev = event.to_string();
    let _ = post_ui(move || {
        super::broadcast_chrome_event(&ev, val);
    });
}

/// Emits an event to one window's trusted chrome UI.
pub fn emit_to_window<S: Serialize + Clone>(label: &str, event: &str, payload: S) {
    let Ok(val) = serde_json::to_value(payload) else { return };
    let label = label.to_string();
    let ev = event.to_string();
    let _ = post_ui(move || {
        super::send_chrome_event(&label, &ev, val);
    });
}

static TAB_WINDOWS: LazyLock<RwLock<HashMap<String, String>>> = LazyLock::new(Default::default);

pub(crate) fn set_tab_window(id: &str, label: &str) {
    if let Ok(mut m) = TAB_WINDOWS.write() {
        m.insert(id.to_string(), label.to_string());
    }
}

pub fn window_of_tab(id: &str) -> String {
    TAB_WINDOWS
        .read()
        .ok()
        .and_then(|m| m.get(id).cloned())
        .unwrap_or_else(|| MAIN.to_string())
}

pub fn tab_is_private(id: &str) -> bool {
    is_private_window(&window_of_tab(id))
}

pub fn emit_tab<S: Serialize + Clone>(tab_id: &str, event: &str, payload: S) {
    emit_to_window(&window_of_tab(tab_id), event, payload);
}
