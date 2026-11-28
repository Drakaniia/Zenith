//! CEF side of media detection: watches each tab's HTTP responses on the IO thread, fetches
//! and parses HLS/DASH manifests off-thread, scans pages via DevTools `Runtime.evaluate`, and
//! queries CEF's `CookieManager` for authenticated media downloads.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cef::*;
use serde::{Deserialize, Serialize};

use super::detect::{self, Sniffed};
use super::{dash, hls, Found, Scan};
use crate::app::runtime::{post_ui, post_ui_delayed};
use crate::host::cef_app::cef_str;
use crate::host::tab::TabShared;
use crate::host::{config, emit_tab, eval_js, with_host};
use crate::platform::urls::{host_of, site_of};

const MAX_MANIFEST: usize = 16 << 20;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MediaChanged {
    tab_id: String,
    count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaAnchor {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub dpr: f64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MediaRequest {
    tab_id: String,
    hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor: Option<MediaAnchor>,
}

pub fn notify(shared: &TabShared) {
    if shared.is_warm() {
        return;
    }
    let id = shared.id();
    let count = shared.media.lock().map(|m| m.count()).unwrap_or(0);
    emit_tab(&id, "media-changed", MediaChanged { tab_id: id.clone(), count });
}

pub fn request_panel(tab_id: &str, hint: &str) {
    let hint = if hint.len() > 4096 { String::new() } else { hint.to_string() };
    emit_tab(tab_id, "media-request", MediaRequest { tab_id: tab_id.to_string(), hint, anchor: None });
}

#[derive(Deserialize)]
struct ButtonSignal {
    #[serde(default)]
    src: String,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    w: f64,
    #[serde(default)]
    h: f64,
    #[serde(default)]
    dpr: f64,
}

/// Decodes the `_zenith_media` payload from the in-page hover button (with video rect anchor),
/// falling back to a plain hint URL.
pub fn parse_button_signal(uri: &str) -> (String, Option<MediaAnchor>) {
    if let Some(frag) = uri.strip_prefix("https://zenith.invalid/#") {
        if frag.len() <= 8192 {
            if let Some(decoded) = percent_decode(frag) {
                if let Ok(sig) = serde_json::from_str::<ButtonSignal>(&decoded) {
                    let hint = if sig.src.len() <= 4096 { sig.src } else { String::new() };
                    let anchor = (sig.w > 0.0 && sig.h > 0.0 && sig.dpr > 0.0 && sig.x.is_finite() && sig.y.is_finite())
                        .then_some(MediaAnchor { x: sig.x, y: sig.y, w: sig.w, h: sig.h, dpr: sig.dpr });
                    return (hint, anchor);
                }
            }
        }
        return (String::new(), None);
    }
    let hint = if uri.len() > 4096 { String::new() } else { uri.to_string() };
    (hint, None)
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hi = (*b.get(i + 1)? as char).to_digit(16)?;
            let lo = (*b.get(i + 2)? as char).to_digit(16)?;
            out.push(((hi << 4) | lo) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn request_from_button(tab_id: &str, uri: &str) {
    let (hint, anchor) = parse_button_signal(uri);
    emit_tab(tab_id, "media-request", MediaRequest { tab_id: tab_id.to_string(), hint, anchor });
}

pub fn default_user_agent() -> String {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36".into()
}

pub fn button_allowed_for_url(url: &str) -> bool {
    let cfg = config();
    if !cfg.media_button {
        return false;
    }
    let Some(host) = host_of(url) else {
        return true;
    };
    let h = host.trim_start_matches("www.").to_ascii_lowercase();
    !cfg.media_disabled_sites
        .iter()
        .any(|s| s.trim().trim_start_matches("www.").eq_ignore_ascii_case(&h))
}

/// Immediately hides or shows the in-page video download button on all open tabs when settings change.
pub fn apply_button_setting(_enabled: bool) {
    let tabs: Vec<(Browser, String)> = with_host(|h| {
        h.tabs
            .values()
            .filter_map(|t| Some((t.browser.clone()?, t.shared.url())))
            .collect()
    })
    .unwrap_or_default();
    for (browser, url) in tabs {
        let off = !button_allowed_for_url(&url);
        let js = format!(
            "(() => {{ window.__zenithMediaBtnOff = {off}; if ({off}) {{ const el = document.querySelector('zenith-media-button'); if (el) el.style.setProperty('display', 'none', 'important'); }} }})()"
        );
        eval_js(&browser, &js, false, false, |_| {});
    }
}

/// Syncs the per-site button disabled flag when a tab finishes loading.
pub fn sync_frame_button(frame: &mut Frame, url: &str) {
    if !button_allowed_for_url(url) {
        let js = "window.__zenithMediaBtnOff = true; (() => { const el = document.querySelector('zenith-media-button'); if (el) el.style.setProperty('display', 'none', 'important'); })();";
        frame.execute_java_script(
            Some(&CefString::from(js)),
            Some(&CefString::from("zenith://internal/media-btn-sync.js")),
            1,
        );
    }
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
        tab: shared.id(),
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
                tab: shared.id(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_button_signal_with_anchor_and_plain_url() {
        let (h1, a1) = parse_button_signal("https://cdn.example/v.mp4");
        assert_eq!(h1, "https://cdn.example/v.mp4");
        assert_eq!(a1, None);

        let uri = "https://zenith.invalid/#%7B%22src%22%3A%22https%3A%2F%2Fa.test%2Fv.mp4%22%2C%22x%22%3A10%2C%22y%22%3A20%2C%22w%22%3A640%2C%22h%22%3A360%2C%22dpr%22%3A1.5%7D";
        let (h2, a2) = parse_button_signal(uri);
        assert_eq!(h2, "https://a.test/v.mp4");
        assert_eq!(a2, Some(MediaAnchor { x: 10.0, y: 20.0, w: 640.0, h: 360.0, dpr: 1.5 }));
    }
}
