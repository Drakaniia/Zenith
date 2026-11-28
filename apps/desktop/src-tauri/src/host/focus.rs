//! Keyboard focus: putting focus back into the right CEF browser after window activation,
//! and moving it on request from the chrome UI.

use cef::*;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsChild};

use super::{cdp_call, with_host, Focus, HOST};

pub(super) fn win_state(hwnd: HWND) -> Option<(String, Focus)> {
    HOST.with(|c| {
        c.try_borrow().ok().and_then(|h| {
            h.wins
                .values()
                .find(|w| w.hwnd == hwnd)
                .map(|w| (w.window.label().to_string(), w.focus.clone()))
        })
    })
}

fn focus_lost(hwnd: HWND) -> bool {
    unsafe {
        let focus = GetFocus();
        focus.is_invalid() || focus == hwnd || !IsChild(hwnd, focus).as_bool()
    }
}

pub(super) fn restore_focus(hwnd: HWND, previous: Option<Focus>, only_if_lost: bool) {
    if unsafe { GetForegroundWindow() } != hwnd {
        return;
    }
    if (only_if_lost || previous.is_none()) && !focus_lost(hwnd) {
        return;
    }
    let target_browser = with_host(|h| {
        let Some(win) = h.wins.values().find(|w| w.hwnd == hwnd) else { return None };
        let previous = previous.unwrap_or_else(|| win.focus.clone());
        let label = win.window.label();
        let tab_browser = match &previous {
            Focus::Tab(id) => h
                .tabs
                .get(id)
                .filter(|t| t.visible && t.window() == label)
                .and_then(|t| t.browser.clone()),
            Focus::DevTools(id) => h
                .tabs
                .get(id)
                .filter(|t| t.window() == label)
                .and_then(|t| t.devtools.as_ref().filter(|dt| dt.visible).and_then(|dt| dt.browser.clone())),
            Focus::Chrome => None,
        };
        tab_browser.or_else(|| win.chrome.clone())
    })
    .flatten();
    if let Some(b) = target_browser {
        if let Some(host) = b.host() {
            host.set_focus(1);
        }
    }
}

/// Marks a real Chrome-style tab as Chromium's last-active browser so extensions'
/// `chrome.tabs.query({ active: true, lastFocusedWindow: true })` targets the focused pane.
pub fn activate_tab_in_chromium(tab_id: &str) {
    let browser = with_host(|h| {
        h.tabs
            .get(tab_id)
            .filter(|t| !t.shared.is_glance())
            .and_then(|t| t.browser.clone())
    })
    .flatten();
    if let Some(b) = browser {
        cdp_call(&b, "Page.bringToFront", "{}", |_| {});
    }
}

pub fn focus_view(label: &str, target: Option<String>) {
    let (target_browser, bring_to_front) = with_host(|h| {
        let Some(win) = h.wins.get_mut(label) else { return (None, false) };
        match target {
            Some(id) => {
                let entry = h
                    .tabs
                    .get(&id)
                    .filter(|t| t.window() == label)
                    .and_then(|t| t.browser.clone().map(|b| (b, !t.shared.is_glance())));
                if entry.is_some() {
                    win.focus = Focus::Tab(id);
                }
                match entry {
                    Some((b, chrome_tab)) => (Some(b), chrome_tab),
                    None => (None, false),
                }
            }
            None => {
                win.focus = Focus::Chrome;
                (win.chrome.clone(), false)
            }
        }
    })
    .unwrap_or((None, false));
    if let Some(b) = target_browser {
        if bring_to_front {
            cdp_call(&b, "Page.bringToFront", "{}", |_| {});
        }
        if let Some(host) = b.host() {
            host.set_focus(1);
        }
    }
}
