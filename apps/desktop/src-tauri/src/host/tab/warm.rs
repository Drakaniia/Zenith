//! Single pre-warmed hidden normal-profile CEF tab (`about:blank`) so opening or waking a web
//! tab skips child HWND + renderer creation latency (~0 ms navigation start), while releasing
//! automatically under Windows memory pressure (`>= 88%` physical memory load).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use cef::*;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::SetParent;

use super::{create_container, events, zoom_factor_to_level, CreateOptions, Tab, TabShared};
use crate::app::bridge::GENERATIONS;
use crate::app::runtime::post_ui_delayed;
use crate::app::security::BrowserRole;
use crate::host::{cdp_call, config, is_private_window, set_tab_window, with_host, Host, MAIN};

pub const MEMORY_PRESSURE_LOAD_PCT: u32 = 88;
const WARM_REPLENISH_DELAY_MS: i64 = 600;

pub struct WarmTab {
    pub window: String,
    pub parent_hwnd: HWND,
    pub container_hwnd: HWND,
    pub browser: Option<Browser>,
    pub browser_hwnd: HWND,
    pub shared: Arc<TabShared>,
}

/// Pure eligibility rule for creating or retaining a pre-warmed tab.
pub fn can_use_warm(system_load_pct: u32, private: bool, glance: bool, light_bg: bool) -> bool {
    !private && !glance && !light_bg && system_load_pct < MEMORY_PRESSURE_LOAD_PCT
}

/// Pre-creates one hidden `about:blank` CEF browser in the primary normal window if none exists
/// and system memory is below the pressure threshold. Must run on the CEF UI thread.
pub fn ensure_warm() {
    let system_load = crate::platform::win::system_memory_load();
    if !can_use_warm(system_load, false, false, false) {
        release_if_pressured(system_load);
        return;
    }
    let target = with_host(|h| {
        if h.shutting_down || h.warm_tab.is_some() {
            return None;
        }
        let (label, win) = h
            .wins
            .get_key_value(MAIN)
            .filter(|(l, _)| !is_private_window(l))
            .or_else(|| h.wins.iter().find(|(l, _)| !is_private_window(l)))?;
        Some((label.clone(), win.hwnd, h.page_bg))
    })
    .flatten();

    let Some((label, parent_hwnd, bg)) = target else { return };
    let Ok(container_hwnd) = create_container(parent_hwnd, 0, 0, 800, 600) else { return };
    let shared = Arc::new(TabShared::new_warm("__warm__", &label));
    shared.container_hwnd.store(container_hwnd.0 as isize, Ordering::SeqCst);

    let mut win_info = WindowInfo::default().set_as_child(
        cef::sys::HWND(container_hwnd.0.cast()),
        &Rect { x: 0, y: 0, width: 800, height: 600 },
    );
    win_info.runtime_style = RuntimeStyle::ALLOY;

    let Some(mut extra) = dictionary_value_create() else {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(container_hwnd);
        }
        return;
    };
    extra.set_string(Some(&CefString::from("zenith_role")), Some(&CefString::from("page")));
    extra.set_string(Some(&CefString::from("zenith_private")), Some(&CefString::from("0")));
    extra.set_string(
        Some(&CefString::from("zenith_media_button")),
        Some(&CefString::from(if config().media_button { "1" } else { "0" })),
    );

    let mut settings = BrowserSettings::default();
    settings.background_color = 0xFF000000 | ((bg.0 as u32) << 16) | ((bg.1 as u32) << 8) | (bg.2 as u32);
    let mut client = events::create_tab_client(shared.clone());

    with_host(|h| {
        h.warm_tab = Some(WarmTab {
            window: label,
            parent_hwnd,
            container_hwnd,
            browser: None,
            browser_hwnd: HWND::default(),
            shared,
        });
    });

    if browser_host_create_browser(
        Some(&win_info),
        Some(&mut client),
        Some(&CefString::from("about:blank")),
        Some(&settings),
        Some(&mut extra),
        None,
    ) == 0
    {
        with_host(|h| {
            if let Some(w) = h.warm_tab.take() {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(w.container_hwnd);
                }
            }
        });
    }
}

