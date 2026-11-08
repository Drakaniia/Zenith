//! Page webviews: creation, WebView2 event wiring and per-tab commands.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use base64::Engine as _;
use serde::Serialize;
use std::result::Result;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use windows::Win32::System::Com::IStream;
use wry::{MemoryUsageLevel, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

use super::{app, config, db, emit_tab, post, with_host, with_win, Focus, BLOCKER};
use crate::urls::{host_of, site_of};

/// Injected into every frame of every page. It cannot reach the app (tabs have no IPC bridge):
/// it only rewrites Alt+click into a Glance request and Ctrl/middle-click into a background-tab
/// request, using named `window.open` calls that NewWindowRequested recognises and cancels.
const PAGE_SCRIPT: &str = r#"(() => {
  const open = window.open.bind(window);
  const link = (e) => {
    for (const n of e.composedPath()) {
      if (n instanceof HTMLAnchorElement && n.href) return n;
    }
    return null;
  };
  const route = (e, target) => {
    const a = link(e);
    if (!a || a.hasAttribute('download') || !/^https?:/i.test(a.href)) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    open(a.href, target);
  };
  addEventListener('click', (e) => {
    if (e.defaultPrevented || e.button !== 0 || !e.isTrusted) return;
    if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) route(e, '_zenith_glance');
    else if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey) route(e, '_zenith_bg');
  });
  addEventListener('auxclick', (e) => {
    if (e.defaultPrevented || e.button !== 1 || !e.isTrusted) return;
    route(e, '_zenith_bg');
  });
})();"#;

