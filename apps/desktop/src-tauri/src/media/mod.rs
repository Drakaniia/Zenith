//! Built-in media downloader: finds the videos and audio that pages play (progressive files,
//! HLS and DASH streams), lists them per tab, and downloads them into the regular downloads list.
//!
//! - `sniff` watches every tab's network responses and reads stream manifests; it also scans the
//!   page (`<video>`/`<audio>`, `og:video`) on demand and serves cookies to downloads.
//! - `TabMedia` (here, implemented in `tab_media` and `request`) is a tab's list of what was found, with HLS media playlists folded into
//!   their master playlist.
//! - `job` downloads an item: plain files with resume, segmented streams in parallel, live HLS
//!   recording, and merging of separate audio and video (natively for fragmented MP4, otherwise
//!   with FFmpeg when it is installed).
//! - DRM-protected media (Encrypted Media Extensions, encrypted HLS, DASH ContentProtection,
//!   Common Encryption) is detected and never downloaded.

pub mod commands;
pub mod dash;
pub mod detect;
pub mod hls;
pub mod http;
pub mod job;
pub mod mp4;
pub mod sniff;

mod choices;
mod naming;
mod request;
mod tab_media;
#[cfg(test)]
mod tests;

use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};

pub use naming::{format_bitrate, mime_for, quality_label, sanitize_stem};

/// A quality or audio-language choice offered for a stream.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub height: Option<u32>,
    pub bandwidth: Option<u64>,
    pub audio_only: bool,
}

/// One downloadable item as the media panel shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: u32,
    /// "file" | "hls" | "dash"
    pub kind: &'static str,
    pub url: String,
    pub title: String,
    pub ext: String,
    pub size: Option<u64>,
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub audio_only: bool,
    pub live: bool,
    pub protected: bool,
    /// "network" (seen loading) or "page" (found in the document).
    pub found_by: &'static str,
    pub detected: i64,
    pub variants: Vec<Choice>,
    pub audio_tracks: Vec<Choice>,
    /// Why the item can't be downloaded, or other remarks.
    pub note: String,
}

pub enum Manifest {
    None,
    HlsMaster(hls::Master),
    HlsMedia(hls::MediaPlaylist),
    Dash(dash::Mpd),
}

pub struct Entry {
    pub item: MediaItem,
    pub manifest: Manifest,
    /// Request headers worth replaying (Referer, Origin, User-Agent).
    pub headers: Vec<(String, String)>,
    pub page_url: String,
    /// Playlists of an HLS master, so their own responses fold into it.
    children: Vec<String>,
    /// Children whose details were folded in already (later refreshes are not read again).
    folded: Vec<String>,
}

/// Where an item was found and on which page.
pub struct Found<'a> {
    pub by: &'static str,
    pub headers: Vec<(String, String)>,
    pub page_url: &'a str,
    pub title: &'a str,
}

/// Result of the page scan script (`detect::SCAN_SCRIPT`); untrusted.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Scan {
    pub title: String,
    pub url: String,
    pub blobs: u32,
    pub drm: u32,
    pub items: Vec<ScanItem>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct ScanItem {
    pub url: String,
    pub audio: bool,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub meta: bool,
    #[serde(rename = "type")]
    pub mime: String,
}

#[derive(Default)]
pub struct TabMedia {
    pub entries: Vec<Entry>,
    next_id: u32,
    /// Bumped on every navigation; late async results for an older page are dropped.
    pub epoch: u64,
    /// Recent requests made by `<video>`/`<audio>` elements.
    media_requests: VecDeque<String>,
    /// Manifests being read.
    pub pending: HashSet<String>,
    /// Manifests read since the page loaded.
    manifest_reads: u32,
    /// Players on the page that stream through Media Source (`blob:` sources).
    pub blobs: u32,
    /// Players using Encrypted Media Extensions (DRM).
    pub drm: u32,
}

/// What the media panel receives.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub items: Vec<MediaItem>,
    pub blobs: u32,
    pub drm: u32,
    pub ffmpeg: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub ffmpeg: Option<String>,
}

pub fn tools() -> Tools {
    Tools { ffmpeg: job::find_ffmpeg().map(|p| p.display().to_string()) }
}

fn report(tab_id: &str) -> Result<Report, String> {
    let shared = crate::host::with_host(|h| h.tabs.get(tab_id).map(|t| t.shared.clone()))
        .flatten()
        .ok_or("This tab is not loaded")?;
    let media = shared.media.borrow();
    Ok(Report { items: media.items(), blobs: media.blobs, drm: media.drm, ffmpeg: job::find_ffmpeg().is_some() })
}

/// Lists a tab's media; with `scan`, the page is searched first. Runs on the UI thread.
pub fn list(tab_id: &str, scan: bool, done: impl FnOnce(Result<Report, String>) + 'static) {
    if !scan {
        done(report(tab_id));
        return;
    }
    let id = tab_id.to_string();
    sniff::scan(tab_id, move || done(report(&id)));
}

/// Starts downloading an item. Runs on the UI thread.
pub fn download(tab_id: &str, item_id: u32, variant: Option<String>, audio: Option<String>) -> Result<i64, String> {
    let (shared, core) = crate::host::with_host(|h| h.tabs.get(tab_id).map(|t| (t.shared.clone(), t.core())))
        .flatten()
        .ok_or("This tab is not loaded")?;
    let ua = sniff::user_agent(&core);
    let request = shared.media.borrow().request(tab_id, item_id, variant.as_deref(), audio.as_deref(), &ua)?;
    job::start(request)
}
