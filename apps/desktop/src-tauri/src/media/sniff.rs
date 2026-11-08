//! WebView2 side of media detection: watches each tab's network responses, reads stream
//! manifests, scans pages on demand, manages the in-page download button, and hands the
//! profile's cookies to download threads.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, HSTRING, PWSTR};
use windows::Win32::System::Com::IStream;

use super::detect::{self, Sniffed};
use super::{dash, hls, Found, Scan};
use crate::host::tab::TabShared;
use crate::host::{app, emit_tab, with_host};
use crate::urls::{host_of, site_of};

/// Largest manifest read (DASH SegmentLists of long films run to a few MB).
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
    /// Address of the media the user pointed at (may be a `blob:` URL or the page).
    hint: String,
}

/// Tells the UI how many items a tab has.
pub fn notify(shared: &TabShared) {
    let count = shared.media.borrow().count();
    emit_tab(&shared.id, "media-changed", MediaChanged { tab_id: shared.id.clone(), count });
}

/// Asks the UI to open the media panel (download button on a video, context menu). The hint
/// comes from the page; an oversized one is dropped.
pub fn request_panel(tab_id: &str, hint: &str) {
    let hint = if hint.len() > 4096 { String::new() } else { hint.to_string() };
    emit_tab(tab_id, "media-request", MediaRequest { tab_id: tab_id.to_string(), hint });
}

fn read_stream(stream: &IStream, max: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut read = 0u32;
        let hr = unsafe { stream.Read(buf.as_mut_ptr() as *mut _, buf.len() as u32, Some(&mut read)) };
        if hr.is_err() {
            return None;
        }
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
        if out.len() > max {
            return None;
        }
    }
    Some(out)
}

unsafe fn response_header(h: &ICoreWebView2HttpResponseHeaders, name: &str) -> Option<String> {
    unsafe {
        let mut v = PWSTR::null();
        h.GetHeader(&HSTRING::from(name), &mut v).ok()?;
        let s = take_pwstr(v);
        (!s.is_empty()).then_some(s)
    }
}

unsafe fn request_header(h: &ICoreWebView2HttpRequestHeaders, name: &str) -> Option<String> {
    unsafe {
        let mut v = PWSTR::null();
        h.GetHeader(&HSTRING::from(name), &mut v).ok()?;
        let s = take_pwstr(v);
        (!s.is_empty()).then_some(s)
    }
}

/// A manifest waiting to be read and parsed.
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

