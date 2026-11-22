//! Commands for the media downloader: the videos and audio a tab plays, downloading them, and
//! the tools (ffmpeg) it uses.

use crate::app::command::{blocking, on_main, on_main_cb, CmdResult};
use crate::app::runtime::AppHandle;
use crate::platform::win::dialog;

/// Videos and audio found in a tab. With `scan` (default), the page is searched first.
pub async fn media_list(app: AppHandle, tab_id: String, scan: Option<bool>) -> CmdResult<crate::media::Report> {
    on_main_cb(&app, move |done| crate::media::list(&tab_id, scan.unwrap_or(true), move |r| done(r))).await?
}

/// Starts downloading a listed item; returns the download id.
pub async fn media_download(
    app: AppHandle,
    tab_id: String,
    item_id: u32,
    variant: Option<String>,
    audio: Option<String>,
) -> CmdResult<i64> {
    on_main(&app, move || crate::media::download(&tab_id, item_id, variant, audio)).await?
}

pub async fn media_tools() -> CmdResult<crate::media::Tools> {
    blocking(crate::media::tools).await
}

/// Lets the user point Zenith at ffmpeg.exe.
pub async fn media_pick_ffmpeg(_app: AppHandle) -> CmdResult<Option<String>> {
    blocking(move || {
        dialog::pick_file("Choose ffmpeg.exe", &[("FFmpeg", &["exe"])]).map(|p| p.display().to_string())
    })
    .await
}
