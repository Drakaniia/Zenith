//! Web requests on the CEF IO thread: full ABP/EasyList network blocking (including service and
//! shared workers via `RequestContextHandler`), CSS-only cosmetic hiding, request counting, and
//! response observation for media sniffing.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use cef::*;

use super::{is_web, TabPatch, TabShared};
use crate::app::runtime::post_ui_delayed;
use crate::host::cef_app::{cef_str, opt_cef_str};
use crate::host::{with_host, BLOCKER};
use crate::platform::urls::{host_of, site_of};

pub static TOTAL_BLOCKED: AtomicU64 = AtomicU64::new(0);

/// No-op in CEF: `on_before_resource_load` checks `blocker_enabled()` atomically on every request.
pub fn apply_blocking(_on: bool) {}

fn resource_type_str(rtype: ResourceType) -> &'static str {
    if rtype == ResourceType::MAIN_FRAME {
        "document"
    } else if rtype == ResourceType::SUB_FRAME {
        "subdocument"
    } else if rtype == ResourceType::STYLESHEET {
        "stylesheet"
    } else if rtype == ResourceType::SCRIPT
        || rtype == ResourceType::WORKER
        || rtype == ResourceType::SHARED_WORKER
        || rtype == ResourceType::SERVICE_WORKER
    {
        "script"
    } else if rtype == ResourceType::IMAGE {
        "image"
    } else if rtype == ResourceType::FONT_RESOURCE {
        "font"
    } else if rtype == ResourceType::OBJECT {
        "object"
    } else if rtype == ResourceType::MEDIA {
        "media"
    } else if rtype == ResourceType::XHR {
        "xmlhttprequest"
    } else if rtype == ResourceType::PING {
        "ping"
    } else {
        "other"
    }
}

fn should_block_url(uri: &str, source_url: &str, page_host: &str, rtype: ResourceType) -> bool {
    if !crate::host::blocker_enabled() {
        return false;
    }
    let Some(req_host) = host_of(uri) else {
        return false;
    };
    let site = site_of(page_host);
    // Same-site subresources and documents are never blocked by default policy.
    if !page_host.is_empty() && site.eq_ignore_ascii_case(site_of(req_host)) {
        return false;
    }
    let rtype_name = resource_type_str(rtype);
    match BLOCKER.read() {
        Ok(b) => {
            if !site.is_empty() && b.disabled_sites.contains(site) {
                return false;
            }
            b.engine.should_block_request(uri, source_url, rtype_name)
        }
        Err(_) => false,
    }
}

/// Injects CSS-only cosmetic hiding rules into a frame without executing filter scriptlets.
pub(super) fn inject_cosmetic_css(frame: &mut Frame, page_url: &str) {
    if !crate::host::blocker_enabled() || !is_web(page_url) {
        return;
    }
    let page_host = host_of(page_url).unwrap_or("").to_ascii_lowercase();
    let site = site_of(&page_host);
    let css = match BLOCKER.read() {
        Ok(b) => {
            if !site.is_empty() && b.disabled_sites.contains(site) {
                None
            } else {
                b.engine.cosmetic_css(page_url)
            }
        }
        Err(_) => None,
    };
    let Some(css) = css else { return };
    let Ok(css_json) = serde_json::to_string(&css) else { return };
    let js = format!(
        "(() => {{ \
            let s = document.querySelector('style[data-zenith-cosmetic]'); \
            if (!s) {{ \
                s = document.createElement('style'); \
                s.setAttribute('data-zenith-cosmetic', '1'); \
                (document.head || document.documentElement)?.appendChild(s); \
            }} \
            s.textContent = {css_json}; \
        }})();"
    );
    frame.execute_java_script(
        Some(&CefString::from(js.as_str())),
        Some(&CefString::from("zenith://internal/cosmetic.js")),
        1,
    );
}

fn schedule_block_flush(s: &Arc<TabShared>) {
    if s.flush_pending.swap(true, Ordering::SeqCst) {
        return;
    }
    let id = s.id();
    post_ui_delayed(350, move || {
        let count = with_host(|h| {
            let t = h.tabs.get(&id)?;
            t.shared.flush_pending.store(false, Ordering::SeqCst);
            Some(t.shared.blocked())
        })
        .flatten();
        if let Some(blocked) = count {
            TabPatch { blocked: Some(blocked), ..TabPatch::new(&id) }.send();
        }
    });
}