/// Adds a parsed manifest to its tab (UI thread). Results for a page that has since navigated
/// away are dropped.
fn register(p: &PendingManifest, parsed: Option<Parsed>) {
    let Some(shared) = with_host(|h| h.tabs.get(&p.tab).map(|t| t.shared.clone())).flatten() else { return };
    let changed = {
        let mut media = shared.media.borrow_mut();
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

/// Parses on a worker thread (a hostile page can serve huge manifests) and registers the result
/// on the UI thread.
fn parse_later(p: PendingManifest, base: String, text: String) {
    std::thread::spawn(move || {
        let parsed = parse_manifest(p.kind, &text, &base);
        let _ = app().run_on_main_thread(move || register(&p, parsed));
    });
}

/// Fetches a manifest directly (when WebView2 cannot hand over the body, or the page scan found
/// it), parses it, and registers it on the UI thread.
fn fetch_manifest(p: PendingManifest) {
    std::thread::spawn(move || {
        let ua = p.headers.iter().find(|(k, _)| k == "User-Agent").map(|(_, v)| v.clone()).unwrap_or_default();
        let site = host_of(&p.page).map(|h| site_of(&h.to_ascii_lowercase()).to_string()).unwrap_or_default();
        let base: Vec<(String, String)> = p.headers.iter().filter(|(k, _)| k != "User-Agent").cloned().collect();
        // Only a playlist the page itself loaded is fetched with the site's cookies; an address
        // merely written in the page (og:video, <source>) is fetched without them. Cookies are
        // looked up per redirect hop, so they only go to the host they belong to.
        let with_cookies = p.by == "network";
        let headers = |url: &str| {
            let mut h = base.clone();
            if with_cookies {
                if let Some(cookie) = cookie_header(url, &site) {
                    h.push(("Cookie".into(), cookie));
                }
            }
            h
        };
        let parsed = super::http::Client::with_policy(&ua, super::job::network_policy(&p.page))
            .and_then(|c| c.text(&p.uri, &headers, MAX_MANIFEST))
            .ok()
            .and_then(|(text, final_url)| parse_manifest(p.kind, &text, &final_url));
        let _ = app().run_on_main_thread(move || register(&p, parsed));
    });
}

/// Watches the tab's responses for media. Called once per tab webview.
pub fn attach(core: &ICoreWebView2, shared: Rc<TabShared>) {
    let Ok(c2) = core.cast::<ICoreWebView2_2>() else { return };
    let mut token = 0i64;
    unsafe {
        let _ = c2.add_WebResourceResponseReceived(
            &WebResourceResponseReceivedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
                if !(uri.starts_with("https://") || uri.starts_with("http://")) || uri.len() > 4096 {
                    return Ok(());
                }
                let response = args.Response()?;
                let mut status = 0i32;
                response.StatusCode(&mut status)?;
                if !(200..300).contains(&status) {
                    return Ok(());
                }
                let headers = response.Headers()?;
                let content_type = response_header(&headers, "Content-Type").unwrap_or_default();
                let total = if status == 206 {
                    response_header(&headers, "Content-Range").and_then(|r| detect::content_range_total(&r))
                } else {
                    response_header(&headers, "Content-Length").and_then(|l| l.trim().parse().ok())
                };
                let media_context = shared.media.borrow().is_media_request(&uri);
                let Some(kind) = detect::classify(&uri, &content_type, media_context, status as u16, total) else {
                    return Ok(());
                };
                if kind != Sniffed::File && !shared.media.borrow().wants_manifest(&uri) {
                    return Ok(());
                }
                if kind != Sniffed::File && !shared.media.borrow_mut().begin_manifest(&uri) {
                    return Ok(()); // this page already had its share of manifests read
                }
                let mut captured = Vec::new();
                if let Ok(h) = request.Headers() {
                    for name in ["Referer", "Origin", "User-Agent"] {
                        if let Some(v) = request_header(&h, name) {
                            captured.push((name.to_string(), v));
                        }
                    }
                }
                let page = shared.url.borrow().clone();
                let title = shared.title.borrow().clone();
                if kind == Sniffed::File {
                    let found = Found { by: "network", headers: captured, page_url: &page, title: &title };
                    let changed = shared.media.borrow_mut().add_file(&uri, &content_type, total, found);
                    if changed {
                        notify(&shared);
                    }
                    return Ok(());
                }
                let pending = PendingManifest {
                    tab: shared.id.clone(),
                    epoch: shared.media.borrow().epoch,
                    uri,
                    kind,
                    by: "network",
                    headers: captured,
                    page,
                    title,
                };
                let p = pending.clone();
                let handler = WebResourceResponseViewGetContentCompletedHandler::create(Box::new(move |result, stream| {
                    let body = result.ok().and(stream).and_then(|s| read_stream(&s, MAX_MANIFEST));
                    match body.filter(|b| !b.is_empty()) {
                        Some(b) => {
                            let base = p.uri.clone();
                            parse_later(p, base, String::from_utf8_lossy(&b).into_owned());
                        }
                        None => fetch_manifest(p),
                    }
                    Ok(())
                }));
                if response.GetContent(&handler).is_err() {
                    fetch_manifest(pending);
                }
                Ok(())
            })),
            &mut token,
        );
    }
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

/// Searches the page for media elements and `og:video` tags, then calls `done` (after at most
/// 1.5 s even if the page does not answer).
pub fn scan(tab_id: &str, done: impl FnOnce() + 'static) {
    let tab = with_host(|h| h.tabs.get(tab_id).map(|t| (t.core(), t.shared.clone()))).flatten();
    let Some((core, shared)) = tab else {
        done();
        return;
    };
    let token = NEXT_SCAN.fetch_add(1, Ordering::Relaxed);
    PENDING_SCANS.with(|p| p.borrow_mut().insert(token, Box::new(done)));
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        let _ = app().run_on_main_thread(move || finish_scan(token));
    });
    let epoch = shared.media.borrow().epoch;
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
        let scan: Scan = result.ok().and_then(|_| serde_json::from_str(&json).ok()).unwrap_or_default();
        let page = shared.url.borrow().clone();
        let title = shared.title.borrow().clone();
        let page_private = host_of(&page).is_some_and(detect::is_private_host);
        let (changed, fetch) = {
            let mut media = shared.media.borrow_mut();
            if media.epoch != epoch {
                (false, Vec::new())
            } else {
                let (changed, mut fetch) = media.apply_scan(&scan, &page, &title);
                // A public page may not point Zenith at the local network.
                fetch.retain(|(url, _)| page_private || !host_of(url).is_some_and(detect::is_private_host));
                fetch.retain(|(url, _)| media.begin_manifest(url));
                (changed, fetch)
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
        Ok(())
    }));
    unsafe {
        if core.ExecuteScript(&HSTRING::from(detect::SCAN_SCRIPT), &handler).is_err() {
            finish_scan(token);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Download button script

/// Adds the in-page download button. With `wait`, returns only once WebView2 registered it, so
/// it applies to the very first navigation (used while creating a tab).
pub fn install_button(core: &ICoreWebView2, shared: &Rc<TabShared>, wait: bool) {
    if shared.button_script.borrow().is_some() {
        return;
    }
    let js = HSTRING::from(detect::BUTTON_SCRIPT);
    if wait {
        let id = Rc::new(RefCell::new(None));
        let id2 = id.clone();
        let core2 = core.clone();
        let r = AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| unsafe { core2.AddScriptToExecuteOnDocumentCreated(&js, &handler).map_err(Into::into) }),
            Box::new(move |result, script_id| {
                if result.is_ok() {
                    *id2.borrow_mut() = Some(script_id);
                }
                Ok(())
            }),
        );
        if let Err(e) = r {
            eprintln!("[zenith] media button script failed: {e:?}");
        }
        *shared.button_script.borrow_mut() = id.borrow_mut().take();
    } else {
        let sh = shared.clone();
        let handler = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(move |result, script_id| {
            if result.is_ok() {
                *sh.button_script.borrow_mut() = Some(script_id);
            }
            Ok(())
        }));
        unsafe {
            let _ = core.AddScriptToExecuteOnDocumentCreated(&js, &handler);
        }
    }
}

