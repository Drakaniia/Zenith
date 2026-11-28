//! Native CEF DevTools windows: docked per-tab child HWNDs inside Zenith's content layout, and
//! detached Alloy popup windows for Browser Toolbox (Zenith chrome UI inspection).

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;

use cef::*;
use serde::Serialize;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, ShowWindow, SW_HIDE, SW_SHOWNOACTIVATE};

use super::placement::px_rect;
use super::tab::{create_container, TabRef};
use super::{
    emit_tab, emit_to_window, focus_view, handle_devtools_key_event, post, track_browser,
    untrack_browser, with_host, with_win, Focus,
};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DevToolsChanged {
    tab_id: String,
    open: bool,
    activate: bool,
}

pub(crate) fn emit_devtools_changed(window: &str, tab_id: &str, open: bool, activate: bool) {
    emit_to_window(
        window,
        "devtools-changed",
        DevToolsChanged { tab_id: tab_id.to_string(), open, activate },
    );
}

// ---------------------------------------------------------------------------------------------
// Detached popup DevTools (used for Browser Toolbox / Zenith chrome UI inspection).

wrap_life_span_handler! {
    struct PopupDevToolsLifeSpan;

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                track_browser(browser);
            }
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                untrack_browser(browser.identifier());
            }
        }
    }
}

wrap_client! {
    struct PopupDevToolsClient;

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(PopupDevToolsLifeSpan::new())
        }
    }
}

/// Opens a detached popup DevTools window inspecting `browser` (used for Zenith's own chrome UI).
pub fn open_popup_devtools(browser: &Browser) -> Result<(), String> {
    let Some(host) = browser.host() else {
        return Err("browser host unavailable".into());
    };
    let mut info = WindowInfo::default().set_as_popup(cef::sys::HWND(std::ptr::null_mut()), "DevTools");
    info.runtime_style = RuntimeStyle::ALLOY;
    let settings = BrowserSettings::default();
    let inspect_at = Point::default();
    let mut client = PopupDevToolsClient::new();
    host.show_dev_tools(Some(&info), Some(&mut client), Some(&settings), Some(&inspect_at));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Docked per-tab child DevTools.

pub struct DevToolsShared {
    pub tab_id: String,
    pub window: String,
    pub container_hwnd: AtomicIsize,
    pub closed: AtomicBool,
}

pub struct TabDevTools {
    pub container_hwnd: HWND,
    pub browser: Option<Browser>,
    pub browser_hwnd: HWND,
    pub shared: Arc<DevToolsShared>,
    pub visible: bool,
    pub last_rect: Option<(f64, f64, f64, f64)>,
    pub content_px: Option<(i32, i32)>,
    pub rgn_px: Option<(i32, i32, i32)>,
}

impl TabDevTools {
    pub fn set_visible(&mut self, show: bool) {
        self.visible = show;
        if !self.container_hwnd.is_invalid() {
            unsafe {
                let _ = ShowWindow(self.container_hwnd, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
        }
    }

    pub fn destroy_container(&self) {
        let raw = self.shared.container_hwnd.swap(0, Ordering::SeqCst);
        if raw != 0 {
            let hwnd = HWND(raw as *mut _);
            if !hwnd.is_invalid() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }
    }
}

wrap_life_span_handler! {
    struct DockedDevToolsLifeSpan {
        shared: Arc<DevToolsShared>,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            track_browser(browser);
            if self.shared.closed.load(Ordering::SeqCst) {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
                return;
            }
            let browser_hwnd = browser
                .host()
                .map(|h| HWND(h.window_handle().0.cast()))
                .unwrap_or_default();
            let id = self.shared.tab_id.clone();
            let window = self.shared.window.clone();
            let attached = with_host(|h| {
                let Some(tab) = h.tabs.get_mut(&id) else { return false };
                let Some(dt) = tab.devtools.as_mut() else { return false };
                if !Arc::ptr_eq(&dt.shared, &self.shared) {
                    return false;
                }
                dt.browser = Some(browser.clone());
                dt.browser_hwnd = browser_hwnd;
                dt.content_px = None;
                h.apply_layout(&window);
                true
            })
            .unwrap_or(false);
            if !attached {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
            }
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            self.shared.closed.store(true, Ordering::SeqCst);
            // Child DevTools: destroy only its child HWND and return 1 so CEF does NOT send
            // WM_CLOSE to the top-level Zenith browser window.
            if let Some(raw) = browser
                .and_then(|b| b.host())
                .map(|h| h.window_handle().0 as isize)
                .filter(|&p| p != 0)
            {
                post(move || {
                    let hwnd = HWND(raw as *mut _);
                    if !hwnd.is_invalid() {
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                });
            }
            1
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                untrack_browser(browser.identifier());
            }
            let id = self.shared.tab_id.clone();
            let window = self.shared.window.clone();
            let (was_current, restore_tab_focus) = with_host(|h| {
                let mut current = false;
                if let Some(tab) = h.tabs.get_mut(&id) {
                    if tab.devtools.as_ref().is_some_and(|dt| Arc::ptr_eq(&dt.shared, &self.shared)) {
                        tab.devtools = None;
                        current = true;
                    }
                }
                let focused_here = h
                    .wins
                    .get(&window)
                    .is_some_and(|w| matches!(&w.focus, Focus::DevTools(tid) if tid == &id));
                (current, current && focused_here)
            })
            .unwrap_or((false, false));

            if was_current {
                emit_devtools_changed(&window, &id, false, false);
            }
            if restore_tab_focus {
                let w = window.clone();
                let tid = id.clone();
                post(move || focus_view(&w, Some(tid)));
            }

            let raw = self.shared.container_hwnd.swap(0, Ordering::SeqCst);
            if raw != 0 {
                post(move || {
                    let hwnd = HWND(raw as *mut _);
                    if !hwnd.is_invalid() {
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                });
            }
        }
    }
}

wrap_focus_handler! {
    struct DockedDevToolsFocus {
        shared: Arc<DevToolsShared>,
    }

    impl FocusHandler {
        fn on_got_focus(&self, _browser: Option<&mut Browser>) {
            let id = self.shared.tab_id.clone();
            let focused = id.clone();
            let window = self.shared.window.clone();
            post(move || {
                with_win(&window, |w| w.focus = Focus::DevTools(focused));
            });
            emit_tab(&id.clone(), "tab-focused", TabRef { id });
        }
    }
}

wrap_keyboard_handler! {
    struct DockedDevToolsKeyboard {
        shared: Arc<DevToolsShared>,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut cef::sys::MSG>,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let Some(event) = event else { return 0 };
            if handle_devtools_key_event(event, &self.shared.tab_id, &self.shared.window) {
                1
            } else {
                0
            }
        }
    }
}

wrap_client! {
    struct DockedDevToolsClient {
        shared: Arc<DevToolsShared>,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(DockedDevToolsLifeSpan::new(self.shared.clone()))
        }
        fn focus_handler(&self) -> Option<FocusHandler> {
            Some(DockedDevToolsFocus::new(self.shared.clone()))
        }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(DockedDevToolsKeyboard::new(self.shared.clone()))
        }
    }
}