wrap_resource_request_handler! {
    pub(super) struct TabResourceRequestHandler {
        shared: Arc<TabShared>,
    }

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let Some(request) = request else {
                return ReturnValue::CONTINUE;
            };
            let uri = cef_str(request.url());
            if !is_web(&uri) {
                return ReturnValue::CONTINUE;
            }
            self.shared.requests.fetch_add(1, Ordering::Relaxed);
            let rtype = request.resource_type();
            if rtype == ResourceType::MEDIA {
                if let Ok(mut media) = self.shared.media.lock() {
                    media.note_media_request(&uri);
                }
            }
            let is_main_doc = rtype == ResourceType::MAIN_FRAME;
            if is_main_doc && self.shared.nav_uri() == uri {
                return ReturnValue::CONTINUE;
            }
            let page_host = self.shared.page_host();
            let source_url = {
                let u = self.shared.url();
                if u.is_empty() { self.shared.nav_uri() } else { u }
            };
            if should_block_url(&uri, &source_url, &page_host, rtype) {
                self.shared.blocked.fetch_add(1, Ordering::Relaxed);
                TOTAL_BLOCKED.fetch_add(1, Ordering::Relaxed);
                schedule_block_flush(&self.shared);
                return ReturnValue::CANCEL;
            }
            ReturnValue::CONTINUE
        }

        fn on_resource_response(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            response: Option<&mut Response>,
        ) -> ::std::os::raw::c_int {
            let (Some(req), Some(resp)) = (request, response) else { return 0 };
            crate::media::sniff::on_cef_response(&self.shared, req, resp);
            if req.resource_type() == ResourceType::MAIN_FRAME && resp.status() < 400 && crate::pdf::is_pdf(&cef_str(resp.mime_type())) {
                open_in_pdf_viewer(browser.map(|b| b.clone()), req);
            }
            0
        }
    }
}

/// Sends a main-frame PDF to Zenith's viewer instead of Chromium's. A local file opens where
/// it is; a web PDF shows the viewer's waiting page while CEF downloads the same address with
/// the page's cookies (see `crate::pdf`).
fn open_in_pdf_viewer(browser: Option<Browser>, req: &Request) {
    let (Some(browser), src) = (browser, cef_str(req.url())) else { return };
    let viewer = if let Some(path) = crate::pdf::file_path(&src) {
        crate::pdf::open(path.clone(), true, &src, &crate::pdf::file_name(&path))
    } else if src.starts_with("https://") || src.starts_with("http://") {
        let Some(waiting) = crate::pdf::request(&src) else { return };
        waiting
    } else {
        return;
    };
    crate::host::post(move || {
        if let Some(frame) = browser.main_frame() {
            frame.load_url(Some(&CefString::from(viewer.as_str())));
        }
        if !src.starts_with("file:") {
            if let Some(host) = browser.host() {
                host.start_download(Some(&CefString::from(src.as_str())));
            }
        }
    });
}

/// Unsolicited Chromium vendor background endpoints blocked when no user page browser owns the
/// request (Section 10.3). Ordinary user navigations in tabs are never affected.
fn is_unsolicited_vendor_bg(uri: &str) -> bool {
    let Some(host) = host_of(uri) else {
        return false;
    };
    let h = host.to_ascii_lowercase();
    matches!(
        h.as_str(),
        "update.googleapis.com"
            | "edgedl.me.gvt1.com"
            | "redirector.gvt1.com"
            | "chrome.cloudflare-dns.com"
            | "clients2.google.com"
            | "optimizationguide-pa.googleapis.com"
    ) || (h == "www.google.com" && uri.contains("/async/folae"))
}

// Request-context-level blocker covering service workers, shared workers, and background fetches
// that have no associated `Browser` instance.
wrap_resource_request_handler! {
    struct WorkerResourceRequestHandler {
        initiator: String,
    }

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let Some(request) = request else {
                return ReturnValue::CONTINUE;
            };
            let uri = cef_str(request.url());
            if !is_web(&uri) {
                return ReturnValue::CONTINUE;
            }
            if is_unsolicited_vendor_bg(&uri) {
                return ReturnValue::CANCEL;
            }
            let page_host = host_of(&self.initiator).unwrap_or("").to_ascii_lowercase();
            let rtype = request.resource_type();
            if should_block_url(&uri, &self.initiator, &page_host, rtype) {
                TOTAL_BLOCKED.fetch_add(1, Ordering::Relaxed);
                return ReturnValue::CANCEL;
            }
            ReturnValue::CONTINUE
        }
    }
}

wrap_request_context_handler! {
    struct ZenithRequestContextHandler;

    impl RequestContextHandler {
        fn resource_request_handler(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            // Only intercept requests not owned by a page browser (workers & Chromium bg tasks).
            if browser.is_some() {
                return None;
            }
            let uri = request.as_ref().map(|r| cef_str(r.url())).unwrap_or_default();
            if is_unsolicited_vendor_bg(&uri) {
                return Some(WorkerResourceRequestHandler::new(String::new()));
            }
            let rtype = request.as_ref().map(|r| r.resource_type());
            let is_worker_or_bg = rtype.is_none_or(|rt| {
                matches!(
                    rt,
                    ResourceType::SERVICE_WORKER
                        | ResourceType::SHARED_WORKER
                        | ResourceType::WORKER
                        | ResourceType::XHR
                        | ResourceType::SCRIPT
                        | ResourceType::PING
                        | ResourceType::IMAGE
                        | ResourceType::SUB_RESOURCE
                        | ResourceType::PREFETCH
                )
            });
            if !is_worker_or_bg {
                return None;
            }
            let initiator = opt_cef_str(request_initiator);
            Some(WorkerResourceRequestHandler::new(initiator))
        }
    }
}

pub fn create_request_context_handler() -> RequestContextHandler {
    ZenithRequestContextHandler::new()
}