/// Chrome Web Store pages (main frame only, normal windows): the store's own "Add to Chrome"
/// button installs into Zenith. It keeps its label and is taken over when it is there; otherwise
/// a floating "Add to Chrome" button appears. A real click calls `window.open` with the reserved `_zenith_ext`
/// name, which NewWindowRequested accepts only from the store and turns into an install prompt.
/// Built with DOM calls only and CSSOM styles (the store uses Trusted Types and a strict CSP).
const STORE_SCRIPT: &str = r#"(() => {
  if (location.hostname !== 'chromewebstore.google.com') return;
  const open = window.open.bind(window);
  const idOf = () => {
    const m = location.pathname.match(/\/detail\/(?:[^/]+\/)?([a-p]{32})(?:[/?#]|$)/);
    return m ? m[1] : null;
  };
  const install = (e) => {
    e.preventDefault();
    e.stopImmediatePropagation();
    if (!e.isTrusted) return;
    const id = idOf();
    if (id) open('https://chromewebstore.google.com/detail/' + id, '_zenith_ext');
  };
  let host = null;
  const pill = () => {
    if (host) return host;
    host = document.createElement('div');
    Object.assign(host.style, { position: 'fixed', right: '24px', bottom: '24px', zIndex: '2147483647', display: 'none' });
    const root = host.attachShadow({ mode: 'closed' });
    const b = document.createElement('button');
    b.textContent = 'Add to Chrome';
    Object.assign(b.style, {
      font: '600 14px/1 "Segoe UI Variable", "Segoe UI", system-ui, sans-serif', padding: '12px 20px',
      borderRadius: '999px', border: '0', color: '#fff', background: '#7c6cf0', cursor: 'pointer',
      boxShadow: '0 6px 24px rgba(40, 20, 120, .35)',
    });
    b.addEventListener('click', install);
    root.appendChild(b);
    return host;
  };
  const nativeButtons = () => [...document.querySelectorAll('button')].filter((b) => b.dataset.zenith ||
    /^(add to (chrome|edge)|get)$/i.test((b.textContent || '').trim()));
  const tick = () => {
    const id = idOf();
    let taken = false;
    if (id) {
      for (const b of nativeButtons()) {
        if (!b.dataset.zenith) {
          b.dataset.zenith = '1';
          b.addEventListener('click', install, true);
        }
        b.disabled = false;
        b.removeAttribute('aria-disabled');
        if (b.offsetParent) taken = true;
      }
    }
    const p = pill();
    if (!p.isConnected && document.documentElement) document.documentElement.appendChild(p);
    p.style.display = id && !taken ? 'block' : 'none';
  };
  setInterval(tick, 800);
  addEventListener('DOMContentLoaded', tick);
})();"#;

static NEXT_POPUP: AtomicU32 = AtomicU32::new(1);
static NEXT_PERMISSION: AtomicU32 = AtomicU32::new(1);
pub static TOTAL_BLOCKED: AtomicU64 = AtomicU64::new(0);

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
struct TabCreated {
    id: String,
    opener_id: String,
    url: String,
    popup: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PermissionEvent {
    request_id: u32,
    tab_id: String,
    origin: String,
    kind: String,
    user_initiated: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FindResult {
    id: String,
    matches: i32,
    active: i32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TabRef {
    id: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PopupBlocked {
    tab_id: String,
    url: String,
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

pub struct PendingPermission {
    pub args: ICoreWebView2PermissionRequestedEventArgs,
    pub deferral: ICoreWebView2Deferral,
    pub tab: String,
    pub answered: bool,
}

impl PendingPermission {
    pub fn answer(mut self, allow: bool, remember: bool) {
        unsafe {
            let _ = self.args.SetState(if allow {
                COREWEBVIEW2_PERMISSION_STATE_ALLOW
            } else {
                COREWEBVIEW2_PERMISSION_STATE_DENY
            });
            if let Ok(a3) = self.args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
                let _ = a3.SetSavesInProfile(remember);
            }
            let _ = self.deferral.Complete();
        }
        self.answered = true;
    }
}

impl Drop for PendingPermission {
    fn drop(&mut self) {
        if !self.answered {
            unsafe {
                let _ = self.args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY);
                let _ = self.deferral.Complete();
            }
        }
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

/// Runs in an extension's toolbar popup. In Chrome a popup is not a tab, so
/// `tabs.query({ active: true, currentWindow: true })` finds the page under it. Every WebView2
/// view is its own browser window, so that query would return the popup itself ("not a
/// website"). Window-scoped queries are answered with the page the popup was opened over.
const EXT_POPUP_SCRIPT: &str = r#"(() => {
  const OPENER = __OPENER__;
  const tabs = globalThis.chrome && chrome.tabs;
  if (!tabs || typeof tabs.query !== 'function' || window.top !== window) return;
  const query = tabs.query.bind(tabs);
  const getCurrent = typeof tabs.getCurrent === 'function' ? tabs.getCurrent.bind(tabs) : null;
  const call = (fn, ...a) => new Promise((res) => {
    try { fn(...a, (r) => { void chrome.runtime.lastError; res(r); }); } catch { res(undefined); }
  });
  const isExt = (t) => typeof t.url === 'string' && /^chrome-extension:/.test(t.url);
  const origin = (u) => { try { return new URL(u).origin; } catch { return ''; } };
  // The opener among `list`: same address, then same site, then the most recently used page.
  const pick = async (list) => {
    const self = getCurrent ? await call(getCurrent) : undefined;
    const pages = (list || []).filter((t) => (!self || t.id !== self.id) && !isExt(t));
    const recent = (a, b) => (b.lastAccessed || 0) - (a.lastAccessed || 0);
    return (
      pages.find((t) => OPENER && t.url === OPENER) ||
      pages.find((t) => OPENER && t.url && origin(t.url) === origin(OPENER)) ||
      pages.sort(recent)[0]
    );
  };
  const scoped = (q) => q && (q.currentWindow || q.lastFocusedWindow);
  const answer = async (q) => {
    if (!scoped(q)) return (await call(query, q || {})) || [];
    const rest = { ...q };
    delete rest.currentWindow;
    delete rest.lastFocusedWindow;
    delete rest.windowId;
    delete rest.active;
    delete rest.highlighted;
    const found = (await call(query, rest)) || [];
    if (q.active || q.highlighted) {
      const t = await pick(found);
      return t ? [t] : [];
    }
    return found.filter((t) => !isExt(t));
  };
  const wrapped = function (q, cb) {
    const p = answer(q);
    if (typeof cb === 'function') { p.then(cb, () => cb([])); return; }
    return p;
  };
  const wrappedCurrent = function (cb) {
    // A popup is not a tab.
    if (typeof cb === 'function') { setTimeout(() => cb(undefined)); return; }
    return Promise.resolve(undefined);
  };
  const set = (obj, key, fn) => {
    try { obj[key] = fn; } catch {}
    if (obj[key] !== fn) { try { Object.defineProperty(obj, key, { value: fn, configurable: true, writable: true }); } catch {} }
  };
  set(tabs, 'query', wrapped);
  if (getCurrent) set(tabs, 'getCurrent', wrappedCurrent);
  const windows = chrome.windows;
  if (windows && typeof windows.get === 'function') {
    const get = windows.get.bind(windows);
    const current = async (opts) => {
      const t = await pick((await call(query, {})) || []);
      if (t) return await call(get, t.windowId, opts || {});
      return undefined;
    };
    for (const key of ['getCurrent', 'getLastFocused']) {
      if (typeof windows[key] !== 'function') continue;
      const orig = windows[key].bind(windows);
      set(windows, key, function (a, b) {
        const cb = typeof a === 'function' ? a : b;
        const opts = typeof a === 'function' ? undefined : a;
        const p = current(opts).then((w) => w || call(orig, opts || {}));
        if (typeof cb === 'function') { p.then(cb); return; }
        return p;
      });
    }
  }
})();"#;

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

fn permission_name(kind: COREWEBVIEW2_PERMISSION_KIND) -> Option<&'static str> {
    Some(match kind {
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => "microphone",
        COREWEBVIEW2_PERMISSION_KIND_CAMERA => "camera",
        COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION => "location",
        COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS => "notifications",
        COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ => "clipboard",
        COREWEBVIEW2_PERMISSION_KIND_MIDI_SYSTEM_EXCLUSIVE_MESSAGES => "midi",
        COREWEBVIEW2_PERMISSION_KIND_WINDOW_MANAGEMENT => "windowManagement",
        COREWEBVIEW2_PERMISSION_KIND_LOCAL_FONTS => "localFonts",
        COREWEBVIEW2_PERMISSION_KIND_FILE_READ_WRITE => "files",
        COREWEBVIEW2_PERMISSION_KIND_MULTIPLE_AUTOMATIC_DOWNLOADS => "downloads",
        COREWEBVIEW2_PERMISSION_KIND_OTHER_SENSORS => "sensors",
        _ => return None,
    })
}

fn origin_of(uri: &str) -> String {
    match tauri::Url::parse(uri) {
        Ok(u) => u.origin().ascii_serialization(),
        Err(_) => uri.to_string(),
    }
}

fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Wires every WebView2 event Zenith cares about. Handlers capture `TabShared` instead of the
/// host so they keep working while the host is borrowed elsewhere.
fn wire(view: &wry::WebView, core: &ICoreWebView2, env: &ICoreWebView2Environment, s: Rc<TabShared>) {
    let controller = view.controller();
    super::attach_accelerators(&controller, Some(s.clone()), &s.window);
    let mut token = 0i64;
    unsafe {
        // Focus tracking (to restore the page focus after Alt+Tab).
        let sh = s.clone();
        let _ = controller.add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                let id = sh.id.clone();
                // May fire synchronously inside a layout pass; record it afterwards.
                let focused = id.clone();
                let window = sh.window.clone();
                post(move || {
                    with_win(&window, |w| w.focus = Focus::Tab(focused));
                });
                emit_tab(&id.clone(), "tab-focused", TabRef { id });
                Ok(())
            })),
            &mut token,
        );

        // Zoom changes made inside WebView2 (Ctrl+wheel, pinch). Zoom set by Zenith is reported
        // by `command("zoom")` itself.
        let sh = s.clone();
        if let Err(e) = controller.add_ZoomFactorChanged(
            &ZoomFactorChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(c) = sender {
                    let mut z = 1.0;
                    c.ZoomFactor(&mut z)?;
                    TabPatch { zoom: Some(z), ..TabPatch::new(&sh.id) }.send();
                }
                Ok(())
            })),
            &mut token,
        ) {
            eprintln!("[zenith] ZoomFactorChanged unavailable: {e}");
        }

        let sh = s.clone();
        let _ = core.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = take(uri);
                *sh.page_host.borrow_mut() = host_of(&uri).unwrap_or("").to_ascii_lowercase();
                *sh.nav_uri.borrow_mut() = uri;
                sh.blocked.set(0);
                sh.requests.set(0);
                sh.media.borrow_mut().reset();
                crate::media::sniff::notify(&sh);
                super::autofill::on_navigation(&sh.id);
                TabPatch {
                    loading: Some(true),
                    blocked: Some(0),
                    crashed: Some(String::new()),
                    error: Some(String::new()),
                    ..TabPatch::new(&sh.id)
                }
                .send();
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(core) = sender else { return Ok(()) };
                let mut uri = PWSTR::null();
                core.Source(&mut uri)?;
                let uri = take(uri);
                *sh.url.borrow_mut() = uri.clone();
                TabPatch { url: Some(uri), ..TabPatch::new(&sh.id) }.send();
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(core) = sender else { return Ok(()) };
                let (mut back, mut fwd) = (BOOL::default(), BOOL::default());
                core.CanGoBack(&mut back)?;
                core.CanGoForward(&mut fwd)?;
                TabPatch {
                    can_go_back: Some(back.as_bool()),
                    can_go_forward: Some(fwd.as_bool()),
                    ..TabPatch::new(&sh.id)
                }
                .send();
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                let (Some(core), Some(args)) = (sender, args) else { return Ok(()) };
                let mut ok = BOOL::default();
                args.IsSuccess(&mut ok)?;
                let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                let _ = args.WebErrorStatus(&mut status);
                let mut title = PWSTR::null();
                core.DocumentTitle(&mut title)?;
                let title = take(title);
                let url = sh.url.borrow().clone();
                let error = if ok.as_bool() || status == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
                    String::new()
                } else {
                    format!("{}", status.0)
                };
                TabPatch {
                    loading: Some(false),
                    blocked: Some(sh.blocked.get()),
                    title: (!title.is_empty()).then(|| title.clone()),
                    error: Some(error),
                    ..TabPatch::new(&sh.id)
                }
                .send();
                if ok.as_bool() && is_web(&url) && config().record_history && !sh.private {
                    db().exec(move |c| {
                        let _ = crate::store::history_add_visit(c, &url, &title);
                    });
                }
                if ok.as_bool() {
                    if let Some((x, y)) = sh.restore_scroll.take() {
                        let js = format!("window.scrollTo({x}, {y})");
                        let _ = core.ExecuteScript(
                            &HSTRING::from(js),
                            &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(()))),
                        );
                    }
                }
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_DocumentTitleChanged(
            &DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(core) = sender else { return Ok(()) };
                let mut title = PWSTR::null();
                core.DocumentTitle(&mut title)?;
                let title = take(title);
                *sh.title.borrow_mut() = title.clone();
                let url = sh.url.borrow().clone();
                if is_web(&url) && !title.is_empty() && config().record_history && !sh.private {
                    let t = title.clone();
                    db().exec(move |c| {
                        let _ = crate::store::history_set_title(c, &url, &t);
                    });
                }
                TabPatch { title: Some(title), ..TabPatch::new(&sh.id) }.send();
                Ok(())
            })),
            &mut token,
        );

        if let Ok(c15) = core.cast::<ICoreWebView2_15>() {
            let sh = s.clone();
            let _ = c15.add_FaviconChanged(
                &FaviconChangedEventHandler::create(Box::new(move |sender, _| {
                    let Some(core) = sender else { return Ok(()) };
                    let c15: ICoreWebView2_15 = core.cast()?;
                    let mut uri = PWSTR::null();
                    c15.FaviconUri(&mut uri)?;
                    if take(uri).is_empty() {
                        TabPatch { favicon: Some(String::new()), ..TabPatch::new(&sh.id) }.send();
                        return Ok(());
                    }
                    let id = sh.id.clone();
                    c15.GetFavicon(
                        COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG,
                        &GetFaviconCompletedHandler::create(Box::new(move |result, stream| {
                            if result.is_ok() {
                                if let Some(bytes) = stream.as_ref().and_then(read_stream) {
                                    if !bytes.is_empty() {
                                        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                                        TabPatch {
                                            favicon: Some(format!("data:image/png;base64,{data}")),
                                            ..TabPatch::new(&id)
                                        }
                                        .send();
                                    }
                                }
                            }
                            Ok(())
                        })),
                    )?;
                    Ok(())
                })),
                &mut token,
            );
        }

        if let Ok(c8) = core.cast::<ICoreWebView2_8>() {
            let sh = s.clone();
            let _ = c8.add_IsDocumentPlayingAudioChanged(
                &IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |sender, _| {
                    let Some(core) = sender else { return Ok(()) };
                    let c8: ICoreWebView2_8 = core.cast()?;
                    let mut playing = BOOL::default();
                    c8.IsDocumentPlayingAudio(&mut playing)?;
                    TabPatch { audible: Some(playing.as_bool()), ..TabPatch::new(&sh.id) }.send();
                    Ok(())
                })),
                &mut token,
            );
            let sh = s.clone();
            let _ = c8.add_IsMutedChanged(
                &IsMutedChangedEventHandler::create(Box::new(move |sender, _| {
                    let Some(core) = sender else { return Ok(()) };
                    let c8: ICoreWebView2_8 = core.cast()?;
                    let mut muted = BOOL::default();
                    c8.IsMuted(&mut muted)?;
                    TabPatch { muted: Some(muted.as_bool()), ..TabPatch::new(&sh.id) }.send();
                    Ok(())
                })),
                &mut token,
            );
        }

        let sh = s.clone();
        let _ = core.add_ContainsFullScreenElementChanged(
            &ContainsFullScreenElementChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(core) = sender else { return Ok(()) };
                let mut full = BOOL::default();
                core.ContainsFullScreenElement(&mut full)?;
                let id = sh.id.clone();
                let on = full.as_bool();
                post(move || super::set_fullscreen_tab(&id, on));
                TabPatch { fullscreen: Some(on), ..TabPatch::new(&sh.id) }.send();
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = take(uri);
                let mut user = BOOL::default();
                args.IsUserInitiated(&mut user)?;
                let name = args
                    .cast::<ICoreWebView2NewWindowRequestedEventArgs2>()
                    .ok()
                    .and_then(|a| {
                        let mut p = PWSTR::null();
                        a.Name(&mut p).ok()?;
                        Some(take(p))
                    })
                    .unwrap_or_default();
                let popup = args
                    .WindowFeatures()
                    .ok()
                    .map(|f| {
                        let mut has = BOOL::default();
                        let _ = f.HasSize(&mut has);
                        has.as_bool()
                    })
                    .unwrap_or(false);
                args.SetHandled(true)?;
                let opener = sh.id.clone();
                match name.as_str() {
                    // Login forms (see host::autofill).
                    "_zenith_pw_field" | "_zenith_pw_type" | "_zenith_pw_dismiss" | "_zenith_pw_capture" | "_zenith_pw_form" => {
                        let main_frame = super::autofill::from_main_frame(&args);
                        super::autofill::on_message(&sh, &name, &uri, user.as_bool(), main_frame);
                    }
                    // The media downloader's button over a video (see media::detect).
                    "_zenith_media" => {
                        if user.as_bool() {
                            crate::media::sniff::request_panel(&opener, &uri);
                        }
                    }
                    // "Add to Zenith" on a Chrome Web Store page (STORE_SCRIPT).
                    "_zenith_ext" => {
                        let on_store = sh.page_host.borrow().as_str() == "chromewebstore.google.com";
                        if let (true, true, Some(id)) = (user.as_bool(), on_store, super::crx::store_id(&uri)) {
                            let source = "Chrome Web Store".to_string();
                            if sh.private {
                                super::extensions::refuse(&opener, source, "Extensions can't be installed from an incognito window.");
                            } else {
                                super::extensions::offer(opener, source, super::extensions::Fetch::Store(id));
                            }
                        }
                    }
                    "_zenith_glance" | "_zenith_bg" => {
                        if user.as_bool() {
                            let kind = if name == "_zenith_glance" { "glance" } else { "background" };
                            emit_tab(
                                &opener.clone(),
                                "tab-open-request",
                                OpenRequest { url: uri, opener_id: Some(opener), kind: kind.into() },
                            );
                        }
                    }
                    _ => {
                        if !user.as_bool() && config().block_popups {
                            emit_tab(&opener.clone(), "popup-blocked", PopupBlocked { tab_id: opener, url: uri });
                            return Ok(());
                        }
                        // Keep the opener relationship (OAuth, payments): hand WebView2 a fresh
                        // tab webview to load the popup into.
                        let deferral = args.GetDeferral()?;
                        let id = format!("w{}", NEXT_POPUP.fetch_add(1, Ordering::Relaxed));
                        let window = sh.window.clone();
                        post(move || {
                            match create(CreateOptions {
                                id: id.clone(),
                                window,
                                url: None,
                                glance: false,
                                muted: false,
                                zoom: None,
                                scroll: None,
                                opener_url: None,
                            }) {
                                Ok(core) => {
                                    let _ = args.SetNewWindow(&core);
                                    let _ = deferral.Complete();
                                    emit_tab(&opener.clone(), "tab-created", TabCreated { id, opener_id: opener, url: uri, popup });
                                }
                                Err(e) => {
                                    eprintln!("[zenith] popup webview failed: {e}");
                                    let _ = deferral.Complete();
                                }
                            }
                        });
                    }
                }
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                emit_tab(&sh.id, "tab-close-request", TabRef { id: sh.id.clone() });
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                args.ProcessFailedKind(&mut kind)?;
                let state = match kind {
                    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => "crashed",
                    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => "unresponsive",
                    _ => return Ok(()),
                };
                TabPatch { crashed: Some(state.into()), loading: Some(false), ..TabPatch::new(&sh.id) }.send();
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut kind)?;
                let Some(name) = permission_name(kind) else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = take(uri);
                let mut user = BOOL::default();
                args.IsUserInitiated(&mut user)?;
                let deferral = args.GetDeferral()?;
                let request_id = NEXT_PERMISSION.fetch_add(1, Ordering::Relaxed);
                let tab = sh.id.clone();
                let pending = PendingPermission { args, deferral, tab: tab.clone(), answered: false };
                let stored = with_host(move |h| {
                    h.permissions.insert(request_id, pending);
                });
                if stored.is_some() {
                    emit_tab(
                        &sh.id,
                        "permission-request",
                        PermissionEvent {
                            request_id,
                            tab_id: tab,
                            origin: origin_of(&uri),
                            kind: name.into(),
                            user_initiated: user.as_bool(),
                        },
                    );
                }
                Ok(())
            })),
            &mut token,
        );

        attach_blocker(core, env, s.clone());
        attach_context_menu(core, env, s.clone());
    }
}

