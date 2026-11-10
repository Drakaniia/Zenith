//! Commands for the downloads page: downloads in progress, the saved records, and actions on
//! them.

use std::path::Path;

use tauri::{AppHandle, WebviewWindow};

use super::DownloadRecord;
use crate::app::command::{on_main, CmdResult};
use crate::host::db;
use crate::host::downloads::{self, DownloadEvent};

#[tauri::command]
pub async fn downloads_active(app: AppHandle) -> CmdResult<Vec<DownloadEvent>> {
    on_main(&app, downloads::active).await
}

#[tauri::command]
pub async fn downloads_history(app: AppHandle, limit: Option<i64>) -> CmdResult<Vec<DownloadRecord>> {
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    let live: Vec<i64> = on_main(&app, || downloads::active().into_iter().map(|d| d.record.id).collect()).await?;
    let now = crate::storage::now_ms();
    db().call(move |c| {
        super::settle_stale(c, &live, now)?;
        super::downloads_list(c, limit)
    })
    .await
}

#[tauri::command]
pub async fn download_action(app: AppHandle, id: i64, action: String) -> CmdResult<()> {
    on_main(&app, move || downloads::action(id, &action)).await?
}

/// Fetches a failed or canceled download that can't resume once more, with Zenith's own
/// downloader. The new download takes the old one's place in the list; returns its id.
#[tauri::command]
pub async fn download_again(app: AppHandle, id: i64) -> CmdResult<i64> {
    let record = db()
        .call(move |c| super::download_get(c, id))
        .await?
        .ok_or("This download is no longer in the history (downloads from incognito windows aren't kept), so it can't be fetched again.")?;
    let new_id = on_main(&app, move || super::again::start(&record)).await??;
    db().call(move |c| super::downloads_remove(c, Some(id))).await?;
    Ok(new_id)
}

#[tauri::command]
pub async fn downloads_remove(id: Option<i64>) -> CmdResult<()> {
    crate::media::job::forget(id);
    crate::torrents::forget(id);
    db().call(move |c| super::downloads_remove(c, id)).await
}

/// Drags a finished download's file out of the window (onto a page's upload area or another app)
/// until it is dropped; true when it was. Only files Zenith downloaded can be dragged.
#[tauri::command]
pub async fn download_drag(app: AppHandle, window: WebviewWindow, id: i64) -> CmdResult<bool> {
    let path = match super::finished_path(id) {
        Some(p) => Some(p),
        None => db().call(move |c| super::completed_path(c, id)).await?,
    };
    let path = path.ok_or("This download is no longer in the list.")?;
    if !Path::new(&path).is_file() {
        return Err("The file was moved or deleted.".into());
    }
    let label = window.label().to_string();
    on_main(&app, move || {
        let hwnd = crate::host::with_win(&label, |w| w.hwnd);
        crate::platform::win::drag::drag_file(hwnd, Path::new(&path))
    })
    .await?
}
