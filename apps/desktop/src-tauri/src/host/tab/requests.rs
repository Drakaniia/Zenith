//! Web requests of a tab: content blocking, request counts and media request notes.
//!
//! WebView2 holds every intercepted request until its handler has run on the UI thread, so
//! interception is kept as narrow as possible: requests of pages and their frames only (never
//! service or shared workers, whose requests would stall behind a busy UI thread or a slept
//! tab), every resource type only while the blocker is on, and otherwise just media requests
//! (the media downloader's hint).

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{HSTRING, PWSTR};
use windows::Win32::System::Com::IStream;

use super::{is_web, take, TabPatch, TabShared};
use crate::host::{app, with_host, BLOCKER};
use crate::platform::urls::{host_of, site_of};

pub static TOTAL_BLOCKED: AtomicU64 = AtomicU64::new(0);

/// Adds or removes the all-requests filter the blocker needs. Idempotent: the filter is removed
/// first so it is never registered twice. The plain `AddWebResourceRequestedFilter` covers
/// document requests only (pages and frames), not service or shared workers.
unsafe fn set_block_filter(core: &ICoreWebView2, on: bool) {
    unsafe {
        let filter = HSTRING::from("*");
        let _ = core.RemoveWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL);
        if on {
            let _ = core.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL);
        }
    }
}

/// Applies a change of the blocker setting to every open tab. Runs on the UI thread.
pub fn apply_blocking(on: bool) {
    let cores: Vec<ICoreWebView2> =
        with_host(|h| h.tabs.values().chain(h.zombies.values()).map(|t| t.core()).collect()).unwrap_or_default();
    for core in cores {
        unsafe { set_block_filter(&core, on) };
    }
}

pub(super) unsafe fn attach_blocker(core: &ICoreWebView2, env: &ICoreWebView2Environment, s: Rc<TabShared>) {
    unsafe {
        // Media requests are always watched (few and cheap); everything else only while blocking.
        let _ = core.AddWebResourceRequestedFilter(&HSTRING::from("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA);
        set_block_filter(core, crate::host::blocker_enabled());
        let env = env.clone();
        let mut token = 0i64;
        let _ = core.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&mut uri)?;
                let uri = take(uri);
                if !is_web(&uri) {
                    return Ok(());
                }
                s.requests.set(s.requests.get().saturating_add(1));
                let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
                args.ResourceContext(&mut context)?;
                if context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA {
                    // Lets the media downloader tell <video>/<audio> files from stream segments.
                    s.media.borrow_mut().note_media_request(&uri);
                }
                if !crate::host::blocker_enabled() {
                    return Ok(());
                }
                if context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT && *s.nav_uri.borrow() == uri {
                    return Ok(()); // the top-level navigation itself
                }
                let Some(request_host) = host_of(&uri) else { return Ok(()) };
                let blocked = {
                    let page = s.page_host.borrow();
                    let page_host = (!page.is_empty()).then_some(page.as_str());
                    // Pages, frames and downloads (all document requests) only meet whole-host rules.
                    let document = context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT;
                    match BLOCKER.read() {
                        Ok(b) => {
                            !b.disabled_sites.contains(site_of(&page))
                                && if document {
                                    b.engine.should_block_document(request_host, page_host)
                                } else {
                                    b.engine.should_block(request_host, page_host)
                                }
                        }
                        Err(_) => false,
                    }
                };
                if blocked {
                    let response = env.CreateWebResourceResponse(
                        None::<&IStream>,
                        403,
                        &HSTRING::from("Blocked by Zenith"),
                        &HSTRING::from(""),
                    )?;
                    args.SetResponse(&response)?;
                    s.blocked.set(s.blocked.get() + 1);
                    TOTAL_BLOCKED.fetch_add(1, Ordering::Relaxed);
                    schedule_block_flush(&s);
                }
                Ok(())
            })),
            &mut token,
        );
    }
}

/// Coalesces blocked-request counts into at most ~3 UI updates per second per tab.
fn schedule_block_flush(s: &Rc<TabShared>) {
    if s.flush_pending.replace(true) {
        return;
    }
    let id = s.id.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(350));
        let _ = app().run_on_main_thread(move || {
            with_host(|h| {
                if let Some(t) = h.tabs.get(&id) {
                    t.shared.flush_pending.set(false);
                    TabPatch { blocked: Some(t.shared.blocked.get()), ..TabPatch::new(&id) }.send();
                }
            });
        });
    });
}