unsafe fn attach_blocker(core: &ICoreWebView2, env: &ICoreWebView2Environment, s: Rc<TabShared>) {
    unsafe {
        let filter = HSTRING::from("*");
        match core.cast::<ICoreWebView2_22>() {
            Ok(c22) => {
                let _ = c22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                    &filter,
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                );
            }
            Err(_) => {
                let _ = core.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL);
            }
        }
        let env = env.clone();
        let mut token = 0i64;
        let _ = core.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&mut uri)?;
                let uri = take(uri);
                if !is_web(&uri) {
                    return Ok(());
                }
                s.requests.set(s.requests.get().saturating_add(1));
                let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
                args.ResourceContext(&mut context)?;
                if context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA {
                    // Lets the media downloader tell <video>/<audio> files from stream segments.
                    s.media.borrow_mut().note_media_request(&uri);
                }
                if !super::blocker_enabled() {
                    return Ok(());
                }
                if context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT && *s.nav_uri.borrow() == uri {
                    return Ok(()); // the top-level navigation itself
                }
                let Some(request_host) = host_of(&uri) else { return Ok(()) };
                let blocked = {
                    let page = s.page_host.borrow();
                    match BLOCKER.read() {
                        Ok(b) => {
                            !b.disabled_sites.contains(site_of(&page))
                                && b.engine.should_block(request_host, (!page.is_empty()).then_some(page.as_str()))
                        }
                        Err(_) => false,
                    }
                };
                if blocked {
                    let response = env.CreateWebResourceResponse(
                        None::<&IStream>,
                        403,
                        &HSTRING::from("Blocked by Zenith"),
                        &HSTRING::from(""),
                    )?;
                    args.SetResponse(&response)?;
                    s.blocked.set(s.blocked.get() + 1);
                    TOTAL_BLOCKED.fetch_add(1, Ordering::Relaxed);
                    schedule_block_flush(&s);
                }
                Ok(())
            })),
            &mut token,
        );
    }
}

