//! Hidden fast mode for page downloads: a large download from a server that serves byte ranges
//! is handed from WebView2 to Zenith's own downloader, which fetches it over several connections.
//!
//! WebView2's DownloadStarting event is deferred while a worker thread asks the server for the
//! first byte, with the page's cookies. Only a partial answer for exactly the size WebView2
//! announced moves the download over (so form posts, one-time links and pages that answer a
//! second request differently stay with WebView2); anything else, or no answer within a few
//! seconds, lets WebView2 carry on as if nothing happened.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::Url;
use webview2_com::Microsoft::Web::WebView2::Win32::*;

use super::downloads::{track, Started};
use super::tab::TabShared;
use crate::media::http::Client;
use crate::media::job::{self, Plan, Request, Source, FAST_MIN_BYTES};
use crate::platform::urls::{host_of, site_of};

/// How long a download may wait for the server's answer before WebView2 takes it after all.
const PROBE_WAIT: Duration = Duration::from_secs(4);

/// A download waiting for the probe. WebView2 objects never leave the UI thread, so they wait
/// here and the worker only sends back the id.
struct Pending {
    args: ICoreWebView2DownloadStartingEventArgs,
    deferral: ICoreWebView2Deferral,
    op: ICoreWebView2DownloadOperation,
    shared: Rc<TabShared>,
    started: Started,
}

thread_local! {
    static PENDING: RefCell<HashMap<u64, Pending>> = RefCell::default();
}
static NEXT: AtomicU64 = AtomicU64::new(1);

/// Worth trying: fast mode on, a normal window, a large http(s) file of known size.
pub(super) fn wanted(shared: &TabShared, s: &Started) -> bool {
    super::config().fast_download
        && !shared.private
        && (s.uri.starts_with("https://") || s.uri.starts_with("http://"))
        && s.total >= FAST_MIN_BYTES as i64
        && !crate::torrents::is_torrent_file(&s.path, &s.mime)
}

/// Holds the download back and checks the server on a worker thread.
pub(super) fn offer(
    core: &ICoreWebView2,
    args: &ICoreWebView2DownloadStartingEventArgs,
    op: ICoreWebView2DownloadOperation,
    shared: Rc<TabShared>,
    started: Started,
) -> windows::core::Result<()> {
    let deferral = unsafe { args.GetDeferral()? };
    let page = shared.url.borrow().clone();
    let req = request(&shared.id, &started, &page, crate::media::sniff::user_agent(core));
    let total = started.total as u64;
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    PENDING.with(|p| p.borrow_mut().insert(id, Pending { args: args.clone(), deferral, op, shared, started }));
    let decided = Arc::new(AtomicBool::new(false));
    let first = decided.clone();
    std::thread::spawn(move || {
        let ok = probe(&req, total);
        if !first.swap(true, Ordering::SeqCst) {
            let _ = super::app().run_on_main_thread(move || decide(id, ok.then_some(req)));
        }
    });
    std::thread::spawn(move || {
        std::thread::sleep(PROBE_WAIT);
        if !decided.swap(true, Ordering::SeqCst) {
            let _ = super::app().run_on_main_thread(move || decide(id, None));
        }
    });
    Ok(())
}

/// Starts Zenith's download and cancels WebView2's, or lets WebView2 go on.
fn decide(id: u64, req: Option<Request>) {
    let Some(p) = PENDING.with(|m| m.borrow_mut().remove(&id)) else { return };
    let dir = Path::new(&p.started.path).parent().map(Path::to_path_buf).filter(|d| d.is_dir()).unwrap_or_else(job::download_dir);
    let taken = req.is_some_and(|r| job::start_download(r, dir, p.started.origin.clone()).is_ok());
    if taken {
        let _ = unsafe { p.args.SetCancel(true) };
    } else if let Err(e) = track(p.op, p.started, &p.shared) {
        eprintln!("[zenith] download could not be followed: {e}");
    }
    let _ = unsafe { p.deferral.Complete() };
}

/// The job for a page download: saved under the name WebView2 chose.
fn request(tab: &str, s: &Started, page: &str, user_agent: String) -> Request {
    let name = Path::new(&s.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "download".into());
    let p = PathBuf::from(&name);
    let ext = p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if ext.is_empty() { name.clone() } else { p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name) };
    let headers = referrer(page, &s.uri).map(|r| vec![("Referer".to_string(), r)]).unwrap_or_default();
    Request {
        tab: tab.to_string(),
        stem,
        mime: s.mime.clone(),
        source_url: s.uri.clone(),
        page_url: page.to_string(),
        headers,
        user_agent,
        plan: Plan { main: Source::Download { url: s.uri.clone(), ext: ext.clone() }, audio: None, live: false },
        ext,
    }
}

/// The Referer a browser sends by default (strict-origin-when-cross-origin): the whole page
/// address to its own origin, only the origin elsewhere, nothing from HTTPS to HTTP.
fn referrer(page: &str, target: &str) -> Option<String> {
    let (page, target) = (Url::parse(page).ok()?, Url::parse(target).ok()?);
    if !matches!(page.scheme(), "http" | "https") || (page.scheme() == "https" && target.scheme() == "http") {
        return None;
    }
    if page.origin() == target.origin() {
        let mut p = page.clone();
        p.set_fragment(None);
        return Some(p.to_string());
    }
    Some(format!("{}/", page.origin().ascii_serialization()))
}

/// True when the server answers the first byte as part of a file of exactly `total` bytes.
fn probe(req: &Request, total: u64) -> bool {
    let Source::Download { url, .. } = &req.plan.main else { return false };
    let Ok(client) = Client::with_policy(&req.user_agent, job::network_policy(&req.page_url)) else { return false };
    let site = host_of(&req.page_url).map(site_of).unwrap_or("").to_string();
    let headers = |u: &str| {
        let mut h = req.headers.clone();
        if let Some(c) = crate::media::sniff::cookie_header(u, &site) {
            h.push(("Cookie".into(), c));
        }
        h
    };
    matches!(client.get(url, &headers, Some((0, Some(0))), false), Ok(r) if r.status == 206 && r.total == Some(total))
}

#[cfg(test)]
mod tests {
    use super::referrer;

    #[test]
    fn referrer_follows_the_default_policy() {
        let page = "https://files.example/releases/v2?tab=win#top";
        assert_eq!(referrer(page, "https://files.example/dl/setup.exe").as_deref(), Some("https://files.example/releases/v2?tab=win"));
        assert_eq!(referrer(page, "https://cdn.example/setup.exe").as_deref(), Some("https://files.example/"));
        assert_eq!(referrer(page, "http://files.example/setup.exe"), None, "never from HTTPS to HTTP");
        assert_eq!(referrer("about:blank", "https://cdn.example/a"), None);
    }
}
