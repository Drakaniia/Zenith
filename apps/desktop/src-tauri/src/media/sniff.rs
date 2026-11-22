//! CEF side of media detection: watches each tab's HTTP responses on the IO thread, fetches
//! and parses HLS/DASH manifests off-thread, scans pages via DevTools `Runtime.evaluate`, and
//! queries CEF's `CookieManager` for authenticated media downloads.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cef::*;
use serde::Serialize;

use super::detect::{self, Sniffed};
use super::{dash, hls, Found, Scan};
use crate::app::runtime::{post_ui, post_ui_delayed};
use crate::host::cef_app::cef_str;
use crate::host::tab::TabShared;
use crate::host::{emit_tab, eval_js, with_host};
use crate::platform::urls::{host_of, site_of};

const MAX_MANIFEST: usize = 16 << 20;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MediaChanged {
    tab_id: String,
    count: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MediaRequest {
    tab_id: String,
    hint: String,
}

pub fn notify(shared: &TabShared) {
    let count = shared.media.lock().map(|m| m.count()).unwrap_or(0);
    emit_tab(&shared.id, "media-changed", MediaChanged { tab_id: shared.id.clone(), count });
}

pub fn request_panel(tab_id: &str, hint: &str) {
    let hint = if hint.len() > 4096 { String::new() } else { hint.to_string() };
    emit_tab(tab_id, "media-request", MediaRequest { tab_id: tab_id.to_string(), hint });
}

pub fn default_user_agent() -> String {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36".into()
}

pub fn apply_button_setting(_enabled: bool) {
    // Applied automatically to newly created tabs via `extra_info` in `host::tab::create`.
}

#[derive(Clone)]
struct PendingManifest {
    tab: String,
    epoch: u64,
    uri: String,
    kind: Sniffed,
    by: &'static str,
    headers: Vec<(String, String)>,
    page: String,
    title: String,
}

enum Parsed {
    Hls(hls::Playlist),
    Dash(dash::Mpd),
}

fn parse_manifest(kind: Sniffed, text: &str, base: &str) -> Option<Parsed> {
    match kind {
        Sniffed::Hls => hls::parse(text, base).ok().map(Parsed::Hls),
        Sniffed::Dash => dash::parse(text, base).ok().map(Parsed::Dash),
        Sniffed::File => None,
    }
}

fn register(p: &PendingManifest, parsed: Option<Parsed>) {
    let Some(shared) = with_host(|h| h.tabs.get(&p.tab).map(|t| t.shared.clone())).flatten() else {
        return;
    };
    let changed = {
        let Ok(mut media) = shared.media.lock() else { return };
        if media.epoch != p.epoch {
            return;
        }
        media.pending.remove(&p.uri);
        let found = Found { by: p.by, headers: p.headers.clone(), page_url: &p.page, title: &p.title };
        match parsed {
            Some(Parsed::Hls(pl)) => media.add_hls(&p.uri, pl, found),
            Some(Parsed::Dash(mpd)) => media.add_dash(&p.uri, mpd, found),
            None => false,
        }
    };
    if changed {
        notify(&shared);
    }
}

fn fetch_manifest(p: PendingManifest) {
    std::thread::spawn(move || {
        let ua = p
            .headers
            .iter()
            .find(|(k, _)| k == "User-Agent")
            .map(|(_, v)| v.clone())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(default_user_agent);
        let site = host_of(&p.page)
            .map(|h| site_of(&h.to_ascii_lowercase()).to_string())
            .unwrap_or_default();
        let base: Vec<(String, String)> = p.headers.iter().filter(|(k, _)| k != "User-Agent").cloned().collect();
        let with_cookies = p.by == "network";
        let headers = |url: &str| {
            let mut h = base.clone();
            if with_cookies {
                if let Some(c) = cookie_header(url, &site) {
                    h.push(("Cookie".into(), c));
                }
            }
            h
        };
        let policy = super::job::network_policy(&p.page);
        // `text` keeps the client alive while reading: dropping it closes the session and cancels the read.
        let parsed = super::http::Client::with_policy(&ua, policy)
            .and_then(|c| c.text(&p.uri, &headers, MAX_MANIFEST))
            .ok()
            .and_then(|(text, final_url)| parse_manifest(p.kind, &text, &final_url));
        let _ = post_ui(move || register(&p, parsed));
    });
}

/// Called from CEF `ResourceRequestHandler::on_resource_response` on the IO thread.
pub fn on_cef_response(shared: &Arc<TabShared>, req: &mut Request, resp: &mut Response) {
    let status = resp.status();
    if !(200..300).contains(&status) {
        return;
    }
    let uri = cef_str(req.url());
    if !uri.starts_with("http://") && !uri.starts_with("https://") {
        return;
    }
    let content_type = cef_str(resp.header_by_name(Some(&CefString::from("content-type"))));
    let content_length = cef_str(resp.header_by_name(Some(&CefString::from("content-length"))))
        .parse::<u64>()
        .ok();
    let content_range = cef_str(resp.header_by_name(Some(&CefString::from("content-range"))));
    let total = detect::content_range_total(&content_range).or(content_length);

    let (media_context, wants_and_began, epoch) = {
        let Ok(mut media) = shared.media.lock() else { return };
        let mc = media.is_media_request(&uri);
        let kind = match detect::classify(&uri, &content_type, mc, status as u16, total) {
            Some(k) => k,
            None => return,
        };
        let ok = kind == Sniffed::File || (media.wants_manifest(&uri) && media.begin_manifest(&uri));
        (mc, (ok, kind), media.epoch)
    };
    let _ = media_context;
    let (ok, kind) = wants_and_began;
    if !ok {
        return;
    }

    let mut headers = Vec::new();
    for name in ["Referer", "Origin", "User-Agent"] {
        let v = cef_str(req.header_by_name(Some(&CefString::from(name))));
        if !v.is_empty() {
            headers.push((name.to_string(), v));
        }
    }
    let page = {
        let u = shared.url();
        if u.is_empty() { shared.nav_uri() } else { u }
    };
    let title = shared.title();
    if kind == Sniffed::File {
        let changed = shared
            .media
            .lock()
            .map(|mut m| {
                let found = Found { by: "network", headers, page_url: &page, title: &title };
                m.add_file(&uri, &content_type, total, found)
            })
            .unwrap_or(false);
        if changed {
            notify(shared);
        }
        return;
    }

    fetch_manifest(PendingManifest {
        tab: shared.id.clone(),
        epoch,
        uri,
        kind,
        by: "network",
        headers,
        page,
        title,
    });
}

// ---------------------------------------------------------------------------------------------
// Page scan

type ScanDone = Box<dyn FnOnce()>;

thread_local! {
    static PENDING_SCANS: RefCell<HashMap<u64, ScanDone>> = RefCell::new(HashMap::new());
}
static NEXT_SCAN: AtomicU64 = AtomicU64::new(1);

fn finish_scan(token: u64) {
    if let Some(done) = PENDING_SCANS.with(|p| p.borrow_mut().remove(&token)) {
        done();
    }
}

pub fn scan(tab_id: &str, done: impl FnOnce() + 'static) {
    let tab = with_host(|h| {
        let t = h.tabs.get(tab_id)?;
        Some((t.browser.clone()?, t.shared.clone()))
    })
    .flatten();
    let Some((browser, shared)) = tab else {
        done();
        return;
    };
    let token = NEXT_SCAN.fetch_add(1, Ordering::Relaxed);
    PENDING_SCANS.with(|p| p.borrow_mut().insert(token, Box::new(done)));
    post_ui_delayed(1500, move || finish_scan(token));

    let epoch = shared.media.lock().map(|m| m.epoch).unwrap_or(0);
    eval_js(&browser, detect::SCAN_SCRIPT, false, false, move |res| {
        let scan: Scan = res
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let page = shared.url();
        let title = shared.title();
        let page_private = host_of(&page).is_some_and(detect::is_private_host);
        let (changed, fetch) = {
            match shared.media.lock() {
                Ok(mut media) if media.epoch == epoch => {
                    let (changed, mut fetch) = media.apply_scan(&scan, &page, &title);
                    fetch.retain(|(url, _)| page_private || !host_of(url).is_some_and(detect::is_private_host));
                    fetch.retain(|(url, _)| media.begin_manifest(url));
                    (changed, fetch)
                }
                _ => (false, Vec::new()),
            }
        };
        for (uri, kind) in fetch {
            fetch_manifest(PendingManifest {
                tab: shared.id.clone(),
                epoch,
                uri,
                kind,
                by: "page",
                headers: vec![("Referer".into(), page.clone())],
                page: page.clone(),
                title: title.clone(),
            });
        }
        if changed {
            notify(&shared);
        }
        finish_scan(token);
    });
}

// ---------------------------------------------------------------------------------------------
// Cookies for authenticated media downloads via CEF CookieManager

struct CookieDone {
    parts: Arc<Mutex<Vec<String>>>,
    tx: Mutex<Option<std::sync::mpsc::Sender<Option<String>>>>,
}

impl CookieDone {
    fn finish(&self) {
        if let Ok(mut tx) = self.tx.lock() {
            if let Some(sender) = tx.take() {
                let joined = self.parts.lock().ok().map(|p| p.join("; "));
                let _ = sender.send(joined);
            }
        }
    }
}

impl Drop for CookieDone {
    fn drop(&mut self) {
        self.finish();
    }
}

wrap_cookie_visitor! {
    struct HeaderCookieVisitor {
        same_site: bool,
        done: Arc<CookieDone>,
    }

    impl CookieVisitor {
        fn visit(
            &self,
            cookie: Option<&Cookie>,
            count: ::std::os::raw::c_int,
            total: ::std::os::raw::c_int,
            _delete_cookie: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if let Some(c) = cookie {
                let strict = c.same_site == CookieSameSite::STRICT_MODE;
                if !strict || self.same_site {
                    let name = c.name.to_string();
                    let value = c.value.to_string();
                    if !name.is_empty() {
                        if let Ok(mut parts) = self.done.parts.lock() {
                            if parts.len() < 300 {
                                parts.push(format!("{name}={value}"));
                            }
                        }
                    }
                }
            }
            if count + 1 >= total {
                self.done.finish();
            }
            1
        }
    }
}

pub fn cookie_header(url: &str, page_site: &str) -> Option<String> {
    let target = host_of(url).map(|h| site_of(&h.to_ascii_lowercase()).to_string())?;
    let same_site = !page_site.is_empty() && target == page_site;
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    let url = url.to_string();
    post_ui(move || {
        let done = Arc::new(CookieDone {
            parts: Arc::new(Mutex::new(Vec::new())),
            tx: Mutex::new(Some(tx)),
        });
        let Some(ctx) = request_context_get_global_context() else {
            return;
        };
        let Some(cm) = ctx.cookie_manager(None) else {
            return;
        };
        let mut visitor = HeaderCookieVisitor::new(same_site, done);
        let _ = cm.visit_url_cookies(Some(&CefString::from(url.as_str())), 1, Some(&mut visitor));
    })
    .ok()?;
    rx.recv_timeout(Duration::from_secs(3)).ok().flatten().filter(|c| !c.is_empty())
}
