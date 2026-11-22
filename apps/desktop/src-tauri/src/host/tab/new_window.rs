//! New-window requests from pages (`LifeSpanHandler::on_before_popup`): handles Zenith's
//! unprivileged `_zenith_*` page signals, magnet links, popup blocking, and real OAuth/payment
//! popups embedded as Zenith child tabs preserving `window.opener`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use cef::*;
use serde::Serialize;
use windows::Win32::Foundation::HWND;

use super::events::create_tab_client;
use super::{create_container, OpenRequest, Tab, TabPatch, TabShared};
use crate::host::{config, emit_tab, post, set_tab_window, with_host};

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

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_before_popup(
    sh: &Arc<TabShared>,
    _browser: Option<&mut Browser>,
    frame: Option<&mut Frame>,
    uri: &str,
    name: &str,
    _disposition: WindowOpenDisposition,
    user: bool,
    popup_features: Option<&PopupFeatures>,
    window_info: Option<&mut WindowInfo>,
    client: Option<&mut Option<Client>>,
    settings: Option<&mut BrowserSettings>,
    extra_info: Option<&mut Option<DictionaryValue>>,
) -> ::std::os::raw::c_int {
    let opener = sh.id.clone();
    let main_frame = frame.is_some_and(|f| f.is_main() != 0);

    match name {
        // Unprivileged login/card form messages (top-level document only).
        "_zenith_pw_field"
        | "_zenith_pw_type"
        | "_zenith_pw_dismiss"
        | "_zenith_pw_capture"
        | "_zenith_pw_form"
        | "_zenith_card_field"
        | "_zenith_card_capture" => {
            let sh = sh.clone();
            let name = name.to_string();
            let uri = uri.to_string();
            post(move || crate::host::autofill::on_message(&sh, &name, &uri, user, main_frame));
            return 1;
        }
        "_zenith_audio" => {
            TabPatch { audible: Some(uri.ends_with("#1")), ..TabPatch::new(&opener) }.send();
            return 1;
        }
        "_zenith_media" => {
            if user {
                crate::media::sniff::request_panel(&opener, uri);
            }
            return 1;
        }
        "_zenith_ext" => {
            let on_store = sh.page_host() == "chromewebstore.google.com";
            if let (true, true, Some(id)) = (user, on_store, crate::extensions::crx::store_id(uri)) {
                let source = "Chrome Web Store".to_string();
                if sh.private {
                    crate::extensions::refuse(
                        &opener,
                        source,
                        "Extensions can't be installed from an incognito window.",
                    );
                } else {
                    crate::extensions::offer(opener, source, crate::extensions::Fetch::Store(id));
                }
            }
            return 1;
        }
        "_zenith_glance" | "_zenith_bg" => {
            if user {
                let kind = if name == "_zenith_glance" { "glance" } else { "background" };
                emit_tab(
                    &opener.clone(),
                    "tab-open-request",
                    OpenRequest { url: uri.to_string(), opener_id: Some(opener), kind: kind.into() },
                );
            }
            return 1;
        }
        _ => {}
    }

    if user && crate::torrents::open_from_page(uri, &opener, sh.private, &sh.window) {
        return 1;
    }

    if !user && config().block_popups {
        emit_tab(&opener.clone(), "popup-blocked", PopupBlocked { tab_id: opener, url: uri.to_string() });
        return 1;
    }

    // Real popup or target=_blank preserving `window.opener`: embed directly as a child tab!
    let popup = popup_features.is_some_and(|f| f.width_set != 0 || f.height_set != 0);
    let id = format!("w{}", NEXT_POPUP.fetch_add(1, Ordering::Relaxed));
    let window = sh.window.clone();
    let parent_hwnd = with_host(|h| h.wins.get(&window).map(|w| w.hwnd)).flatten();
    let Some(parent_hwnd) = parent_hwnd else {
        return 1;
    };
    let Ok(container_hwnd) = create_container(parent_hwnd, 0, 0, 800, 600) else {
        return 1;
    };

    set_tab_window(&id, &window);
    let light_bg = uri.starts_with("chrome-extension://");
    let child_shared = Arc::new(TabShared::new(&id, &window, false, light_bg));
    child_shared.container_hwnd.store(container_hwnd.0 as isize, Ordering::SeqCst);

    if let Some(wi) = window_info {
        *wi = WindowInfo::default().set_as_child(
            cef::sys::HWND(container_hwnd.0.cast()),
            &Rect { x: 0, y: 0, width: 800, height: 600 },
        );
        wi.runtime_style = RuntimeStyle::ALLOY;
    }
    if let Some(c) = client {
        *c = Some(create_tab_client(child_shared.clone()));
    }
    if let Some(s) = settings {
        if light_bg {
            s.background_color = 0xFFFFFFFF;
        }
    }
    if let Some(extra_slot) = extra_info {
        if let Some(extra) = dictionary_value_create() {
            extra.set_string(Some(&CefString::from("zenith_role")), Some(&CefString::from("page")));
            extra.set_string(
                Some(&CefString::from("zenith_private")),
                Some(&CefString::from(if sh.private { "1" } else { "0" })),
            );
            extra.set_string(
                Some(&CefString::from("zenith_media_button")),
                Some(&CefString::from(if config().media_button { "1" } else { "0" })),
            );
            *extra_slot = Some(extra);
        }
    }

    let tab = Tab {
        container_hwnd,
        browser: None,
        browser_hwnd: HWND::default(),
        shared: child_shared,
        visible: false,
        last_rect: None,
        content_px: None,
        pending_url: None,
        pending_muted: false,
        pending_zoom: None,
        find_text: String::new(),
        find_case: false,
    };
    let id_clone = id.clone();
    let win_clone = window.clone();
    with_host(move |h| {
        h.tabs.insert(id_clone, tab);
        h.apply_layout(&win_clone);
    });

    emit_tab(
        &opener.clone(),
        "tab-created",
        TabCreated { id, opener_id: opener, url: uri.to_string(), popup },
    );
    0
}
