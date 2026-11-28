//! New-window requests from pages (`LifeSpanHandler::on_before_popup`): handles Zenith's
//! unprivileged `_zenith_*` page signals, magnet links, popup blocking, and real OAuth/payment
//! popups embedded as Zenith child tabs preserving `window.opener`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use cef::*;
use serde::Serialize;
use windows::Win32::Foundation::HWND;

use super::events::create_tab_client;
use super::{create_container, is_web, ClickOrigin, OpenRequest, Tab, TabPatch, TabShared};
use crate::host::{config, emit_tab, post, set_tab_window, with_host};
use crate::platform::urls::{host_of, site_of};

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
    let opener = sh.id();
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
                crate::media::sniff::request_from_button(&opener, uri);
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
        "_zenith_bg" | "_zenith_split" | "_zenith_little" => {
            if user {
                let kind = match name {
                    "_zenith_split" => "split",
                    "_zenith_little" => "little",
                    _ => "background",
                };
                emit_tab(
                    &opener.clone(),
                    "tab-open-request",
                    OpenRequest { url: uri.to_string(), opener_id: Some(opener), kind: kind.into(), origin: None },
                );
            }
            return 1;
        }
        other if other == "_zenith_glance" || other.starts_with("_zenith_glance:") => {
            if user {
                let origin = other.strip_prefix("_zenith_glance:").and_then(parse_origin);
                emit_tab(
                    &opener.clone(),
                    "tab-open-request",
                    OpenRequest { url: uri.to_string(), opener_id: Some(opener), kind: "glance".into(), origin },
                );
            }
            return 1;
        }
        _ => {}
    }

    let window = sh.window();
    if user && crate::torrents::open_from_page(uri, &opener, sh.private, &window) {
        return 1;
    }

    if !user && config().block_popups {
        emit_tab(&opener.clone(), "popup-blocked", PopupBlocked { tab_id: opener, url: uri.to_string() });
        return 1;
    }

    // Real popup or target=_blank preserving `window.opener`: embed directly as a child tab!
    let popup = popup_features.is_some_and(|f| f.width_set != 0 || f.height_set != 0);
    let id = format!("w{}", NEXT_POPUP.fetch_add(1, Ordering::Relaxed));
    let parent_hwnd = with_host(|h| h.wins.get(&window).map(|w| w.hwnd)).flatten();
    let Some(parent_hwnd) = parent_hwnd else {
        return 1;
    };
    let Ok(container_hwnd) = create_container(parent_hwnd, 0, 0, 800, 600) else {
        return 1;
    };

    set_tab_window(&id, &window);
    let light_bg = uri.starts_with("chrome-extension://");
    let child_shared = Arc::new(TabShared::new(&id, &window, false, light_bg, None));
    child_shared.container_hwnd.store(container_hwnd.0 as isize, Ordering::SeqCst);

    if let Some(wi) = window_info {
        *wi = WindowInfo::default().set_as_child(
            cef::sys::HWND(container_hwnd.0.cast()),
            &Rect { x: 0, y: 0, width: 800, height: 600 },
        );
        wi.runtime_style = RuntimeStyle::CHROME;
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
            let trig = crate::host::config::triggers_json_for(&config());
            extra.set_string(Some(&CefString::from("zenith_triggers")), Some(&CefString::from(trig.as_str())));
            *extra_slot = Some(extra);
        }
    }

    let tab = Tab {
        container_hwnd,
        browser: None,
        browser_hwnd: HWND::default(),
        devtools: None,
        shared: child_shared,
        visible: false,
        last_rect: None,
        content_px: None,
        rgn_px: None,
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

fn parse_origin(raw: &str) -> Option<ClickOrigin> {
    let (xs, ys) = raw.split_once(',')?;
    let x = xs.parse::<i32>().ok()?.clamp(0, 10_000);
    let y = ys.parse::<i32>().ok()?.clamp(0, 10_000);
    Some(ClickOrigin { x, y })
}

/// Intercepts cross-site user navigations from Essentials (Glance) or Pinned tabs (new tab).
pub(super) fn intercept_pinned_external(sh: &TabShared, uri: &str, user_gesture: bool, is_redirect: bool) -> bool {
    if !user_gesture || is_redirect || !is_web(uri) {
        return false;
    }
    let Some((pin_site, essential)) = sh.pin_site.read().ok().and_then(|s| s.clone()) else {
        return false;
    };
    let Some(target_host) = host_of(uri) else {
        return false;
    };
    if site_of(target_host).eq_ignore_ascii_case(&pin_site) {
        return false;
    }
    let cfg = config();
    let kind = if essential {
        if !cfg.glance_essential_external { return false; }
        "glance"
    } else {
        if !cfg.open_pinned_external_in_new_tab { return false; }
        "foreground"
    };
    let id = sh.id();
    emit_tab(
        &id.clone(),
        "tab-open-request",
        OpenRequest { url: uri.to_string(), opener_id: Some(id), kind: kind.into(), origin: None },
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_origin_clamps_and_rejects_bad_input() {
        let ok = parse_origin("120,450").expect("valid coords");
        assert_eq!((ok.x, ok.y), (120, 450));
        let clamped = parse_origin("-5,99999").expect("clamped");
        assert_eq!((clamped.x, clamped.y), (0, 10_000));
        assert!(parse_origin("bad").is_none());
    }
}
