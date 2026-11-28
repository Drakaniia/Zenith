//! The tab host: owns every CEF page browser and trusted chrome surface, keeping page child
//! HWNDs positioned and z-ordered inside Zenith's Win32 top-level windows.

pub mod autofill;
pub mod cef_app;
pub mod commands;
pub mod downloads;
pub mod page_tools;
pub mod tab;
pub mod window_proc;

mod accelerators;
mod chrome;
mod config;
mod debug;
mod devtools;
mod devtools_view;
mod fast_download;
mod focus;
pub mod perf;
mod placement;
mod profile;
mod state;

pub use accelerators::{
    bound_action, emit_shortcut, handle_devtools_key_event, handle_pre_key_event, key_down,
    set_bindings, ShortcutEvent,
};
pub use chrome::{
    close_all_for_exit, create_chrome_window, detach_window, init_services, open_chrome_devtools,
    profile_override, shutdown, STARTED,
};
pub use config::{apply_config, blocker_enabled, config, data_saver_enabled, set_extensions_in_incognito, RuntimeConfig, BLOCKER};
pub use debug::{debug_layout, DebugLayout};
pub(crate) use devtools::{
    browser_cdp_pipe_arg, close_browser_cdp_pipes, eval_js, init_browser_cdp_pipes, target_id_of,
};
pub use devtools::{browser_cdp, cdp_call, chrome_cdp};
pub(crate) use devtools_view::{
    emit_devtools_changed, handle_devtools_command, open_tab_devtools, TabDevTools,
};
pub use focus::focus_view;
pub use placement::{set_fullscreen_tab, set_layout};
pub use profile::{clear_browsing_data, incognito_context, maybe_drop_incognito_context, set_content_color_scheme, set_page_background};
pub(crate) use state::set_tab_window;
pub use state::{app, bookmarks_db, db, paths, persistent_paths, MAIN};
pub use state::{emit, emit_tab, emit_to_window, is_normal_window, is_private_window, primary_window, tab_is_private, window_labels, window_of_tab};

use std::cell::RefCell;
use std::collections::HashMap;

use cef::*;
use serde::Deserialize;
use windows::Win32::Foundation::HWND;

use crate::app::bridge::{send_event_to_browser, send_json_message, InvokeResponse, IPC_RESPONSE_MSG};
use crate::app::runtime::{post_ui, WindowHandle};
use tab::Tab;

#[derive(Clone, Debug, PartialEq)]
pub enum Focus {
    Chrome,
    Tab(String),
    DevTools(String),
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PaneSpec {
    #[serde(default)]
    pub cw: f64,
    #[serde(default)]
    pub ch: f64,
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OverlayRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub r: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSpec {
    #[serde(default)]
    pub panes: Vec<PaneSpec>,
    #[serde(default)]
    pub glance: Option<PaneSpec>,
    #[serde(default)]
    pub devtools: Option<PaneSpec>,
    #[serde(default)]
    pub overlay: bool,
    #[serde(default)]
    pub overlay_rects: Vec<OverlayRect>,
    pub viewport_w: f64,
    pub viewport_h: f64,
}

/// One browser window: its Win32 frame, Alloy trusted UI browser, and pane layout.
pub struct Win {
    pub window: WindowHandle,
    pub hwnd: HWND,
    pub chrome: Option<Browser>,
    pub chrome_hwnd: HWND,
    pub chrome_browser_id: i32,
    pub layout: LayoutSpec,
    pub focus: Focus,
    pub fullscreen_tab: Option<String>,
    pub chrome_raised: Option<bool>,
    pub chrome_rgn_key: Option<Vec<(i32, i32, i32, i32, i32)>>,
    pub chrome_target_id: String,
}

pub struct Host {
    pub wins: HashMap<String, Win>,
    pub tabs: HashMap<String, Tab>,
    pub warm_tab: Option<tab::WarmTab>,
    /// Shared non-persistent in-memory CEF RequestContext for incognito tabs.
    pub incognito_ctx: Option<RequestContext>,
    /// Every live CEF browser (trusted, tab, or extension-created) so shutdown closes all cleanly.
    pub all_browsers: HashMap<i32, Browser>,
    pub page_bg: (u8, u8, u8),
    pub content_scheme: String,
    pub permissions: HashMap<u32, tab::PendingPermission>,
    pub cert_errors: HashMap<u32, crate::security::PendingCertError>,
    pub protocol_prompts: HashMap<u32, crate::security::PendingProtocol>,
    pub pending_downloads: HashMap<u32, crate::security::PendingDownloadConfirm>,
    pub downloads: HashMap<i64, downloads::ActiveDownload>,
    pub target_owner: String,
    pub shutting_down: bool,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            wins: HashMap::new(),
            tabs: HashMap::new(),
            warm_tab: None,
            incognito_ctx: None,
            all_browsers: HashMap::new(),
            page_bg: (32, 32, 36),
            content_scheme: "system".into(),
            permissions: HashMap::new(),
            cert_errors: HashMap::new(),
            protocol_prompts: HashMap::new(),
            pending_downloads: HashMap::new(),
            downloads: HashMap::new(),
            target_owner: MAIN.into(),
            shutting_down: false,
        }
    }
}

thread_local! {
    pub(super) static HOST: RefCell<Host> = RefCell::new(Host::default());
}

#[track_caller]
pub fn with_host<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    let caller = std::panic::Location::caller();
    HOST.with(|cell| match cell.try_borrow_mut() {
        Ok(mut guard) => Some(f(&mut guard)),
        Err(_) => {
            eprintln!("[zenith] host re-entrancy at {}:{}", caller.file(), caller.line());
            None
        }
    })
}

pub(crate) fn with_win<R>(label: &str, f: impl FnOnce(&mut Win) -> R) -> Option<R> {
    with_host(|h| h.wins.get_mut(label).map(f)).flatten()
}

/// Posts a closure onto the CEF UI (Win32 main) thread asynchronously.
pub(crate) fn post(f: impl FnOnce() + Send + 'static) {
    let _ = post_ui(f);
}

pub(crate) fn send_chrome_event(label: &str, event: &str, payload: serde_json::Value) {
    let browser = with_host(|h| h.wins.get(label).and_then(|w| w.chrome.clone())).flatten();
    if let Some(b) = browser {
        send_event_to_browser(&b, event, payload);
    }
}

pub(crate) fn broadcast_chrome_event(event: &str, payload: serde_json::Value) {
    let browsers: Vec<Browser> = with_host(|h| {
        h.wins.values().filter_map(|w| w.chrome.clone()).collect()
    })
    .unwrap_or_default();
    for b in browsers {
        send_event_to_browser(&b, event, payload.clone());
    }
}

pub(crate) fn send_chrome_response(browser_id: i32, resp: InvokeResponse) {
    let browser = with_host(|h| {
        h.wins
            .values()
            .find(|w| w.chrome_browser_id == browser_id)
            .and_then(|w| w.chrome.clone())
    })
    .flatten();
    if let Some(b) = browser {
        if let Some(frame) = b.main_frame() {
            send_json_message(&frame, IPC_RESPONSE_MSG, &resp);
        }
    }
}

pub(crate) fn track_browser(browser: &Browser) {
    with_host(|h| {
        h.all_browsers.insert(browser.identifier(), browser.clone());
    });
}

pub(crate) fn untrack_browser(browser_id: i32) {
    devtools::detach_observer(browser_id);
    let should_quit = with_host(|h| {
        h.all_browsers.remove(&browser_id);
        h.shutting_down && h.all_browsers.is_empty()
    })
    .unwrap_or(false);
    if should_quit {
        cef::quit_message_loop();
    }
}