/// Coalesces blocked-request counts into at most ~3 UI updates per second per tab.
fn schedule_block_flush(s: &Rc<TabShared>) {
    if s.flush_pending.replace(true) {
        return;
    }
    let id = s.id.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(350));
        let _ = app().run_on_main_thread(move || {
            with_host(|h| {
                if let Some(t) = h.tabs.get(&id) {
                    t.shared.flush_pending.set(false);
                    TabPatch { blocked: Some(t.shared.blocked.get()), ..TabPatch::new(&id) }.send();
                }
            });
        });
    });
}

unsafe fn attach_context_menu(core: &ICoreWebView2, env: &ICoreWebView2Environment, s: Rc<TabShared>) {
    unsafe {
        let (Ok(c11), Ok(env9)) = (core.cast::<ICoreWebView2_11>(), env.cast::<ICoreWebView2Environment9>()) else {
            return;
        };
        let mut token = 0i64;
        let _ = c11.add_ContextMenuRequested(
            &ContextMenuRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let target = args.ContextMenuTarget()?;
                let items = args.MenuItems()?;
                // Videos and audio: open the media downloader for this element (also works for
                // streaming players whose source is a blob: address).
                let mut kind = COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND::default();
                target.Kind(&mut kind)?;
                if kind == COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_VIDEO || kind == COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_AUDIO {
                    let mut has_source = BOOL::default();
                    target.HasSourceUri(&mut has_source)?;
                    let source = if has_source.as_bool() {
                        let mut p = PWSTR::null();
                        target.SourceUri(&mut p)?;
                        take(p)
                    } else {
                        String::new()
                    };
                    let label = if kind == COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_VIDEO {
                        "Download video with Zenith…"
                    } else {
                        "Download audio with Zenith…"
                    };
                    let item = env9.CreateContextMenuItem(
                        &HSTRING::from(label),
                        None::<&IStream>,
                        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
                    )?;
                    let tab = s.id.clone();
                    let mut t = 0i64;
                    item.add_CustomItemSelected(
                        &CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
                            crate::media::sniff::request_panel(&tab, &source);
                            Ok(())
                        })),
                        &mut t,
                    )?;
                    let separator = env9.CreateContextMenuItem(
                        &HSTRING::from(""),
                        None::<&IStream>,
                        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
                    )?;
                    items.InsertValueAtIndex(0, &item)?;
                    items.InsertValueAtIndex(1, &separator)?;
                }
                let mut has_link = BOOL::default();
                target.HasLinkUri(&mut has_link)?;
                if !has_link.as_bool() {
                    return Ok(());
                }
                let mut link = PWSTR::null();
                target.LinkUri(&mut link)?;
                let link = take(link);
                if !is_web(&link) {
                    return Ok(());
                }
                let mut count = 0u32;
                items.Count(&mut count)?;
                let mut index = 0u32;
                for i in 0..count {
                    let item = items.GetValueAtIndex(i)?;
                    let mut name = PWSTR::null();
                    item.Name(&mut name)?;
                    if take(name) == "openLinkInNewWindow" {
                        index = i + 1;
                        break;
                    }
                }
                for (label, kind) in [
                    ("Open link in new tab", "background"),
                    ("Open link in Glance", "glance"),
                    ("Open link in split view", "split"),
                    ("Open link in incognito window", "incognito"),
                ] {
                    let item = env9.CreateContextMenuItem(
                        &HSTRING::from(label),
                        None::<&IStream>,
                        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
                    )?;
                    let url = link.clone();
                    let opener = s.id.clone();
                    let mut t = 0i64;
                    item.add_CustomItemSelected(
                        &CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
                            if kind == "incognito" {
                                crate::incognito::open_window(vec![url.clone()]);
                                return Ok(());
                            }
                            emit_tab(
                                &opener,
                                "tab-open-request",
                                OpenRequest { url: url.clone(), opener_id: Some(opener.clone()), kind: kind.into() },
                            );
                            Ok(())
                        })),
                        &mut t,
                    )?;
                    items.InsertValueAtIndex(index, &item)?;
                    index += 1;
                }
                Ok(())
            })),
            &mut token,
        );
    }
}

fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let mut read = 0u32;
        let hr = unsafe { stream.Read(buf.as_mut_ptr() as *mut _, buf.len() as u32, Some(&mut read)) };
        if hr.is_err() {
            return None;
        }
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
        if out.len() > 512 * 1024 {
            return None;
        }
    }
    Some(out)
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

type SleepDone = Box<dyn FnOnce(Option<(f64, f64)>)>;

thread_local! {
    static PENDING_SLEEPS: RefCell<std::collections::HashMap<u64, SleepDone>> = RefCell::new(Default::default());
}
static NEXT_SLEEP: AtomicU64 = AtomicU64::new(1);

/// Completes a sleep request exactly once: whichever of the script result, a script error or
/// the timeout arrives first closes the webview and answers the caller.
fn finish_sleep(token: u64, id: &str, scroll: Option<(f64, f64)>) {
    let done = PENDING_SLEEPS.with(|p| p.borrow_mut().remove(&token));
    if let Some(done) = done {
        close(id);
        done(scroll);
    }
}

/// Captures the scroll position, then frees the webview (tab sleeping). A crashed or hung
/// renderer never answers the script, so the webview is freed after 1.5 s regardless.
pub fn sleep(id: &str, done: impl FnOnce(Option<(f64, f64)>) + 'static) {
    let core = with_host(|h| h.tabs.get(id).map(|t| t.core())).flatten();
    let Some(core) = core else {
        done(None);
        return;
    };
    let token = NEXT_SLEEP.fetch_add(1, Ordering::Relaxed);
    PENDING_SLEEPS.with(|p| p.borrow_mut().insert(token, Box::new(done)));
    let id = id.to_string();
    {
        let id = id.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let _ = app().run_on_main_thread(move || finish_sleep(token, &id, None));
        });
    }
    let script_id = id.clone();
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |_, json| {
        let scroll = serde_json::from_str::<Vec<f64>>(&json)
            .ok()
            .filter(|v| v.len() == 2)
            .map(|v| (v[0], v[1]));
        post(move || finish_sleep(token, &script_id, scroll));
        Ok(())
    }));
    unsafe {
        if core.ExecuteScript(&HSTRING::from("[window.scrollX, window.scrollY]"), &handler).is_err() {
            post(move || finish_sleep(token, &id, None));
        }
    }
}

