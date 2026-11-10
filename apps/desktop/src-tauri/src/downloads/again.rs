//! "Download again": a failed or canceled download that WebView2 can't resume is fetched anew by
//! Zenith's own downloader (the one fast mode and the media downloader use), under the name it
//! had, in the folder it was going to.

use std::path::Path;

use super::DownloadRecord;
use crate::media::job::{self, Plan, Request, Source};

/// A file name as (stem, extension); the extension is "" when there is none.
pub(crate) fn split_name(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && !ext.contains(' ') => (stem.to_string(), ext.to_string()),
        _ => (name.to_string(), String::new()),
    }
}

/// Starts the download of `record` again and returns the new download's id. Call on the UI
/// thread (it reads the browser's user agent from an open tab).
pub fn start(record: &DownloadRecord) -> Result<i64, String> {
    if !(record.url.starts_with("https://") || record.url.starts_with("http://")) {
        return Err("This file was made by the page itself, so it can't be fetched again. Download it from the page.".into());
    }
    if !matches!(record.state.as_str(), "interrupted" | "canceled") {
        return Err("Only failed or canceled downloads can be downloaded again.".into());
    }
    // A normal (not incognito) tab: its user agent, and the downloads list entry belongs to it.
    let (tab, user_agent) = crate::host::with_host(|h| {
        h.tabs
            .iter()
            .find(|(_, t)| !t.shared.private)
            .map(|(id, t)| (id.clone(), crate::media::sniff::user_agent(&t.core())))
    })
    .flatten()
    .unwrap_or_default();
    let path = Path::new(&record.path);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| "download".into());
    let (stem, ext) = split_name(&name);
    let dir = path.parent().filter(|d| d.is_dir()).map(Path::to_path_buf).unwrap_or_else(job::download_dir);
    let req = Request {
        tab,
        stem,
        mime: record.mime.clone(),
        source_url: record.url.clone(),
        // The page is not known any more: the file's own address stands in for it, so its site's
        // cookies go along as they did when the link was opened.
        page_url: record.url.clone(),
        headers: Vec::new(),
        user_agent,
        plan: Plan { main: Source::Download { url: record.url.clone(), ext: ext.clone() }, audio: None, live: false },
        ext,
    };
    job::start_download(req, dir, None)
}
