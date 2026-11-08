//! Commands for the media downloader: the videos and audio a tab plays, downloading them, and
//! the tools (ffmpeg) it uses.

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::app::command::{blocking, on_main, on_main_cb, CmdResult};

/// Videos and audio found in a tab. With `scan` (default), the page is searched first.
#[tauri::command]
pub async fn media_list(app: AppHandle, tab_id: String, scan: Option<bool>) -> CmdResult<crate::media::Report> {
    on_main_cb(&app, move |done| crate::media::list(&tab_id, scan.unwrap_or(true), move |r| done(r))).await?
}

/// Starts downloading a listed item; returns the download id.
#[tauri::command]
pub async fn media_download(
    app: AppHandle,
    tab_id: String,
    item_id: u32,
    variant: Option<String>,
    audio: Option<String>,
) -> CmdResult<i64> {
    on_main(&app, move || crate::media::download(&tab_id, item_id, variant, audio)).await?
}

#[tauri::command]
pub async fn media_tools() -> CmdResult<crate::media::Tools> {
    blocking(crate::media::tools).await
}

/// Lets the user point Zenith at ffmpeg.exe.
#[tauri::command]
pub async fn media_pick_ffmpeg(app: AppHandle) -> CmdResult<Option<String>> {
    blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose ffmpeg.exe")
            .add_filter("FFmpeg", &["exe"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
}
