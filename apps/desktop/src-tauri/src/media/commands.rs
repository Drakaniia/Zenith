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
    audio_format: Option<String>,
) -> CmdResult<i64> {
    on_main(&app, move || crate::media::download(&tab_id, item_id, variant, audio, audio_format)).await?
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

/// Lets the user point Zenith at yt-dlp.exe.
pub async fn media_pick_ytdlp(_app: AppHandle) -> CmdResult<Option<String>> {
    blocking(move || {
        dialog::pick_file("Choose yt-dlp.exe", &[("yt-dlp", &["exe"])]).map(|p| p.display().to_string())
    })
    .await
}

/// Downloads and installs Zenith's managed copy of `ffmpeg` or `ytdlp`.
pub async fn media_install_tool(tool: String) -> CmdResult<String> {
    blocking(move || crate::media::tools::install(&tool)).await?
}

/// Removes Zenith's managed copy of `ffmpeg` or `ytdlp`.
pub async fn media_remove_tool(tool: String) -> CmdResult<()> {
    blocking(move || crate::media::tools::remove(&tool)).await?
}

/// Probes a video URL (native HTTP/HLS/DASH/HTML first, optional yt-dlp fallback).
pub async fn media_probe_url(url: String, prefer_ytdlp: Option<bool>) -> CmdResult<crate::media::probe::UrlProbeResult> {
    blocking(move || crate::media::probe::probe_url(&url, prefer_ytdlp.unwrap_or(false))).await?
}

/// Starts downloading an item returned by `media_probe_url`.
pub async fn media_download_probed(
    probe_id: String,
    item_id: u32,
    variant: Option<String>,
    audio: Option<String>,
    audio_format: Option<String>,
    tab_id: Option<String>,
) -> CmdResult<i64> {
    blocking(move || {
        crate::media::probe::download_probed(&probe_id, item_id, variant, audio, audio_format, tab_id)
    })
    .await?
}