fn remove_button(core: &ICoreWebView2, shared: &TabShared) {
    if let Some(id) = shared.button_script.borrow_mut().take() {
        unsafe {
            let _ = core.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(id));
        }
    }
}

/// Applies the "download button on videos" setting to every live tab (from its next page load).
pub fn apply_button_setting(enabled: bool) {
    let tabs: Vec<(ICoreWebView2, Rc<TabShared>)> =
        with_host(|h| h.tabs.values().map(|t| (t.core(), t.shared.clone())).collect()).unwrap_or_default();
    for (core, shared) in tabs {
        if enabled {
            install_button(&core, &shared, false);
        } else {
            remove_button(&core, &shared);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Cookies and user agent for downloads

unsafe fn cookie_string(list: &ICoreWebView2CookieList, same_site: bool) -> String {
    unsafe {
        let mut count = 0u32;
        let _ = list.Count(&mut count);
        let mut parts = Vec::new();
        for i in 0..count.min(300) {
            let Ok(c) = list.GetValueAtIndex(i) else { continue };
            let mut kind = COREWEBVIEW2_COOKIE_SAME_SITE_KIND::default();
            let _ = c.SameSite(&mut kind);
            // A navigation from another site would not carry Strict cookies either.
            if kind == COREWEBVIEW2_COOKIE_SAME_SITE_KIND_STRICT && !same_site {
                continue;
            }
            let (mut name, mut value) = (PWSTR::null(), PWSTR::null());
            let ok_name = c.Name(&mut name).is_ok();
            let ok_value = c.Value(&mut value).is_ok();
            let (name, value) = (take_pwstr(name), take_pwstr(value));
            if ok_name && ok_value && !name.is_empty() {
                parts.push(format!("{name}={value}"));
            }
        }
        parts.join("; ")
    }
}

/// The `Cookie` header the browser would send to `url`. Called from download threads; it hops to
/// the UI thread (WebView2 is single-threaded) and waits at most 5 s.
pub fn cookie_header(url: &str, page_site: &str) -> Option<String> {
    let target = host_of(url).map(|h| site_of(&h.to_ascii_lowercase()).to_string())?;
    let same_site = !page_site.is_empty() && target == page_site;
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    let url = url.to_string();
    app()
        .run_on_main_thread(move || {
            // The user's profile (every chrome webview uses it).
            let manager = with_host(|h| {
                let core = h.wins.values().next()?.chrome_core.clone();
                unsafe { core.cast::<ICoreWebView2_2>().and_then(|c| c.CookieManager()) }.ok()
            })
            .flatten();
            let Some(manager) = manager else {
                let _ = tx.send(None);
                return;
            };
            let tx2 = tx.clone();
            let handler = GetCookiesCompletedHandler::create(Box::new(move |result, list| {
                let header = match (result, list) {
                    (Ok(()), Some(list)) => Some(unsafe { cookie_string(&list, same_site) }),
                    _ => None,
                };
                let _ = tx2.send(header);
                Ok(())
            }));
            if unsafe { manager.GetCookies(&HSTRING::from(url), &handler) }.is_err() {
                let _ = tx.send(None);
            }
        })
        .ok()?;
    rx.recv_timeout(Duration::from_secs(5)).ok().flatten().filter(|c| !c.is_empty())
}

/// The webview's User-Agent string (CDNs sometimes check it).
pub fn user_agent(core: &ICoreWebView2) -> String {
    unsafe {
        core.Settings()
            .ok()
            .and_then(|s| s.cast::<ICoreWebView2Settings2>().ok())
            .and_then(|s| {
                let mut p = PWSTR::null();
                s.UserAgent(&mut p).ok()?;
                Some(take_pwstr(p))
            })
            .unwrap_or_default()
    }
}