/// Claims the pre-warmed CEF browser for `opts` when eligible, navigating immediately.
pub(super) fn try_claim(opts: &CreateOptions, parent_hwnd: HWND, light_bg: bool) -> Option<Tab> {
    let private = is_private_window(&opts.window);
    if private || opts.glance || light_bg {
        return None;
    }
    let (warm, scheme) = with_host(|h| {
        let w = h.warm_tab.take()?;
        Some((w, h.content_scheme.clone()))
    })
    .flatten()?;

    if warm.shared.closed.load(Ordering::SeqCst) {
        return None;
    }

    let label = opts.window.clone();
    set_tab_window(&opts.id, &label);
    warm.shared.rebind(&opts.id, &label, opts.scroll);
    if warm.parent_hwnd != parent_hwnd && !warm.container_hwnd.is_invalid() {
        unsafe {
            let _ = SetParent(warm.container_hwnd, Some(parent_hwnd));
        }
    }

    let initial_url = opts.url.clone().unwrap_or_else(|| "about:blank".into());
    let mut pending_url = None;
    let mut pending_muted = opts.muted;
    let mut pending_zoom = opts.zoom;

    if let Some(ref browser) = warm.browser {
        let bid = browser.identifier();
        let gen = GENERATIONS
            .lock()
            .map(|mut g| {
                g.register(
                    bid,
                    BrowserRole::UntrustedPage {
                        tab_id: opts.id.clone(),
                        window: label.clone(),
                    },
                )
            })
            .unwrap_or(0);
        warm.shared.generation.store(gen, Ordering::SeqCst);
        if let Some(host) = browser.host() {
            if pending_muted {
                host.set_audio_muted(1);
                pending_muted = false;
            }
            if let Some(z) = pending_zoom.take() {
                host.set_zoom_level(zoom_factor_to_level(z));
            }
        }
        if scheme == "light" || scheme == "dark" {
            let params = format!(r#"{{"features":[{{"name":"prefers-color-scheme","value":"{scheme}"}}]}}"#);
            cdp_call(browser, "Emulation.setEmulatedMedia", &params, |_| {});
        }
        if initial_url != "about:blank" {
            if let Some(frame) = browser.main_frame() {
                frame.load_url(Some(&CefString::from(initial_url.as_str())));
            }
        }
    } else if initial_url != "about:blank" {
        pending_url = Some(initial_url);
    }

    post_ui_delayed(WARM_REPLENISH_DELAY_MS, ensure_warm);

    Some(Tab {
        container_hwnd: warm.container_hwnd,
        browser: warm.browser,
        browser_hwnd: warm.browser_hwnd,
        devtools: None,
        shared: warm.shared,
        visible: false,
        last_rect: None,
        content_px: None,
        rgn_px: None,
        pending_url,
        pending_muted,
        pending_zoom,
        find_text: String::new(),
        find_case: false,
    })
}

pub fn drop_for_window(h: &mut Host, label: &str) -> Option<Browser> {
    if h.warm_tab.as_ref().is_some_and(|w| w.window == label) {
        return drain_warm(h);
    }
    None
}

pub fn drain_warm(h: &mut Host) -> Option<Browser> {
    let warm = h.warm_tab.take()?;
    warm.shared.closed.store(true, Ordering::SeqCst);
    if let Some(b) = warm.browser {
        Some(b)
    } else {
        let raw = warm.shared.container_hwnd.swap(0, Ordering::SeqCst);
        if raw != 0 {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(HWND(raw as *mut _));
            }
        }
        None
    }
}

pub fn release_if_pressured(system_load_pct: u32) {
    if system_load_pct < MEMORY_PRESSURE_LOAD_PCT {
        return;
    }
    let to_close = with_host(drain_warm).flatten();
    if let Some(b) = to_close {
        if let Some(host) = b.host() {
            host.close_browser(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warm_eligibility_respects_privacy_glance_and_memory_pressure() {
        assert!(can_use_warm(50, false, false, false));
        assert!(can_use_warm(87, false, false, false));
        assert!(!can_use_warm(88, false, false, false));
        assert!(!can_use_warm(95, false, false, false));
        assert!(!can_use_warm(40, true, false, false));
        assert!(!can_use_warm(40, false, true, false));
        assert!(!can_use_warm(40, false, false, true));
    }
}