/// The natural size of a page's content (CSS px), laid out shrink-to-fit the way Chrome sizes
/// an extension's toolbar popup. None when the page cannot run the script.
pub fn content_size(id: &str, done: impl FnOnce(Option<(f64, f64)>) + 'static) {
    const SCRIPT: &str = r#"(() => {
  const d = document.documentElement, b = document.body;
  if (!b) return null;
  const saved = d.style.cssText;
  d.style.width = 'max-content';
  d.style.height = 'max-content';
  const r = d.getBoundingClientRect(), br = b.getBoundingClientRect();
  const w = Math.max(r.width, br.width, b.scrollWidth), h = Math.max(r.height, br.height, b.scrollHeight);
  d.style.cssText = saved;
  return [Math.ceil(w), Math.ceil(h)];
})()"#;
    let Some(core) = with_host(|h| h.tabs.get(id).map(|t| t.core())).flatten() else {
        return done(None);
    };
    let done = RefCell::new(Some(done));
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
        let size = result
            .ok()
            .and_then(|_| serde_json::from_str::<Vec<f64>>(&json).ok())
            .filter(|v| v.len() == 2)
            .map(|v| (v[0], v[1]));
        if let Some(done) = done.borrow_mut().take() {
            done(size);
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) = core.ExecuteScript(&HSTRING::from(SCRIPT), &handler) {
            eprintln!("[zenith] content size script failed: {e}");
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

pub fn find(id: &str, op: &str, text: &str, case_sensitive: bool, whole_word: bool) -> Result<(), String> {
    let prepared = with_host(|h| {
        let env = h.env.clone();
        let tab = h.tabs.get_mut(id)?;
        if tab.find.is_none() {
            let finder = unsafe { tab.core().cast::<ICoreWebView2_28>().and_then(|c| c.Find()) }.ok()?;
            let report = |finder: &ICoreWebView2Find, tab_id: &str| {
                let (mut matches, mut active) = (0i32, -1i32);
                unsafe {
                    let _ = finder.MatchCount(&mut matches);
                    let _ = finder.ActiveMatchIndex(&mut active);
                }
                emit_tab(tab_id, "find-result", FindResult { id: tab_id.to_string(), matches, active });
            };
            let mut token = 0i64;
            let tab_id = id.to_string();
            unsafe {
                let _ = finder.add_MatchCountChanged(
                    &FindMatchCountChangedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(f) = sender {
                            report(&f, &tab_id);
                        }
                        Ok(())
                    })),
                    &mut token,
                );
                let tab_id = id.to_string();
                let _ = finder.add_ActiveMatchIndexChanged(
                    &FindActiveMatchIndexChangedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(f) = sender {
                            report(&f, &tab_id);
                        }
                        Ok(())
                    })),
                    &mut token,
                );
            }
            tab.find = Some(finder);
        }
        Some((env, tab.find.clone()?))
    })
    .flatten();
    let (env, finder) = prepared.ok_or("find is unavailable for this tab")?;
    unsafe {
        match op {
            "start" => {
                let env15: ICoreWebView2Environment15 = env.cast().map_err(|e| e.to_string())?;
                let options = env15.CreateFindOptions().map_err(|e| e.to_string())?;
                let _ = options.SetFindTerm(&HSTRING::from(text));
                let _ = options.SetIsCaseSensitive(case_sensitive);
                let _ = options.SetShouldMatchWord(whole_word);
                let _ = options.SetSuppressDefaultFindDialog(true);
                let _ = options.SetShouldHighlightAllMatches(true);
                finder
                    .Start(&options, &FindStartCompletedHandler::create(Box::new(|_| Ok(()))))
                    .map_err(|e| e.to_string())?;
            }
            "next" => finder.FindNext().map_err(|e| e.to_string())?,
            "prev" => finder.FindPrevious().map_err(|e| e.to_string())?,
            _ => finder.Stop().map_err(|e| e.to_string())?,
        }
    }
    Ok(())
}

pub fn respond_permission(request_id: u32, allow: bool, remember: bool) {
    let pending = with_host(|h| h.permissions.remove(&request_id)).flatten();
    if let Some(p) = pending {
        p.answer(allow, remember);
    }
}
