//! Pure Win32 browser windows: borderless frameless top-level HWND hosting a CEF Alloy
//! trusted UI child browser and native CEF Chrome-style page child windows.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use windows::core::{w, HSTRING};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::runtime::{register_window, AppHandle, WindowHandle};
use super::startup::set_startup_urls;
use crate::{host, incognito, profiles, session, storage};

pub(crate) static NEXT_INCOGNITO: AtomicU32 = AtomicU32::new(1);
static NEXT_WINDOW: AtomicU32 = AtomicU32::new(1);
static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

/// The saved state was maximized. Applied when the window is first shown (`ui_ready`).
pub static RESTORE_MAXIMIZED: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    maximized: bool,
}

static WINDOW_STATE: Mutex<Option<WindowState>> = Mutex::new(None);

fn window_state_file() -> std::path::PathBuf {
    host::persistent_paths().roaming.join("window.json")
}

pub(crate) fn remember_window_state(window: &WindowHandle) {
    let maximized = window.is_maximized();
    if window.is_minimized() || window.is_fullscreen() {
        return;
    }
    let Ok(mut guard) = WINDOW_STATE.lock() else { return };
    if maximized {
        if let Some(s) = guard.as_mut() {
            s.maximized = true;
        }
        return;
    }
    if let (Ok((x, y)), Ok((width, height))) = (window.outer_position(), window.inner_size()) {
        *guard = Some(WindowState { x, y, width, height, maximized: false });
    }
}

pub(crate) fn save_window_state() {
    if let Some(state) = WINDOW_STATE.lock().ok().and_then(|s| *s) {
        if let Ok(json) = serde_json::to_vec(&state) {
            let _ = storage::write_atomic(&window_state_file(), &json);
        }
    }
}

fn restore_window_state(window: &WindowHandle) {
    let Some(state) = storage::read_text(&window_state_file())
        .and_then(|s| serde_json::from_str::<WindowState>(&s).ok())
    else {
        let _ = window.center();
        return;
    };
    let visible = window.available_monitors().iter().any(|m| {
        state.x + 80 < m.x + m.width as i32
            && state.x + state.width as i32 - 80 > m.x
            && state.y >= m.y - 8
            && state.y + 40 < m.y + m.height as i32
    });
    if visible && state.width >= 400 && state.height >= 300 {
        let _ = window.set_size(state.width, state.height);
        let _ = window.set_position(state.x, state.y);
    } else {
        let _ = window.center();
    }
    if state.maximized {
        RESTORE_MAXIMIZED.store(true, Ordering::SeqCst);
    }
    if let Ok(mut guard) = WINDOW_STATE.lock() {
        *guard = Some(state);
    }
}

fn place_cascaded(app: &AppHandle, window: &WindowHandle) {
    let others: Vec<WindowHandle> = app
        .windows()
        .into_values()
        .filter(|w| w.label() != window.label())
        .collect();
    let base = others
        .iter()
        .find(|w| w.is_focused())
        .or_else(|| others.iter().find(|w| w.label() == host::MAIN))
        .or_else(|| others.first())
        .cloned();
    let Some(main) = base else {
        let _ = window.center();
        return;
    };
    if main.is_maximized() {
        let _ = window.center();
        return;
    }
    if let (Ok((x, y)), Ok((w, h))) = (main.outer_position(), main.inner_size()) {
        let step = (32.0 * main.scale_factor()) as i32;
        let _ = window.set_size(w, h);
        let _ = window.set_position(x + step, y + step);
    }
}

fn ensure_window_class() -> Result<(), String> {
    if CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    unsafe {
        let hinstance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(host::window_proc::host_wnd_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            lpszClassName: w!("ZenithBrowserWindow"),
            ..Default::default()
        };
        RegisterClassW(&wc);
    }
    Ok(())
}

/// Builds a frameless top-level Win32 browser window and embeds its CEF Alloy trusted UI.
/// Must be called on the CEF UI thread.
pub(crate) fn create_window(app: &AppHandle, label: &str) -> Result<(), String> {
    ensure_window_class()?;
    let private = host::is_private_window(label);
    let extra = session::is_extra_window(label);
    let title = window_title(private);

    let hwnd = unsafe {
        let hinstance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("ZenithBrowserWindow"),
            &HSTRING::from(&title),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1280,
            820,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("CreateWindowExW: {e}"))?
    };

    // Enable DWM drop shadow and Windows 11 rounded corners on the borderless frame.
    unsafe {
        let margins = MARGINS { cxLeftWidth: 1, cxRightWidth: 1, cyTopHeight: 1, cyBottomHeight: 1 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        );
    }

    let window = WindowHandle::new(label, hwnd);
    let _ = window.set_theme(None);
    register_window(window.clone());

    if private || extra {
        place_cascaded(app, &window);
    } else {
        restore_window_state(&window);
    }

    if !private {
        if let Some(icon) = profiles::commands::window_icon().or_else(|| app.default_window_icon()) {
            let _ = window.set_icon(icon);
        }
    } else if let Some(icon) = app.default_window_icon() {
        let _ = window.set_icon(icon);
    }

    host::create_chrome_window(window, private)?;
    Ok(())
}

/// Window title: "Zenith", "Zenith — Work" for a non-default profile, "Zenith Incognito".
pub fn window_title(private: bool) -> String {
    let active = profiles::active();
    let name = (!active.is_default()).then(|| active.profile().name);
    match (private, host::profile_override().is_some(), name) {
        (true, _, None) => "Zenith Incognito".into(),
        (true, _, Some(n)) => format!("Zenith Incognito — {n}"),
        (false, true, _) => "Zenith (test instance)".into(),
        (false, false, None) => "Zenith".into(),
        (false, false, Some(n)) => format!("Zenith — {n}"),
    }
}

pub fn open_incognito_window(app: &AppHandle, urls: Vec<String>) -> Result<(), String> {
    let label = format!("{}{}", incognito::LABEL_PREFIX, NEXT_INCOGNITO.fetch_add(1, Ordering::Relaxed));
    set_startup_urls(&label, urls);
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = create_window(&app2, &label) {
            eprintln!("[zenith] open_incognito_window failed: {e}");
        }
    })
}

pub fn open_normal_window(app: &AppHandle, start: session::Start) -> Result<(), String> {
    let label = format!("{}{}", session::LABEL_PREFIX, NEXT_WINDOW.fetch_add(1, Ordering::Relaxed));
    session::set_pending(&label, start);
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = create_window(&app2, &label) {
            eprintln!("[zenith] open_normal_window failed: {e}");
        }
    })
}

pub fn reopen_main_window(app: &AppHandle) -> Result<(), String> {
    session::set_pending(host::MAIN, session::Start { data: None, keep: Some(true), fresh: false });
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = create_window(&app2, host::MAIN) {
            eprintln!("[zenith] reopen_main_window failed: {e}");
        }
    })
}
