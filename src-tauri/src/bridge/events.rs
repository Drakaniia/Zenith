use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget};

pub const BROWSER_WEBVIEW_NAVIGATION: &str = "browser-webview-navigation";
pub const BROWSER_WEBVIEW_TITLE: &str = "browser-webview-title";
pub const BROWSER_WEBVIEW_LOAD: &str = "browser-webview-load";
pub const BROWSER_SHORTCUT: &str = "browser-shortcut";
pub const DOWNLOAD_PROGRESS: &str = "download-progress";

pub fn emit_to_shell<T>(
    app: &AppHandle,
    shell_label: impl Into<String>,
    event: &str,
    payload: T,
) -> Result<(), tauri::Error>
where
    T: Clone + Serialize,
{
    app.emit_to(EventTarget::labeled(shell_label.into()), event, payload)
}

pub fn emit_global<T>(app: &AppHandle, event: &str, payload: T) -> Result<(), tauri::Error>
where
    T: Clone + Serialize,
{
    app.emit(event, payload)
}