/// Opens (or focuses / inspects at `inspect_at`) docked DevTools inside the tab's window.
pub fn open_tab_devtools(id: &str, inspect_at: Option<Point>, activate: bool) -> Result<bool, String> {
    let snapshot = with_host(|h| {
        let tab = h.tabs.get(id)?;
        let label = tab.window();
        let win = h.wins.get(&label)?;
        let scale = h.scale(&label);
        let existing_dt = tab.devtools.as_ref().map(|dt| dt.browser.clone());
        let rect = win
            .layout
            .devtools
            .as_ref()
            .filter(|d| d.id == id)
            .map(|d| px_rect(d.x, d.y, d.w, d.h, scale));
        Some((label, win.hwnd, scale, h.page_bg, tab.browser.clone(), existing_dt, rect))
    })
    .flatten()
    .ok_or("tab has no browser")?;

    let (label, parent_hwnd, scale, bg, browser, existing_dt, rect) = snapshot;
    let browser = browser.ok_or("tab has no browser")?;
    let host = browser.host().ok_or("browser host unavailable")?;

    if let Some(dt_browser) = existing_dt {
        if host.has_dev_tools() != 0 {
            if let Some(pt) = inspect_at {
                host.show_dev_tools(None, None, None, Some(&pt));
            }
            if let Some(b) = dt_browser {
                if let Some(dt_host) = b.host() {
                    dt_host.set_focus(1);
                }
            }
            emit_devtools_changed(&label, id, true, activate);
            return Ok(true);
        }
        // Stale entry with no live DevTools: drop before creating a fresh one.
        with_host(|h| {
            if let Some(tab) = h.tabs.get_mut(id) {
                if let Some(dt) = tab.devtools.take() {
                    dt.shared.closed.store(true, Ordering::SeqCst);
                    dt.destroy_container();
                }
            }
        });
    } else if host.has_dev_tools() != 0 {
        host.close_dev_tools();
    }

    let (x, y, w, h) = rect.unwrap_or_else(|| {
        (0, 0, (420.0 * scale).round().max(1.0) as i32, (600.0 * scale).round().max(1.0) as i32)
    });
    let container_hwnd = create_container(parent_hwnd, x, y, w, h)?;
    let shared = Arc::new(DevToolsShared {
        tab_id: id.to_string(),
        window: label.clone(),
        container_hwnd: AtomicIsize::new(container_hwnd.0 as isize),
        closed: AtomicBool::new(false),
    });

    let mut win_info = WindowInfo::default().set_as_child(
        cef::sys::HWND(container_hwnd.0.cast()),
        &Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) },
    );
    win_info.runtime_style = RuntimeStyle::ALLOY;

    let mut settings = BrowserSettings::default();
    settings.background_color = 0xFF000000 | ((bg.0 as u32) << 16) | ((bg.1 as u32) << 8) | (bg.2 as u32);

    let mut client = DockedDevToolsClient::new(shared.clone());
    let dt = TabDevTools {
        container_hwnd,
        browser: None,
        browser_hwnd: HWND::default(),
        shared,
        visible: false,
        last_rect: None,
        content_px: None,
        rgn_px: None,
    };

    with_host(|h| {
        if let Some(tab) = h.tabs.get_mut(id) {
            tab.devtools = Some(dt);
            h.apply_layout(&label);
        }
    });

    let pt = inspect_at.unwrap_or_default();
    host.show_dev_tools(Some(&win_info), Some(&mut client), Some(&settings), Some(&pt));
    emit_devtools_changed(&label, id, true, activate);
    Ok(true)
}

