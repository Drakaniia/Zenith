//! New-window requests from a page: Zenith's own `window.open` messages (autofill, media,
//! extension installs, Glance and background tabs), blocked popups and real popups, which get a
//! fresh tab webview so the opener relationship is kept.

use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, PWSTR};

use super::{create, take, CreateOptions, OpenRequest, TabShared};
use crate::host::{config, emit_tab, post};

static NEXT_POPUP: AtomicU32 = AtomicU32::new(1);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TabCreated {
    id: String,
    opener_id: String,
    url: String,
    popup: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PopupBlocked {
    tab_id: String,
    url: String,
}

/// `window.open` and link targets that ask for a new window.
pub(super) unsafe fn wire_new_window(core: &ICoreWebView2, s: &Rc<TabShared>) {
    let mut token = 0i64;
    unsafe {
        let sh = s.clone();
        let _ = core.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = take(uri);
                let mut user = BOOL::default();
                args.IsUserInitiated(&mut user)?;
                let name = args
                    .cast::<ICoreWebView2NewWindowRequestedEventArgs2>()
                    .ok()
                    .and_then(|a| {
                        let mut p = PWSTR::null();
                        a.Name(&mut p).ok()?;
                        Some(take(p))
                    })
                    .unwrap_or_default();
                let popup = args
                    .WindowFeatures()
                    .ok()
                    .map(|f| {
                        let mut has = BOOL::default();
                        let _ = f.HasSize(&mut has);
                        has.as_bool()
                    })
                    .unwrap_or(false);
                args.SetHandled(true)?;
                let opener = sh.id.clone();
                match name.as_str() {
                    // Login forms (see host::autofill).
                    "_zenith_pw_field" | "_zenith_pw_type" | "_zenith_pw_dismiss" | "_zenith_pw_capture" | "_zenith_pw_form"
                    | "_zenith_card_field" | "_zenith_card_capture" => {
                        let main_frame = crate::host::autofill::from_main_frame(&args);
                        crate::host::autofill::on_message(&sh, &name, &uri, user.as_bool(), main_frame);
                    }
                    // The media downloader's button over a video (see media::detect).
                    "_zenith_media" => {
                        if user.as_bool() {
                            crate::media::sniff::request_panel(&opener, &uri);
                        }
                    }
                    // "Add to Zenith" on a Chrome Web Store page (STORE_SCRIPT).
                    "_zenith_ext" => {
                        let on_store = sh.page_host.borrow().as_str() == "chromewebstore.google.com";
                        if let (true, true, Some(id)) = (user.as_bool(), on_store, crate::extensions::crx::store_id(&uri)) {
                            let source = "Chrome Web Store".to_string();
                            if sh.private {
                                crate::extensions::refuse(&opener, source, "Extensions can't be installed from an incognito window.");
                            } else {
                                crate::extensions::offer(opener, source, crate::extensions::Fetch::Store(id));
                            }
                        }
                    }
                    "_zenith_glance" | "_zenith_bg" => {
                        if user.as_bool() {
                            let kind = if name == "_zenith_glance" { "glance" } else { "background" };
                            emit_tab(
                                &opener.clone(),
                                "tab-open-request",
                                OpenRequest { url: uri, opener_id: Some(opener), kind: kind.into() },
                            );
                        }
                    }
                    _ => {
                        // A magnet link opened in a new window: download it, no tab.
                        if user.as_bool() && crate::torrents::open_from_page(&uri, &opener, sh.private, &sh.window) {
                            return Ok(());
                        }
                        if !user.as_bool() && config().block_popups {
                            emit_tab(&opener.clone(), "popup-blocked", PopupBlocked { tab_id: opener, url: uri });
                            return Ok(());
                        }
                        // Keep the opener relationship (OAuth, payments): hand WebView2 a fresh
                        // tab webview to load the popup into.
                        let deferral = args.GetDeferral()?;
                        let id = format!("w{}", NEXT_POPUP.fetch_add(1, Ordering::Relaxed));
                        let window = sh.window.clone();
                        post(move || {
                            match create(CreateOptions {
                                id: id.clone(),
                                window,
                                url: None,
                                glance: false,
                                muted: false,
                                zoom: None,
                                scroll: None,
                                opener_url: None,
                            }) {
                                Ok(core) => {
                                    let _ = args.SetNewWindow(&core);
                                    let _ = deferral.Complete();
                                    emit_tab(&opener.clone(), "tab-created", TabCreated { id, opener_id: opener, url: uri, popup });
                                }
                                Err(e) => {
                                    eprintln!("[zenith] popup webview failed: {e}");
                                    let _ = deferral.Complete();
                                }
                            }
                        });
                    }
                }
                Ok(())
            })),
            &mut token,
        );
    }
}
