//! Win32 window procedure for Zenith's frameless top-level browser windows: borderless
//! `WM_NCCALCSIZE`, native 8px resize edges (`WM_NCHITTEST`), min-size enforcement, resize
//! relayout, window-state tracking, close interception, focus restoration, and frame-level
//! shortcut fallback.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::accelerators::{bound_action, key_down, ShortcutEvent};
use super::focus::{restore_focus, win_state};
use super::placement::on_resize;
use super::{emit_to_window, post, MAIN};
use crate::app::runtime::window_by_hwnd;

const MIN_WIDTH_LOGICAL: f64 = 560.0;
const MIN_HEIGHT_LOGICAL: f64 = 400.0;
const RESIZE_BORDER_PX: i32 = 8;

pub unsafe extern "system" fn host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCALCSIZE => {
            if wparam.0 != 0 {
                // Keep frameless client area filling the window; when maximized, inset by the
                // monitor work area so the window doesn't spill past the taskbar/edges.
                if IsZoomed(hwnd).as_bool() {
                    let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
                    let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                    let mut mi = MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    if GetMonitorInfoW(mon, &mut mi).as_bool() {
                        params.rgrc[0] = mi.rcWork;
                    }
                }
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCHITTEST => {
            if !IsZoomed(hwnd).as_bool() {
                let pt = POINT {
                    x: (lparam.0 & 0xFFFF) as i16 as i32,
                    y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
                };
                let mut r = RECT::default();
                if GetWindowRect(hwnd, &mut r).is_ok() {
                    let left = pt.x < r.left + RESIZE_BORDER_PX;
                    let right = pt.x >= r.right - RESIZE_BORDER_PX;
                    let top = pt.y < r.top + RESIZE_BORDER_PX;
                    let bottom = pt.y >= r.bottom - RESIZE_BORDER_PX;
                    let hit = match (left, right, top, bottom) {
                        (true, _, true, _) => Some(HTTOPLEFT),
                        (_, true, true, _) => Some(HTTOPRIGHT),
                        (true, _, _, true) => Some(HTBOTTOMLEFT),
                        (_, true, _, true) => Some(HTBOTTOMRIGHT),
                        (true, _, _, _) => Some(HTLEFT),
                        (_, true, _, _) => Some(HTRIGHT),
                        (_, _, true, _) => Some(HTTOP),
                        (_, _, _, true) => Some(HTBOTTOM),
                        _ => None,
                    };
                    if let Some(code) = hit {
                        return LRESULT(code as isize);
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_GETMINMAXINFO => {
            let mmi = &mut *(lparam.0 as *mut MINMAXINFO);
            let dpi = GetDpiForWindow(hwnd);
            let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };
            mmi.ptMinTrackSize.x = (MIN_WIDTH_LOGICAL * scale).round() as i32;
            mmi.ptMinTrackSize.y = (MIN_HEIGHT_LOGICAL * scale).round() as i32;
            LRESULT(0)
        }
        WM_SIZE => {
            let result = DefWindowProcW(hwnd, msg, wparam, lparam);
            if wparam.0 as u32 == SIZE_MINIMIZED {
                MINIMIZED.with(|m| m.borrow_mut().insert(hwnd.0 as isize));
            } else {
                let w = (lparam.0 & 0xFFFF) as i32;
                let h = ((lparam.0 >> 16) & 0xFFFF) as i32;
                on_resize(hwnd, w, h);
                if let Some(win) = window_by_hwnd(hwnd) {
                    if win.label() == MAIN {
                        crate::app::window::remember_window_state(&win);
                    }
                    emit_to_window(
                        win.label(),
                        "tauri://resize",
                        serde_json::json!({ "width": w.max(0), "height": h.max(0) }),
                    );
                }
                if MINIMIZED.with(|m| m.borrow_mut().remove(&(hwnd.0 as isize))) {
                    let raw = hwnd.0 as isize;
                    post(move || restore_focus(HWND(raw as *mut _), None, true));
                }
            }
            result
        }
        WM_MOVE => {
            if let Some(win) = window_by_hwnd(hwnd) {
                if win.label() == MAIN {
                    crate::app::window::remember_window_state(&win);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CLOSE => {
            if crate::app::is_quitting() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            if let Some(win) = window_by_hwnd(hwnd) {
                crate::app::on_window_close_requested(win.label());
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_ACTIVATE => {
            let state = (wparam.0 & 0xFFFF) as u32;
            let minimized = (wparam.0 >> 16) & 0xFFFF != 0;
            if state == WA_INACTIVE || minimized {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let previous = win_state(hwnd).map(|(_, focus)| focus);
            ACTIVATING.set(true);
            let result = DefWindowProcW(hwnd, msg, wparam, lparam);
            ACTIVATING.set(false);
            let only_if_lost = state == WA_CLICKACTIVE;
            let raw = hwnd.0 as isize;
            post(move || restore_focus(HWND(raw as *mut _), previous, only_if_lost));
            result
        }
        WM_SETFOCUS => {
            if ACTIVATING.get() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let previous = win_state(hwnd).map(|(_, focus)| focus);
            let result = DefWindowProcW(hwnd, msg, wparam, lparam);
            let raw = hwnd.0 as isize;
            post(move || restore_focus(HWND(raw as *mut _), previous, false));
            result
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if let Some((label, _)) = win_state(hwnd) {
                let raw = hwnd.0 as isize;
                post(move || restore_focus(HWND(raw as *mut _), None, true));
                let alt = lparam.0 & (1 << 29) != 0 || key_down(VK_MENU);
                let ctrl = key_down(VK_CONTROL);
                let shift = key_down(VK_SHIFT);
                if let Some(action) = bound_action(ctrl, alt, shift, wparam.0 as u32) {
                    let repeat = lparam.0 & (1 << 30) != 0;
                    emit_to_window(&label, "shortcut", ShortcutEvent { action, repeat, source: None, keys: None });
                    return LRESULT(0);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

thread_local! {
    static ACTIVATING: Cell<bool> = const { Cell::new(false) };
    static MINIMIZED: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
}
