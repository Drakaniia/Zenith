//! Hidden fast mode for page downloads: probes whether the server serves exact byte ranges
//! before deciding whether Zenith's segmented downloader takes over or CEF downloads normally.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use cef::*;
use url::Url;

use super::downloads::{register_download, Started};
use super::tab::TabShared;
use crate::app::runtime::{post_ui, post_ui_delayed};
use crate::media::http::Client;
use crate::media::job::{self, Plan, Request, Source, FAST_MIN_BYTES};
use crate::platform::urls::{host_of, site_of};

const PROBE_WAIT_MS: i64 = 4000;

struct Pending {
    cb: BeforeDownloadCallback,
    shared: Arc<TabShared>,
    started: Started,
}

thread_local! {
    static PENDING: RefCell<HashMap<u64, Pending>> = RefCell::default();
}
static NEXT: AtomicU64 = AtomicU64::new(1);

pub(super) fn wanted(shared: &TabShared, s: &Started) -> bool {
    super::config().fast_download
        && !shared.private
        && (s.uri.starts_with("https://") || s.uri.starts_with("http://"))
        && s.total >= FAST_MIN_BYTES as i64
        && !crate::torrents::is_torrent_file(&s.path, &s.mime)
}

pub(super) fn offer(
    cb: BeforeDownloadCallback,
    shared: Arc<TabShared>,
    started: Started,
) {
    let page = shared.url();
    let req = request(&shared.id, &started, &page, crate::media::sniff::default_user_agent());
    let total = started.total as u64;
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    PENDING.with(|p| p.borrow_mut().insert(id, Pending { cb, shared, started }));
    let decided = Arc::new(AtomicBool::new(false));
    let first = decided.clone();
    std::thread::spawn(move || {
        let ok = probe(&req, total);
        if !first.swap(true, Ordering::SeqCst) {
            let _ = post_ui(move || decide(id, ok.then_some(req)));
        }
    });
    post_ui_delayed(PROBE_WAIT_MS, move || {
        if !decided.swap(true, Ordering::SeqCst) {
            decide(id, None);
        }
    });
}

fn decide(id: u64, req: Option<Request>) {
    let Some(p) = PENDING.with(|m| m.borrow_mut().remove(&id)) else { return };
    let dir = Path::new(&p.started.path)
        .parent()
        .map(Path::to_path_buf)
        .filter(|d| d.is_dir())
        .unwrap_or_else(job::download_dir);
    let taken = req.is_some_and(|r| job::start_download(r, dir, p.started.origin.clone()).is_ok());
    if !taken {
        let path = p.started.path.clone();
        let _ = register_download(p.started, &p.shared);
        p.cb.cont(Some(&CefString::from(path.as_str())), 0);
    }
    // When `taken` is true, dropping `p.cb` without calling `cont` cancels the CEF download.
}

fn request(tab: &str, s: &Started, page: &str, user_agent: String) -> Request {
    let name = Path::new(&s.path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".into());
    let p = PathBuf::from(&name);
    let ext = p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if ext.is_empty() {
        name.clone()
    } else {
        p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name)
    };
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

fn probe(req: &Request, total: u64) -> bool {
    let Source::Download { url, .. } = &req.plan.main else { return false };
    let Ok(client) = Client::with_policy(&req.user_agent, job::network_policy(&req.page_url)) else {
        return false;
    };
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
