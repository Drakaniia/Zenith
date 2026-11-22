//! Keyboard focus: putting focus back into the right CEF browser after window activation,
//! and moving it on request from the chrome UI.

use cef::*;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsChild};

use super::{with_host, Focus, HOST};

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
    with_host(|h| {
        let Some(win) = h.wins.values().find(|w| w.hwnd == hwnd) else { return };
        let previous = previous.unwrap_or_else(|| win.focus.clone());
        let label = win.window.label();
        let tab_browser = match &previous {
            Focus::Tab(id) => h
                .tabs
                .get(id)
                .filter(|t| t.visible && t.window() == label)
                .and_then(|t| t.browser.clone()),
            Focus::Chrome => None,
        };
        if let Some(b) = tab_browser {
            if let Some(host) = b.host() {
                host.set_focus(1);
            }
        } else if let Some(b) = &win.chrome {
            if let Some(host) = b.host() {
                host.set_focus(1);
            }
        }
    });
}

pub fn focus_view(label: &str, target: Option<String>) {
    with_host(|h| {
        let Some(win) = h.wins.get_mut(label) else { return };
        match target {
            Some(id) => {
                if let Some(b) = h
                    .tabs
                    .get(&id)
                    .filter(|t| t.window() == label)
                    .and_then(|t| t.browser.clone())
                {
                    if let Some(host) = b.host() {
                        host.set_focus(1);
                    }
                    win.focus = Focus::Tab(id);
                }
            }
            None => {
                if let Some(b) = &win.chrome {
                    if let Some(host) = b.host() {
                        host.set_focus(1);
                    }
                }
                win.focus = Focus::Chrome;
            }
        }
    });
}
