//! Commands for the downloads page: downloads in progress, the saved records, and actions on
//! them.

use tauri::AppHandle;

use super::DownloadRecord;
use crate::app::command::{on_main, CmdResult};
use crate::host::db;
use crate::host::downloads::{self, DownloadEvent};

#[tauri::command]
pub async fn downloads_active(app: AppHandle) -> CmdResult<Vec<DownloadEvent>> {
    on_main(&app, downloads::active).await
}

#[tauri::command]
pub async fn downloads_history(limit: Option<i64>) -> CmdResult<Vec<DownloadRecord>> {
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    db().call(move |c| super::downloads_list(c, limit)).await
}

#[tauri::command]
pub async fn download_action(app: AppHandle, id: i64, action: String) -> CmdResult<()> {
    on_main(&app, move || downloads::action(id, &action)).await?
}

#[tauri::command]
pub async fn downloads_remove(id: Option<i64>) -> CmdResult<()> {
    crate::media::job::forget(id);
    db().call(move |c| super::downloads_remove(c, id)).await
}
