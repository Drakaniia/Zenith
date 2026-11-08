//! The command the UI pushes its shortcut bindings with.

use tauri::AppHandle;

use super::keys::BindingSpec;
use crate::app::command::{on_main, CmdResult};
use crate::host;

/// Returns the system-wide shortcuts that could not be registered (another app owns them).
#[tauri::command]
pub async fn set_shortcuts(app: AppHandle, bindings: Vec<BindingSpec>) -> CmdResult<Vec<String>> {
    on_main(&app, move || host::set_bindings(bindings)).await
}
