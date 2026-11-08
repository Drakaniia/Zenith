//! Page webviews: creation, WebView2 event wiring and per-tab commands.
//!
//! This file holds the tab types, creation and the per-tab commands. Event wiring is in
//! `events` (with `new_window`, `permissions`, `requests` and `context_menu` for the larger
//! handlers), injected scripts in `scripts`, find in page in `find` and tab sleeping in `sleep`.

mod context_menu;
mod events;
mod find;
mod new_window;
mod permissions;
mod requests;
mod scripts;
mod sleep;

pub use find::find;
pub use permissions::{respond_permission, PendingPermission};
pub use requests::TOTAL_BLOCKED;
pub use sleep::{content_size, sleep};

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use serde::Serialize;
use std::result::Result;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, HSTRING, PWSTR};
use wry::{MemoryUsageLevel, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

use events::wire;
use scripts::{EXT_POPUP_SCRIPT, PAGE_SCRIPT, STORE_SCRIPT};
use super::{config, emit_tab, post, with_host, with_win};

// ---------------------------------------------------------------------------------------------
// Events sent to the chrome UI

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TabPatch {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_go_back: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_go_forward: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crashed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fullscreen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl TabPatch {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            ..Default::default()
        }
    }
    fn send(self) {
        let id = self.id.clone();
        emit_tab(&id, "tab-update", self);
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpenRequest {
    url: String,
    opener_id: Option<String>,
    /// "glance" | "background" | "split" | "foreground"
    kind: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TabRef {
    id: String,
}

// ---------------------------------------------------------------------------------------------
// Tab state

/// Per-tab values the WebView2 handlers read and write without touching the host.
pub struct TabShared {
    pub id: String,
    /// Label of the window the tab lives in.
    pub window: String,
    /// A tab of an incognito window (InPrivate profile, no history).
    pub private: bool,
    pub glance: Cell<bool>,
    /// An extension page (toolbar popup): drawn on white like in Chrome, never on the theme's
    /// page colour.
    pub light_bg: bool,
    pub nav_uri: RefCell<String>,
    pub page_host: RefCell<String>,
    pub url: RefCell<String>,
    pub title: RefCell<String>,
    pub blocked: Cell<u32>,
    /// Web requests the page made since its last navigation (all frames and workers).
    pub requests: Cell<u32>,
    pub flush_pending: Cell<bool>,
    pub restore_scroll: Cell<Option<(f64, f64)>>,
    /// DevTools target id of this view (used to tell Zenith's pages from stray ones).
    pub target_id: RefCell<String>,
    /// Videos and audio found on the current page (media downloader).
    pub media: RefCell<crate::media::TabMedia>,
    /// Id of the injected "download video" button script, when installed.
    pub button_script: RefCell<Option<String>>,
}

impl TabShared {
    fn new(id: &str, window: &str, glance: bool, light_bg: bool) -> Self {
        Self {
            id: id.to_string(),
            window: window.to_string(),
            private: super::is_private_window(window),
            glance: Cell::new(glance),
            light_bg,
            nav_uri: RefCell::new(String::new()),
            page_host: RefCell::new(String::new()),
            url: RefCell::new(String::new()),
            title: RefCell::new(String::new()),
            blocked: Cell::new(0),
            requests: Cell::new(0),
            flush_pending: Cell::new(false),
            restore_scroll: Cell::new(None),
            target_id: RefCell::new(String::new()),
            media: RefCell::new(Default::default()),
            button_script: RefCell::new(None),
        }
    }
}

pub struct Tab {
    pub view: wry::WebView,
    pub shared: Rc<TabShared>,
    pub visible: bool,
    pub last_rect: Option<(f64, f64, f64, f64)>,
    /// The window region showing only the top-left of the webview (pixels), see `PaneSpec::vis_w`.
    pub last_clip: Option<(i32, i32)>,
    pub find: Option<ICoreWebView2Find>,
}

impl Tab {
    pub fn core(&self) -> ICoreWebView2 {
        self.view.webview()
    }

    /// Label of the window the tab lives in.
    pub fn window(&self) -> &str {
        &self.shared.window
    }

    pub fn set_memory_level(&self, low: bool) {
        let _ = self.view.set_memory_usage_level(if low {
            MemoryUsageLevel::Low
        } else {
            MemoryUsageLevel::Normal
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Creation

pub struct CreateOptions {
    pub id: String,
    /// Label of the window the tab belongs to.
    pub window: String,
    pub url: Option<String>,
    pub glance: bool,
    pub muted: bool,
    pub zoom: Option<f64>,
    pub scroll: Option<(f64, f64)>,
    /// For an extension's toolbar popup: the address of the page it was opened over.
    pub opener_url: Option<String>,
}

/// Creates the webview for a tab. Must run on the UI thread outside any host borrow.
pub fn create(opts: CreateOptions) -> Result<ICoreWebView2, String> {
    let label = opts.window.clone();
    let snapshot = with_host(|h| {
        Some((
            h.wins.get(&label)?.window.clone(),
            h.env.clone(),
            h.page_bg,
            h.pane_for(&label, &opts.id),
            h.tabs.get(&opts.id).map(|t| t.core()),
            h.content_scheme,
        ))
    })
    .flatten();
    let (window, env, bg, pane, existing, scheme) = snapshot.ok_or("browser is not ready")?;
    let private = super::is_private_window(&label);
    if let Some(core) = existing {
        if let Some(url) = &opts.url {
            unsafe {
                let _ = core.Navigate(&HSTRING::from(url.as_str()));
            }
        }
        return Ok(core);
    }

    super::set_tab_window(&opts.id, &label);
    let light_bg = opts.url.as_deref().is_some_and(|u| u.starts_with("chrome-extension://"));
    let bg = if light_bg { (255, 255, 255) } else { bg };
    let shared = Rc::new(TabShared::new(&opts.id, &label, opts.glance, light_bg));
    shared.restore_scroll.set(opts.scroll);
    let rect = pane.as_ref().map(super::logical_rect).unwrap_or_default();
    let mut builder = WebViewBuilder::new()
        .with_visible(false)
        .with_bounds(rect)
        .with_environment(env.clone())
        .with_devtools(true)
        .with_hotkeys_zoom(true)
        .with_back_forward_navigation_gestures(true)
        .with_background_color((bg.0, bg.1, bg.2, 255))
        .with_browser_accelerator_keys(true)
        .with_default_context_menus(true)
        // Incognito windows: their tabs share one in-memory InPrivate profile, which WebView2
        // discards once the last of them closes.
        .with_incognito(private)
        .with_initialization_script_for_main_only(PAGE_SCRIPT, false)
        // Login forms: save offers and saved-account suggestions (top-level document only).
        .with_initialization_script_for_main_only(super::autofill::PAGE_SCRIPT, true);
    if !private {
        builder = builder.with_initialization_script_for_main_only(STORE_SCRIPT, true);
    }
    if light_bg && opts.glance {
        let opener = serde_json::to_string(&opts.opener_url).unwrap_or_else(|_| "null".into());
        builder = builder.with_initialization_script_for_main_only(&EXT_POPUP_SCRIPT.replace("__OPENER__", &opener), true);
    }
    // The first navigation starts below, after the optional media button script is registered.
    let view = builder.build_as_child(&window).map_err(|e| e.to_string())?;
    let core = view.webview();
    unsafe {
        if let Ok(settings) = core.Settings() {
            let _ = settings.SetIsStatusBarEnabled(true);
            let _ = settings.SetIsWebMessageEnabled(false);
        }
        if opts.muted {
            if let Ok(c8) = core.cast::<ICoreWebView2_8>() {
                let _ = c8.SetIsMuted(true);
            }
        }
        if private {
            if let Ok(profile) = core.cast::<ICoreWebView2_13>().and_then(|c| c.Profile()) {
                let _ = profile.SetPreferredColorScheme(scheme);
            }
        }
    }
    if let Some(z) = opts.zoom {
        let _ = view.zoom(z);
    }
    wire(&view, &core, &env, shared.clone());
    super::downloads::attach(&core, shared.clone());
    crate::media::sniff::attach(&core, shared.clone());
    if config().media_button {
        crate::media::sniff::install_button(&core, &shared, true);
    }
    if let Some(url) = &opts.url {
        unsafe {
            let _ = core.Navigate(&HSTRING::from(url.as_str()));
        }
    }
    {
        let shared = shared.clone();
        super::cdp_call(&core, "Target.getTargetInfo", "{}", move |r| {
            if let Some(id) = r.ok().as_deref().and_then(super::target_id_of) {
                *shared.target_id.borrow_mut() = id;
            }
        });
    }

    let id = opts.id.clone();
    let tab = Tab {
        view,
        shared,
        visible: false,
        last_rect: None,
        last_clip: None,
        find: None,
    };
    with_host(move |h| {
        h.tabs.insert(id, tab);
        h.apply_layout(&label);
    });
    Ok(core)
}

fn take(p: PWSTR) -> String {
    take_pwstr(p)
}

fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

// ---------------------------------------------------------------------------------------------
// Commands

/// Destroys a tab's webview. Tabs with running downloads are kept alive (hidden) until the
/// downloads finish, because WebView2 cancels downloads of closed webviews.
pub fn close(id: &str) {
    super::autofill::forget(id);
    let label = super::window_of_tab(id);
    let removed = with_host(|h| {
        if let Some(w) = h.wins.get_mut(&label) {
            if w.fullscreen_tab.as_deref() == Some(id) {
                w.fullscreen_tab = None;
            }
        }
        h.permissions.retain(|_, p| p.tab != id);
        let tab = h.tabs.remove(id)?;
        let busy = h.downloads.values().any(|d| d.tab == id && d.record.state == "progress");
        if busy {
            let _ = tab.view.set_visible(false);
            tab.set_memory_level(true);
            h.zombies.insert(id.to_string(), tab);
            None
        } else {
            Some(tab)
        }
    })
    .flatten();
    if let Some(tab) = removed {
        post(move || drop(tab));
    }
    let window = with_win(&label, |w| w.window.clone());
    if let Some(w) = window {
        if w.is_fullscreen().unwrap_or(false) {
            let _ = w.set_fullscreen(false);
        }
    }
}

pub fn navigate(id: &str, url: &str) -> Result<(), String> {
    let core = with_host(|h| h.tabs.get(id).map(|t| t.core())).flatten().ok_or("tab has no webview")?;
    unsafe { core.Navigate(&HSTRING::from(url)) }.map_err(|e| e.to_string())
}

pub fn set_glance(id: &str, glance: bool) {
    with_host(|h| {
        if let Some(t) = h.tabs.get(id) {
            t.shared.glance.set(glance);
        }
    });
}

pub fn command(id: &str, cmd: &str, arg: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    let (core, controller) = with_host(|h| h.tabs.get(id).map(|t| (t.core(), t.view.controller())))
        .flatten()
        .ok_or("tab has no webview")?;
    let num = |default: f64| arg.as_ref().and_then(|v| v.as_f64()).unwrap_or(default);
    unsafe {
        match cmd {
            "back" => core.GoBack().map_err(|e| e.to_string())?,
            "forward" => core.GoForward().map_err(|e| e.to_string())?,
            "reload" => core.Reload().map_err(|e| e.to_string())?,
            "hardReload" => {
                core.CallDevToolsProtocolMethod(
                    &HSTRING::from("Page.reload"),
                    &HSTRING::from(r#"{"ignoreCache":true}"#),
                    &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(()))),
                )
                .map_err(|e| e.to_string())?;
            }
            "stop" => core.Stop().map_err(|e| e.to_string())?,
            "devtools" => core.OpenDevToolsWindow().map_err(|e| e.to_string())?,
            "print" => {
                let c16: ICoreWebView2_16 = core.cast().map_err(|e| e.to_string())?;
                c16.ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_BROWSER).map_err(|e| e.to_string())?;
            }
            "saveAs" => {
                let c25: ICoreWebView2_25 = core.cast().map_err(|e| e.to_string())?;
                c25.ShowSaveAsUI(&ShowSaveAsUICompletedHandler::create(Box::new(|_, _| Ok(()))))
                    .map_err(|e| e.to_string())?;
            }
            "mute" | "unmute" => {
                let c8: ICoreWebView2_8 = core.cast().map_err(|e| e.to_string())?;
                c8.SetIsMuted(cmd == "mute").map_err(|e| e.to_string())?;
            }
            "zoom" => {
                let z = num(1.0).clamp(0.25, 5.0);
                controller.SetZoomFactor(z).map_err(|e| e.to_string())?;
                let mut applied = z;
                let _ = controller.ZoomFactor(&mut applied);
                TabPatch { zoom: Some(applied), ..TabPatch::new(id) }.send();
                return Ok(serde_json::json!(applied));
            }
            "suspend" => {
                let c3: ICoreWebView2_3 = core.cast().map_err(|e| e.to_string())?;
                let _ = c3.TrySuspend(&TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(()))));
            }
            "resume" => {
                let c3: ICoreWebView2_3 = core.cast().map_err(|e| e.to_string())?;
                let _ = c3.Resume();
            }
            "viewSource" => {
                let mut uri = PWSTR::null();
                core.Source(&mut uri).map_err(|e| e.to_string())?;
                let src = take(uri);
                return Ok(serde_json::json!(format!("view-source:{src}")));
            }
            other => return Err(format!("unknown tab command {other}")),
        }
    }
    Ok(serde_json::Value::Null)
}
