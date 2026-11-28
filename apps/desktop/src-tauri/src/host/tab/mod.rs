//! Page browsers: creation, CEF Chrome-style child embedding, generation-safe state, and
//! per-tab commands.

mod context_menu;
mod events;
mod find;
mod new_window;
mod permissions;
mod requests;
mod scripts;
mod sleep;
pub mod warm;

pub use find::find;
pub use permissions::{respond_permission, PendingPermission};
pub use requests::{apply_blocking, create_request_context_handler, TOTAL_BLOCKED};
pub use scripts::{apply_triggers, EXT_POPUP_SCRIPT, PAGE_SCRIPT, STORE_SCRIPT};
pub use sleep::{content_size, sleep};
pub use warm::{ensure_warm, release_if_pressured, WarmTab};

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use cef::*;
use serde::Serialize;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, ShowWindow, SW_HIDE, SW_SHOWNOACTIVATE};

pub(crate) use super::placement::create_container;

use super::{
    config, emit_devtools_changed, emit_tab, handle_devtools_command, incognito_context,
    maybe_drop_incognito_context, with_host, with_win, TabDevTools,
};
use crate::platform::urls::host_of;

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
    pub(crate) fn new(id: &str) -> Self {
        Self { id: id.to_string(), ..Default::default() }
    }
    pub(crate) fn send(self) {
        let id = self.id.clone();
        emit_tab(&id, "tab-update", self);
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClickOrigin {
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenRequest {
    pub url: String,
    pub opener_id: Option<String>,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<ClickOrigin>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TabRef {
    pub id: String,
}

/// Thread-safe per-tab state shared across CEF UI and IO thread handlers.
pub struct TabShared {
    id: RwLock<String>,
    window: RwLock<String>,
    warm: AtomicBool,
    pub private: bool,
    pub glance: AtomicBool,
    pub light_bg: bool,
    pub generation: AtomicU64,
    pub closed: AtomicBool,
    pub container_hwnd: AtomicIsize,
    pub nav_uri: RwLock<String>,
    pub page_host: RwLock<String>,
    pub url: RwLock<String>,
    pub title: RwLock<String>,
    pub blocked: AtomicU32,
    pub requests: AtomicU32,
    pub flush_pending: AtomicBool,
    pub restore_scroll: Mutex<Option<(f64, f64)>>,
    pub target_id: RwLock<String>,
    pub media: Mutex<crate::media::TabMedia>,
    /// Pinned/Essential site boundary: `(registrable_site, is_essential)` for cross-site link routing.
    pub pin_site: RwLock<Option<(String, bool)>>,
}

impl TabShared {
    pub(crate) fn new(id: &str, window: &str, glance: bool, light_bg: bool) -> Self {
        Self {
            id: RwLock::new(id.to_string()),
            window: RwLock::new(window.to_string()),
            warm: AtomicBool::new(false),
            private: super::is_private_window(window),
            glance: AtomicBool::new(glance),
            light_bg,
            generation: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            container_hwnd: AtomicIsize::new(0),
            nav_uri: RwLock::new(String::new()),
            page_host: RwLock::new(String::new()),
            url: RwLock::new(String::new()),
            title: RwLock::new(String::new()),
            blocked: AtomicU32::new(0),
            requests: AtomicU32::new(0),
            flush_pending: AtomicBool::new(false),
            restore_scroll: Mutex::new(None),
            target_id: RwLock::new(String::new()),
            media: Mutex::new(Default::default()),
            pin_site: RwLock::new(None),
        }
    }

    pub(crate) fn new_warm(id: &str, window: &str) -> Self {
        let s = Self::new(id, window, false, false);
        s.warm.store(true, Ordering::Release);
        s
    }

    pub fn id(&self) -> String {
        self.id.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn window(&self) -> String {
        self.window.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn is_warm(&self) -> bool {
        self.warm.load(Ordering::Acquire)
    }

    pub fn rebind(&self, id: &str, window: &str, scroll: Option<(f64, f64)>) {
        if let Ok(mut slot) = self.id.write() {
            *slot = id.to_string();
        }
        if let Ok(mut slot) = self.window.write() {
            *slot = window.to_string();
        }
        if let Ok(mut s) = self.restore_scroll.lock() {
            *s = scroll;
        }
        self.warm.store(false, Ordering::Release);
    }

    pub fn is_glance(&self) -> bool {
        self.glance.load(Ordering::Relaxed)
    }

    pub fn url(&self) -> String {
        self.url.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn title(&self) -> String {
        self.title.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn page_host(&self) -> String {
        self.page_host.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn nav_uri(&self) -> String {
        self.nav_uri.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn target_id(&self) -> String {
        self.target_id.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn requests(&self) -> u32 {
        self.requests.load(Ordering::Relaxed)
    }

    pub fn blocked(&self) -> u32 {
        self.blocked.load(Ordering::Relaxed)
    }

    pub fn restore_page_after_download(&self) {
        let url = self.url();
        if url.is_empty() {
            return;
        }
        if let Ok(mut h) = self.page_host.write() {
            *h = host_of(&url).unwrap_or("").to_ascii_lowercase();
        }
        if let Ok(mut n) = self.nav_uri.write() {
            *n = url;
        }
    }
}

pub struct Tab {
    pub container_hwnd: HWND,
    pub browser: Option<Browser>,
    pub browser_hwnd: HWND,
    pub devtools: Option<TabDevTools>,
    pub shared: Arc<TabShared>,
    pub visible: bool,
    pub last_rect: Option<(f64, f64, f64, f64)>,
    pub content_px: Option<(i32, i32)>,
    pub rgn_px: Option<(i32, i32, i32)>,
    pub pending_url: Option<String>,
    pub pending_muted: bool,
    pub pending_zoom: Option<f64>,
    pub find_text: String,
    pub find_case: bool,
}

impl Tab {
    pub fn window(&self) -> String {
        self.shared.window()
    }

    pub fn set_visible(&mut self, show: bool) {
        self.visible = show;
        if !self.container_hwnd.is_invalid() {
            unsafe {
                let _ = ShowWindow(self.container_hwnd, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
        }
    }

    pub fn destroy_container(&self) {
        if !self.container_hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(self.container_hwnd);
            }
        }
    }
}

pub struct CreateOptions {
    pub id: String,
    pub window: String,
    pub url: Option<String>,
    pub glance: bool,
    pub muted: bool,
    pub zoom: Option<f64>,
    pub scroll: Option<(f64, f64)>,
    pub opener_url: Option<String>,
}

/// Creates a CEF Chrome-style child page browser inside a clipping container HWND.
pub fn create(opts: CreateOptions) -> Result<(), String> {
    let label = opts.window.clone();
    let snapshot = with_host(|h| {
        let win = h.wins.get(&label)?;
        let scale = h.scale(&label);
        Some((
            win.hwnd,
            scale,
            h.page_bg,
            h.pane_for(&label, &opts.id),
            h.tabs.get(&opts.id).and_then(|t| t.browser.clone()),
            h.tabs.contains_key(&opts.id),
        ))
    })
    .flatten();
    let (parent_hwnd, scale, bg, pane, existing_browser, exists) = snapshot.ok_or("browser is not ready")?;
    if exists {
        if let Some(url) = &opts.url {
            if let Some(b) = existing_browser {
                if let Some(frame) = b.main_frame() {
                    frame.load_url(Some(&CefString::from(url.as_str())));
                }
            } else {
                with_host(|h| {
                    if let Some(t) = h.tabs.get_mut(&opts.id) {
                        t.pending_url = Some(url.clone());
                    }
                });
            }
        }
        return Ok(());
    }

    let private = super::is_private_window(&label);
    let light_bg = opts.url.as_deref().is_some_and(|u| u.starts_with("chrome-extension://"));
    if let Some(claimed) = warm::try_claim(&opts, parent_hwnd, light_bg) {
        let id = opts.id.clone();
        with_host(move |h| {
            h.tabs.insert(id, claimed);
            h.apply_layout(&label);
        });
        return Ok(());
    }

    super::set_tab_window(&opts.id, &label);
    let shared = Arc::new(TabShared::new(&opts.id, &label, opts.glance, light_bg));
    if let Ok(mut s) = shared.restore_scroll.lock() {
        *s = opts.scroll;
    }

    let (x, y, w, h) = pane
        .as_ref()
        .map(|p| super::placement::px_rect(p.x, p.y, p.w, p.h, scale))
        .unwrap_or((0, 0, 800, 600));
    let container_hwnd = create_container(parent_hwnd, x, y, w, h)?;
    shared.container_hwnd.store(container_hwnd.0 as isize, Ordering::SeqCst);

    let mut win_info = WindowInfo::default().set_as_child(
        cef::sys::HWND(container_hwnd.0.cast()),
        &Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) },
    );
    // shortcut: Alloy style used for child page tabs in Phase 3 because Chrome style crashes
    // inside libcef.dll when hosted alongside an Alloy chrome child under bootstrap.exe;
    // revisit Chrome style for full chrome.tabs extension APIs in Phase 4.
    win_info.runtime_style = RuntimeStyle::ALLOY;

    let mut extra = dictionary_value_create().ok_or("dictionary_value_create failed")?;
    extra.set_string(Some(&CefString::from("zenith_role")), Some(&CefString::from("page")));
    extra.set_string(
        Some(&CefString::from("zenith_private")),
        Some(&CefString::from(if private { "1" } else { "0" })),
    );
    extra.set_string(
        Some(&CefString::from("zenith_media_button")),
        Some(&CefString::from(if config().media_button { "1" } else { "0" })),
    );
    let trig = super::config::triggers_json_for(&config());
    extra.set_string(Some(&CefString::from("zenith_triggers")), Some(&CefString::from(trig.as_str())));
    if light_bg && opts.glance {
        extra.set_string(Some(&CefString::from("zenith_ext_popup")), Some(&CefString::from("1")));
        if let Some(opener) = &opts.opener_url {
            extra.set_string(
                Some(&CefString::from("zenith_ext_opener")),
                Some(&CefString::from(opener.as_str())),
            );
        }
    }

    let mut settings = BrowserSettings::default();
    settings.background_color = if light_bg {
        0xFFFFFFFF
    } else {
        0xFF000000 | ((bg.0 as u32) << 16) | ((bg.1 as u32) << 8) | (bg.2 as u32)
    };

    let mut ctx = if private { incognito_context() } else { None };
    let initial_url = opts.url.clone().unwrap_or_else(|| "about:blank".into());
    let mut client = events::create_tab_client(shared.clone());

    let tab = Tab {
        container_hwnd,
        browser: None,
        browser_hwnd: HWND::default(),
        devtools: None,
        shared,
        visible: false,
        last_rect: None,
        content_px: None,
        rgn_px: None,
        pending_url: None,
        pending_muted: opts.muted,
        pending_zoom: opts.zoom,
        find_text: String::new(),
        find_case: false,
    };
    let id = opts.id.clone();
    with_host(move |h| {
        h.tabs.insert(id, tab);
        h.apply_layout(&label);
    });

    if browser_host_create_browser(
        Some(&win_info),
        Some(&mut client),
        Some(&CefString::from(initial_url.as_str())),
        Some(&settings),
        Some(&mut extra),
        ctx.as_mut(),
    ) == 0
    {
        with_host(|h| {
            if let Some(t) = h.tabs.remove(&opts.id) {
                t.destroy_container();
            }
        });
        return Err("browser_host_create_browser failed for tab".into());
    }
    Ok(())
}

pub(crate) fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub fn zoom_factor_to_level(factor: f64) -> f64 {
    factor.clamp(0.25, 5.0).ln() / 1.2_f64.ln()
}

pub fn zoom_level_to_factor(level: f64) -> f64 {
    (1.2_f64.powf(level) * 100.0).round() / 100.0
}

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
        h.cert_errors.retain(|_, p| p.tab != id);
        h.protocol_prompts.retain(|_, p| p.tab != id);
        h.pending_downloads.retain(|_, p| p.tab != id);
        if let Ok(mut sb) = crate::security::SAFE_BROWSING.write() {
            sb.drop_tab(id);
        }
        let mut tab = h.tabs.remove(id)?;
        tab.shared.closed.store(true, Ordering::SeqCst);
        tab.set_visible(false);
        Some(tab)
    })
    .flatten();

    if let Some(mut tab) = removed {
        if let Some(mut dt) = tab.devtools.take() {
            dt.shared.closed.store(true, Ordering::SeqCst);
            dt.set_visible(false);
            let closed_via_host = tab
                .browser
                .as_ref()
                .and_then(|b| b.host())
                .is_some_and(|h| {
                    if h.has_dev_tools() != 0 {
                        h.close_dev_tools();
                        true
                    } else {
                        false
                    }
                });
            if !closed_via_host {
                if let Some(b) = dt.browser {
                    if let Some(h) = b.host() {
                        h.close_browser(1);
                    }
                } else {
                    dt.destroy_container();
                }
            }
            emit_devtools_changed(&label, id, false, false);
        }
        if let Some(b) = tab.browser {
            if let Some(host) = b.host() {
                // Defer destroying `container_hwnd` until `on_before_close` so Chromium
                // finishes tearing down its child HWND first.
                host.close_browser(1);
            }
        } else {
            let raw = tab.shared.container_hwnd.swap(0, Ordering::SeqCst);
            if raw != 0 {
                unsafe {
                    let _ = DestroyWindow(HWND(raw as *mut _));
                }
            }
            maybe_drop_incognito_context();
        }
    }

    let window = with_win(&label, |w| w.window.clone());
    if let Some(w) = window {
        if w.is_fullscreen() {
            let _ = w.set_fullscreen(false);
        }
    }
}

pub fn navigate(id: &str, url: &str) -> Result<(), String> {
    let browser = with_host(|h| {
        let t = h.tabs.get_mut(id)?;
        match &t.browser {
            Some(b) => Some(Some(b.clone())),
            None => {
                t.pending_url = Some(url.to_string());
                Some(None)
            }
        }
    })
    .flatten()
    .ok_or("tab has no browser")?;
    if let Some(b) = browser {
        if let Some(frame) = b.main_frame() {
            frame.load_url(Some(&CefString::from(url)));
        }
    }
    Ok(())
}

pub fn set_glance(id: &str, glance: bool) {
    with_host(|h| {
        if let Some(t) = h.tabs.get(id) {
            t.shared.glance.store(glance, Ordering::Relaxed);
        }
    });
}

pub fn command(id: &str, cmd: &str, arg: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    if cmd == "devtools" {
        return handle_devtools_command(id, arg.as_ref());
    }
    let browser = with_host(|h| h.tabs.get(id).and_then(|t| t.browser.clone()))
        .flatten()
        .ok_or("tab has no browser")?;
    let host = browser.host().ok_or("browser host unavailable")?;
    let num = |default: f64| arg.as_ref().and_then(|v| v.as_f64()).unwrap_or(default);
    match cmd {
        "back" => browser.go_back(),
        "forward" => browser.go_forward(),
        "reload" => browser.reload(),
        "hardReload" => browser.reload_ignore_cache(),
        "stop" => browser.stop_load(),
        "print" => host.print(),
        "saveAs" => {
            if let Some(frame) = browser.main_frame() {
                let url = super::cef_app::cef_str(frame.url());
                if is_web(&url) {
                    host.start_download(Some(&CefString::from(url.as_str())));
                }
            }
        }
        "mute" | "unmute" => {
            let muted = cmd == "mute";
            host.set_audio_muted(if muted { 1 } else { 0 });
            TabPatch { muted: Some(muted), ..TabPatch::new(id) }.send();
        }
        "zoom" => {
            let z = num(1.0).clamp(0.25, 5.0);
            host.set_zoom_level(zoom_factor_to_level(z));
            TabPatch { zoom: Some(z), ..TabPatch::new(id) }.send();
            return Ok(serde_json::json!(z));
        }
        "suspend" | "resume" => {}
        "pinMeta" => {
            let meta = arg.as_ref().and_then(|v| {
                let site = v.get("site")?.as_str()?.trim().to_ascii_lowercase();
                if site.is_empty() { return None; }
                let essential = v.get("essential").and_then(|b| b.as_bool()).unwrap_or(false);
                Some((site, essential))
            });
            with_host(|h| {
                if let Some(t) = h.tabs.get(id) {
                    if let Ok(mut slot) = t.shared.pin_site.write() {
                        *slot = meta;
                    }
                }
            });
        }
        "viewSource" => {
            let src = browser
                .main_frame()
                .map(|f| super::cef_app::cef_str(f.url()))
                .unwrap_or_default();
            return Ok(serde_json::json!(format!("view-source:{src}")));
        }
        other => return Err(format!("unknown tab command {other}")),
    }
    Ok(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_level_round_trips() {
        for z in [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0] {
            let got = zoom_level_to_factor(zoom_factor_to_level(z));
            assert!((got - z).abs() < 0.01, "expected {z}, got {got}");
        }
    }
}
