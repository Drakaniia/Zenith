//! Browser windows: building the normal window (`main`), extra normal windows (`window-N`) and
//! incognito ones, placing them, and remembering the main window's geometry.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::utils::config::WindowEffectsConfig;
use tauri::webview::ScrollBarStyle;
use tauri::window::Effect;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::startup::set_startup_urls;
use crate::platform::win;
use crate::{host, incognito, profile_commands, profiles, session, storage};

pub(crate) static NEXT_INCOGNITO: AtomicU32 = AtomicU32::new(1);
static NEXT_WINDOW: AtomicU32 = AtomicU32::new(1);

/// The saved state was maximized. Applied when the window is first shown (`ui_ready`):
/// maximizing a still-hidden window leaves it flagged maximized at its restored size, so it
/// never fills the screen and the webviews never get the maximized client size.
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

/// Geometry of the normal window. Incognito windows open next to it and never save their own.
fn window_state_file() -> std::path::PathBuf {
    host::persistent_paths().roaming.join("window.json")
}

pub(crate) fn remember_window_state(window: &tauri::Window) {
    let maximized = window.is_maximized().unwrap_or(false);
    if window.is_minimized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }
    let Ok(mut guard) = WINDOW_STATE.lock() else { return };
    if maximized {
        if let Some(s) = guard.as_mut() {
            s.maximized = true;
        }
        return;
    }
    if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
        *guard = Some(WindowState {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            maximized: false,
        });
    }
}

pub(crate) fn save_window_state() {
    if let Some(state) = WINDOW_STATE.lock().ok().and_then(|s| *s) {
        if let Ok(json) = serde_json::to_vec(&state) {
            let _ = storage::write_atomic(&window_state_file(), &json);
        }
    }
}

fn restore_window_state(window: &WebviewWindow) {
    let Some(state) = storage::read_text(&window_state_file())
        .and_then(|s| serde_json::from_str::<WindowState>(&s).ok())
    else {
        let _ = window.center();
        return;
    };
    // Only restore onto a monitor that still exists.
    let visible = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|m| {
            let (p, s) = (m.position(), m.size());
            state.x + 80 < p.x + s.width as i32
                && state.x + state.width as i32 - 80 > p.x
                && state.y >= p.y - 8
                && state.y + 40 < p.y + s.height as i32
        });
    if visible && state.width >= 400 && state.height >= 300 {
        let _ = window.set_size(PhysicalSize::new(state.width, state.height));
        let _ = window.set_position(PhysicalPosition::new(state.x, state.y));
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

fn browser_args() -> String {
    // msWebOOUI: Edge "mini menu" on text selection; msPdfOOUI: Edge PDF toolbar pop-ups.
    #[allow(unused_mut)]
    let mut args = String::from("--disable-features=msWebOOUI,msPdfOOUI");
    #[cfg(any(debug_assertions, feature = "remote-debug"))]
    if let Ok(port) = std::env::var("ZENITH_REMOTE_DEBUG_PORT") {
        if port.parse::<u16>().is_ok() {
            args.push_str(" --remote-debugging-port=");
            args.push_str(&port);
        }
    }
    args
}

/// An incognito or extra window opens over the window the user is in (else the main one),
/// offset like a cascaded window.
fn place_cascaded(app: &AppHandle, window: &WebviewWindow) {
    let others: Vec<WebviewWindow> = app.webview_windows().into_values().filter(|w| w.label() != window.label()).collect();
    let base = others
        .iter()
        .find(|w| w.is_focused().unwrap_or(false))
        .or_else(|| others.iter().find(|w| w.label() == host::MAIN))
        .or_else(|| others.first())
        .cloned();
    let Some(main) = base else {
        let _ = window.center();
        return;
    };
    if main.is_maximized().unwrap_or(false) {
        let _ = window.center();
        return;
    }
    if let (Ok(pos), Ok(size)) = (main.outer_position(), main.inner_size()) {
        let step = (32.0 * main.scale_factor().unwrap_or(1.0)) as i32;
        let _ = window.set_size(size);
        let _ = window.set_position(PhysicalPosition::new(pos.x + step, pos.y + step));
    }
}

/// Builds a browser window: the normal window (`main`) or an incognito one. Every window
/// passes the same WebView2 options, so all of them share one WebView2 browser process.
pub(crate) fn create_window(app: &AppHandle, label: &str) -> tauri::Result<()> {
    let private = host::is_private_window(label);
    let extra = session::is_extra_window(label);
    let title = window_title(private);
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(&title)
        .inner_size(1280.0, 820.0)
        .min_inner_size(560.0, 400.0)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .visible(false);
    // Mica needs Windows 11 with transparency allowed; otherwise the UI paints the solid theme.
    // The UI's settings then pick the material the user chose.
    if win::translucency_blocker().is_none() {
        builder = builder.effects(WindowEffectsConfig {
            effects: vec![Effect::Mica],
            ..Default::default()
        });
    }
    if private {
        // Known to the UI before its first render, so it never restores or saves a session.
        builder = builder.initialization_script("window.__ZENITH_INCOGNITO__ = true;");
    }
    let window = builder
        .data_directory(host::paths().webview_data())
        .additional_browser_args(&browser_args())
        .browser_extensions_enabled(true)
        .scroll_bar_style(ScrollBarStyle::FluentOverlay)
        .disable_drag_drop_handler()
        .zoom_hotkeys_enabled(false)
        .devtools(cfg!(debug_assertions))
        .build()?;
    if private || extra {
        place_cascaded(app, &window);
    } else {
        restore_window_state(&window);
    }
    // A profile with a picture shows it as its window (and taskbar) icon.
    if !private {
        if let Some(icon) = profile_commands::window_icon() {
            let _ = window.set_icon(icon);
        }
    }
    let w = window.clone();
    window.with_webview(move |pw| host::attach_chrome(w, pw.controller(), pw.environment()))?;
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

/// Opens a new incognito window that loads `urls` once its UI is ready. Call off the UI thread.
pub fn open_incognito_window(app: &AppHandle, urls: Vec<String>) -> tauri::Result<()> {
    let label = format!("{}{}", incognito::LABEL_PREFIX, NEXT_INCOGNITO.fetch_add(1, Ordering::Relaxed));
    set_startup_urls(&label, urls);
    create_window(app, &label)
}

/// Opens another normal window (`window-N`) that starts with `start`. Call off the UI thread.
pub fn open_normal_window(app: &AppHandle, start: session::Start) -> tauri::Result<()> {
    let label = format!("{}{}", session::LABEL_PREFIX, NEXT_WINDOW.fetch_add(1, Ordering::Relaxed));
    session::set_pending(&label, start);
    create_window(app, &label)
}

/// Reopens the main window (closed while other windows stayed open) with all of its tabs.
pub fn reopen_main_window(app: &AppHandle) -> tauri::Result<()> {
    session::set_pending(host::MAIN, session::Start { data: None, keep: Some(true), fresh: false });
    create_window(app, host::MAIN)
}
