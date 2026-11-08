//! The tab host: owns every page webview and keeps them positioned under the chrome UI.
//!
//! Architecture (see plan.md section 6):
//! - Each browser window has its own chrome UI: a Tauri webview, transparent and filling the
//!   window (`main`, plus `incognito-N` for incognito windows, all in this one process).
//! - Every tab is a raw `wry` child webview (no Tauri IPC bridge) sharing the chrome's
//!   WebView2 environment, so all views live in one browser process.
//! - Floating UI (palette, compact sidebar, Glance, menus) is drawn by the chrome. When it is
//!   open the chrome's HWND is raised above the tab HWNDs; its transparent pixels let the live
//!   page show through (verified in the Phase 0 spike). Otherwise the chrome sits at the bottom.
//!
//! All state here is main-thread only (WebView2 is single-threaded apartment). Commands reach
//! it through `AppHandle::run_on_main_thread`; WebView2 callbacks already run on that thread.
//!
//! This file holds the host and window state, `with_host` and the UI-thread dispatcher (`post`).
//! The rest is split by concern: `state` (process-wide services, event emitting, the tab-window
//! map), `config` (runtime settings, blocker state), `chrome` (window attach/detach, startup and
//! shutdown), `window_proc` (the window subclass), `focus`, `placement` (layout and z-order),
//! `accelerators` (shortcuts), `devtools` (DevTools calls, stray targets), `profile` (page
//! profile settings and browsing data) and `debug` (layout snapshots).

pub mod autofill;
pub mod commands;
pub mod downloads;
pub mod page_tools;
pub mod tab;

mod accelerators;
mod chrome;
mod config;
mod debug;
mod devtools;
mod focus;
mod placement;
mod profile;
mod state;
mod window_proc;

pub use accelerators::{attach_accelerators, emit_shortcut, set_bindings};
pub use chrome::{attach_chrome, detach_window, init_services, profile_override, shutdown, STARTED};
pub use config::{apply_config, blocker_enabled, config, RuntimeConfig, BLOCKER};
pub use debug::{debug_layout, DebugLayout};
pub(crate) use devtools::target_id_of;
pub use devtools::{cdp_call, chrome_cdp};
pub use focus::focus_view;
pub(crate) use placement::logical_rect;
pub use placement::{set_fullscreen_tab, set_layout};
pub use profile::{clear_browsing_data, set_content_color_scheme, set_page_background};
pub(crate) use state::set_tab_window;
pub use state::{app, bookmarks_db, db, paths, persistent_paths, MAIN};
pub use state::{emit, emit_tab, emit_to_window, is_normal_window, is_private_window, primary_window, tab_is_private, window_of_tab};

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::LazyLock;

use serde::Deserialize;
use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

use tab::Tab;

// ---------------------------------------------------------------------------------------------
// Main-thread host state

#[derive(Clone, Debug, PartialEq)]
pub enum Focus {
    Chrome,
    Tab(String),
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PaneSpec {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSpec {
    #[serde(default)]
    pub panes: Vec<PaneSpec>,
    #[serde(default)]
    pub glance: Option<PaneSpec>,
    #[serde(default)]
    pub overlay: bool,
    pub viewport_w: f64,
    pub viewport_h: f64,
}

/// One browser window: its chrome webview and where its pages are placed.
pub struct Win {
    pub window: WebviewWindow,
    pub hwnd: HWND,
    pub chrome: ICoreWebView2Controller,
    pub chrome_core: ICoreWebView2,
    pub chrome_hwnd: HWND,
    pub layout: LayoutSpec,
    pub focus: Focus,
    pub fullscreen_tab: Option<String>,
    pub chrome_raised: Option<bool>,
    /// DevTools target id of the chrome webview (see `check_stray_target`).
    pub chrome_target_id: String,
    _target_watch: Option<ICoreWebView2DevToolsProtocolEventReceiver>,
}

pub struct Host {
    /// Open windows by label.
    pub wins: HashMap<String, Win>,
    /// The WebView2 environment every view shares (one browser process for all windows).
    pub env: ICoreWebView2Environment,
    /// Tabs of every window; `Tab::window()` tells which.
    pub tabs: HashMap<String, Tab>,
    /// Tabs the user closed while they still had downloads running.
    pub zombies: HashMap<String, Tab>,
    pub page_bg: (u8, u8, u8),
    /// prefers-color-scheme for pages, re-applied to the InPrivate profile of incognito tabs.
    pub content_scheme: COREWEBVIEW2_PREFERRED_COLOR_SCHEME,
    pub permissions: HashMap<u32, tab::PendingPermission>,
    pub downloads: HashMap<i64, downloads::ActiveDownload>,
    /// The window whose chrome watches for stray page targets (the first one opened).
    pub target_owner: String,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

/// Runs `f` with the host. Never hold this across calls that pump messages (webview creation),
/// because WebView2 callbacks may run inside the pump and need the host themselves.
#[track_caller]
pub fn with_host<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    let caller = std::panic::Location::caller();
    HOST.with(|cell| match cell.try_borrow_mut() {
        Ok(mut guard) => guard.as_mut().map(f),
        Err(_) => {
            eprintln!("[zenith] host busy at {caller}; skipped");
            None
        }
    })
}

// ---------------------------------------------------------------------------------------------
// UI-thread dispatcher (posted closures) and window subclass

const SUBCLASS_ID: usize = 0x5A45_4E01;
static EXEC_MSG: LazyLock<u32> =
    LazyLock::new(|| unsafe { RegisterWindowMessageW(&windows::core::HSTRING::from("Zenith::Exec")) });
/// A window `post` delivers to. Any open window of this thread works; it is moved to another
/// window when it closes.
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

/// Queues `f` to run on the UI thread after the current WebView2 callback has returned.
/// WebView2 forbids re-entrant work (like creating webviews) inside its event handlers.
pub fn post(f: impl FnOnce() + 'static) {
    let hwnd = HWND(MAIN_HWND.load(Ordering::SeqCst) as *mut _);
    let boxed: Box<Box<dyn FnOnce()>> = Box::new(Box::new(f));
    let raw = Box::into_raw(boxed);
    unsafe {
        if PostMessageW(Some(hwnd), *EXEC_MSG, WPARAM(raw as usize), LPARAM(0)).is_err() {
            drop(Box::from_raw(raw));
        }
    }
}

/// Runs `f` with one window's state.
#[track_caller]
pub fn with_win<R>(label: &str, f: impl FnOnce(&mut Win) -> R) -> Option<R> {
    with_host(|h| h.wins.get_mut(label).map(f)).flatten()
}

/// Labels of the open windows.
pub fn window_labels() -> Vec<String> {
    with_host(|h| h.wins.keys().cloned().collect()).unwrap_or_default()
}