/// Closes docked DevTools for tab `id` if open.
pub fn close_tab_devtools(id: &str) -> Result<bool, String> {
    let state = with_host(|h| {
        let tab = h.tabs.get_mut(id)?;
        let label = tab.window();
        let dt = tab.devtools.take();
        let was_focused = h
            .wins
            .get(&label)
            .is_some_and(|w| matches!(&w.focus, Focus::DevTools(tid) if tid == id));
        Some((label, tab.browser.clone(), dt, was_focused))
    })
    .flatten();

    let Some((label, tab_browser, dt, was_focused)) = state else {
        return Ok(false);
    };

    let had_dt = dt.is_some();
    if let Some(mut dt) = dt {
        dt.shared.closed.store(true, Ordering::SeqCst);
        dt.set_visible(false);
        let closed_via_host = tab_browser
            .as_ref()
            .and_then(|b| b.host())
            .is_some_and(|host| {
                if host.has_dev_tools() != 0 {
                    host.close_dev_tools();
                    true
                } else {
                    false
                }
            });
        if !closed_via_host {
            if let Some(b) = dt.browser {
                if let Some(host) = b.host() {
                    host.close_browser(1);
                }
            } else {
                dt.destroy_container();
            }
        }
    } else if let Some(host) = tab_browser.as_ref().and_then(|b| b.host()) {
        if host.has_dev_tools() != 0 {
            host.close_dev_tools();
        }
    }

    if had_dt {
        emit_devtools_changed(&label, id, false, false);
    }
    if was_focused {
        focus_view(&label, Some(id.to_string()));
    }
    Ok(false)
}

#[derive(Debug, PartialEq, Eq)]
enum DevToolsMode {
    Toggle,
    Open,
    Close,
}

fn parse_devtools_mode(arg: Option<&serde_json::Value>) -> DevToolsMode {
    let Some(v) = arg else { return DevToolsMode::Toggle };
    if v.is_null() {
        return DevToolsMode::Toggle;
    }
    if v.is_string() {
        return match v.as_str() {
            Some("close") => DevToolsMode::Close,
            Some("toggle") => DevToolsMode::Toggle,
            _ => DevToolsMode::Open,
        };
    }
    match v.get("mode").and_then(|m| m.as_str()) {
        Some("open") => DevToolsMode::Open,
        Some("close") => DevToolsMode::Close,
        _ => DevToolsMode::Toggle,
    }
}

/// Handles `tab_command(id, "devtools", arg)` for toggling, opening, or closing docked DevTools.
pub fn handle_devtools_command(id: &str, arg: Option<&serde_json::Value>) -> Result<serde_json::Value, String> {
    let open = match parse_devtools_mode(arg) {
        DevToolsMode::Open => open_tab_devtools(id, None, false)?,
        DevToolsMode::Close => close_tab_devtools(id)?,
        DevToolsMode::Toggle => {
            let is_open = with_host(|h| {
                h.tabs.get(id).is_some_and(|t| {
                    t.devtools.is_some()
                        || t.browser.as_ref().and_then(|b| b.host()).is_some_and(|bh| bh.has_dev_tools() != 0)
                })
            })
            .unwrap_or(false);
            if is_open {
                close_tab_devtools(id)?
            } else {
                open_tab_devtools(id, None, false)?
            }
        }
    };
    Ok(serde_json::json!(open))
}

#[cfg(test)]
mod tests {
    use super::{parse_devtools_mode, DevToolsMode};
    use serde_json::json;

    #[test]
    fn parses_devtools_command_modes() {
        assert_eq!(parse_devtools_mode(None), DevToolsMode::Toggle);
        assert_eq!(parse_devtools_mode(Some(&json!(null))), DevToolsMode::Toggle);
        assert_eq!(parse_devtools_mode(Some(&json!("toggle"))), DevToolsMode::Toggle);
        assert_eq!(parse_devtools_mode(Some(&json!("close"))), DevToolsMode::Close);
        assert_eq!(parse_devtools_mode(Some(&json!("elements"))), DevToolsMode::Open);
        assert_eq!(parse_devtools_mode(Some(&json!({ "mode": "open", "panel": "console" }))), DevToolsMode::Open);
        assert_eq!(parse_devtools_mode(Some(&json!({ "mode": "close" }))), DevToolsMode::Close);
        assert_eq!(parse_devtools_mode(Some(&json!({ "mode": "toggle" }))), DevToolsMode::Toggle);
    }
}
