//! Keyboard focus: putting it back into the right webview after the window is activated, and
//! moving it on request.

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{with_host, Focus, HOST};

/// Label and remembered focus of the window `hwnd`, without waiting for a busy host.
pub(super) fn win_state(hwnd: HWND) -> Option<(String, Focus)> {
    HOST.with(|c| {
        c.try_borrow().ok().and_then(|h| {
            h.as_ref().and_then(|h| {
                h.wins.values().find(|w| w.hwnd == hwnd).map(|w| (w.window.label().to_string(), w.focus.clone()))
            })
        })
    })
}

/// True when the keyboard focus is in none of the window's webviews. Their focusable HWNDs belong
/// to the WebView2 browser process, so a focus on a window of this process (the frame, a wry
/// container) is a focus no webview receives keys from.
fn focus_lost(hwnd: HWND) -> bool {
    unsafe {
        let focus = GetFocus();
        if focus.is_invalid() || !IsChild(hwnd, focus).as_bool() {
            return true;
        }
        let mut owner = 0u32;
        GetWindowThreadProcessId(focus, Some(&mut owner as *mut u32));
        owner == windows::Win32::System::Threading::GetCurrentProcessId()
    }
}

/// Gives the keyboard focus of the active window `hwnd` back to the view it was in: the page
/// `previous` names when that tab is still shown, otherwise the chrome. With `only_if_lost` (or
/// no known previous focus) a focus already inside a webview is left alone.
pub(super) fn restore_focus(hwnd: HWND, previous: Option<Focus>, only_if_lost: bool) {
    // The user may have switched away again before this ran.
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
        let tab = match &previous {
            Focus::Tab(id) => h.tabs.get(id).filter(|t| t.visible && t.window() == label),
            Focus::Chrome => None,
        };
        match tab {
            Some(t) => {
                let _ = t.view.focus();
            }
            None => unsafe {
                let _ = win.chrome.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
            },
        }
    });
}

pub fn focus_view(label: &str, target: Option<String>) {
    with_host(|h| {
        let Some(win) = h.wins.get_mut(label) else { return };
        match target {
            Some(id) => {
                if let Some(t) = h.tabs.get(&id).filter(|t| t.window() == label) {
                    let _ = t.view.focus();
                    win.focus = Focus::Tab(id);
                }
            }
            None => unsafe {
                let _ = win.chrome.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
                win.focus = Focus::Chrome;
            },
        }
    });
}
