//! The window subclass: runs posted closures and reacts to resizing, activation, focus and
//! keys that reach the bare window frame.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::Shell::DefSubclassProc;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::accelerators::{bound_action, key_down, ShortcutEvent};
use super::focus::{restore_focus, win_state};
use super::placement::on_resize;
use super::{emit, emit_to_window, post, EXEC_MSG};

pub(super) unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        if msg == *EXEC_MSG {
            let f: Box<Box<dyn FnOnce()>> = Box::from_raw(wparam.0 as *mut _);
            f();
            return LRESULT(0);
        }
        match msg {
            WM_SIZE => {
                let result = DefSubclassProc(hwnd, msg, wparam, lparam);
                if wparam.0 as u32 == SIZE_MINIMIZED {
                    MINIMIZED.with(|m| m.borrow_mut().insert(hwnd.0 as isize));
                }
                if wparam.0 as u32 != SIZE_MINIMIZED {
                    let w = (lparam.0 & 0xFFFF) as i32;
                    let h = ((lparam.0 >> 16) & 0xFFFF) as i32;
                    on_resize(hwnd, w, h);
                    // A window brought back from the taskbar or Win+D was activated while still
                    // minimized, when no webview can take the focus: it is left on the frame. Only
                    // that case needs it: moving the focus on every resize while the window is
                    // active leaves the webviews' Chromium windows at their old size, so a
                    // shrunk window shows its pages cut off at the window edge.
                    if !MINIMIZED.with(|m| m.borrow_mut().remove(&(hwnd.0 as isize))) {
                        return result;
                    }
                    post(move || restore_focus(hwnd, None, true));
                }
                result
            }
            // Transparency effects or battery saver may have been switched (settings changes are
            // broadcast; battery saver is caught by the power broadcast or the next activation).
            WM_SETTINGCHANGE | WM_POWERBROADCAST => {
                post(check_translucency);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            WM_ACTIVATE => {
                let state = (wparam.0 & 0xFFFF) as u32;
                let minimized = (wparam.0 >> 16) & 0xFFFF != 0;
                if state == WA_INACTIVE || minimized {
                    return DefSubclassProc(hwnd, msg, wparam, lparam);
                }
                post(check_translucency);
                // Coming back to the window (Alt+Tab, the taskbar, another window closing) puts
                // the keyboard focus on this frame, and wry then asks the chrome to take it. That
                // does not reliably land in a webview, and keys sent to the bare frame never
                // reach the shortcut handlers. Once activation is over, put the focus back where
                // the user left it. The focus is read now: activation itself refocuses the chrome.
                let previous = win_state(hwnd).map(|(_, focus)| focus);
                ACTIVATING.set(true);
                let result = DefSubclassProc(hwnd, msg, wparam, lparam);
                ACTIVATING.set(false);
                // A click puts the focus where the user clicked; only rescue it if it got lost.
                let only_if_lost = state == WA_CLICKACTIVE;
                post(move || restore_focus(hwnd, previous, only_if_lost));
                result
            }
            WM_SETFOCUS => {
                if ACTIVATING.get() {
                    return DefSubclassProc(hwnd, msg, wparam, lparam);
                }
                // wry moves focus to the chrome webview when the frame gets focus; give it back
                // to the page the user was in.
                let previous = win_state(hwnd).map(|(_, focus)| focus);
                let result = DefSubclassProc(hwnd, msg, wparam, lparam);
                post(move || restore_focus(hwnd, previous, false));
                result
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                // Keys only reach the frame when no webview has the focus. Shortcuts still run,
                // and the focus goes back into a webview for the keys that follow.
                if let Some((label, _)) = win_state(hwnd) {
                    post(move || restore_focus(hwnd, None, true));
                    let alt = lparam.0 & (1 << 29) != 0 || key_down(VK_MENU);
                    let ctrl = key_down(VK_CONTROL);
                    let shift = key_down(VK_SHIFT);
                    if let Some(action) = bound_action(ctrl, alt, shift, wparam.0 as u32) {
                        let repeat = lparam.0 & (1 << 30) != 0;
                        emit_to_window(&label, "shortcut", ShortcutEvent { action, repeat, source: None });
                        return LRESULT(0);
                    }
                }
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
    }
}

/// Tells every window to re-apply its material when Windows started or stopped allowing it.
fn check_translucency() {
    if crate::platform::win::translucency_changed() {
        emit("translucency-changed", ());
    }
}

thread_local! {
    /// Set while `WM_ACTIVATE` runs, so its nested `WM_SETFOCUS` leaves the restore to it.
    static ACTIVATING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Windows (by handle) that were minimized and have not been sized since being shown again.
    static MINIMIZED: std::cell::RefCell<std::collections::HashSet<isize>> = std::cell::RefCell::new(std::collections::HashSet::new());
}
