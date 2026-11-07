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

pub mod autofill;
pub mod crx;
pub mod downloads;
pub mod extensions;
pub mod tab;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{LazyLock, OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use std::result::Result;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, HSTRING, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RMENU, VK_SHIFT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebViewExtWindows};

use crate::blocker::Engine;
use crate::keys::{BindingSpec, Bindings, Chord};
use crate::store::{Db, Paths};
use tab::{Tab, TabShared};

// ---------------------------------------------------------------------------------------------
// Process-wide services

static APP: OnceLock<AppHandle> = OnceLock::new();
static PATHS: OnceLock<Paths> = OnceLock::new();
static DB: OnceLock<Db> = OnceLock::new();

pub fn app() -> &'static AppHandle {
    APP.get().expect("app handle initialised")
}
pub fn paths() -> &'static Paths {
    PATHS.get().expect("paths initialised")
}
pub fn db() -> &'static Db {
    DB.get().expect("database initialised")
}
/// The user's profile. Incognito windows run in the same process and keep their pages'
/// data in WebView2's in-memory InPrivate profile, so there is only one profile on disk.
pub fn persistent_paths() -> &'static Paths {
    paths()
}
/// Bookmarks are shared with incognito windows, as in other browsers; history is not.
pub fn bookmarks_db() -> &'static Db {
    db()
}

/// Label of the normal browser window.
pub const MAIN: &str = "main";

/// Incognito windows are labelled `incognito-N`.
pub fn is_private_window(label: &str) -> bool {
    label.starts_with(crate::incognito::LABEL_PREFIX)
}

/// Emits an app-wide event (downloads, updates) to every window's chrome UI.
pub fn emit<S: Serialize + Clone>(event: &str, payload: S) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, payload);
    }
}

/// Emits an event to one window's chrome UI.
pub fn emit_to_window<S: Serialize + Clone>(label: &str, event: &str, payload: S) {
    if let Some(app) = APP.get() {
        let _ = app.emit_to(label, event, payload);
    }
}

/// Which window each tab id belongs to. Readable from any thread (events are emitted from
/// worker threads too). Entries are tiny and tab ids are unique, so they are never removed:
/// a sleeping tab woken later keeps its window.
static TAB_WINDOWS: LazyLock<RwLock<HashMap<String, String>>> = LazyLock::new(Default::default);

pub(crate) fn set_tab_window(id: &str, label: &str) {
    if let Ok(mut m) = TAB_WINDOWS.write() {
        m.insert(id.to_string(), label.to_string());
    }
}

/// The window a tab lives in (the main window for unknown ids).
pub fn window_of_tab(id: &str) -> String {
    TAB_WINDOWS
        .read()
        .ok()
        .and_then(|m| m.get(id).cloned())
        .unwrap_or_else(|| MAIN.to_string())
}

/// True for tabs of an incognito window.
pub fn tab_is_private(id: &str) -> bool {
    is_private_window(&window_of_tab(id))
}

/// Emits a tab's event to the window the tab lives in.
pub fn emit_tab<S: Serialize + Clone>(tab_id: &str, event: &str, payload: S) {
    emit_to_window(&window_of_tab(tab_id), event, payload);
}

/// Settings the backend needs at runtime; pushed by the UI with `apply_settings`.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct RuntimeConfig {
    pub download_dir: Option<PathBuf>,
    pub record_history: bool,
    pub block_popups: bool,
    pub blocker_enabled: bool,
    /// Show the media downloader's button over videos in pages.
    pub media_button: bool,
    /// Use FFmpeg (when found) to convert .ts recordings and merge streams.
    pub use_ffmpeg: bool,
    /// FFmpeg chosen by the user; otherwise it is looked up on PATH.
    pub ffmpeg_path: Option<PathBuf>,
    /// Check for updates in the background and install them when Zenith closes.
    pub auto_update: bool,
    /// Password manager: off means no saving, suggestions or filling.
    pub passwords_enabled: bool,
    /// Offer to save logins submitted on pages.
    pub passwords_offer_save: bool,
    /// Show saved accounts when a login field is clicked.
    pub autofill_suggest: bool,
    pub autofill_usernames: bool,
    pub autofill_passwords: bool,
    /// Fill the only saved account as soon as a login form appears (vault unlocked, exact origin).
    pub autofill_on_load: bool,
    /// Offer credentials on other subdomains of the site they were saved on.
    pub autofill_same_site: bool,
    /// Suggest a generated password in sign-up and change-password forms.
    pub password_generator: bool,
    /// Ask for the master password before every fill, reveal or copy, even when unlocked.
    pub vault_ask_every_time: bool,
    /// Lock after this many minutes without using a password (0: only when Zenith closes).
    pub vault_lock_minutes: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            download_dir: None,
            record_history: true,
            block_popups: true,
            blocker_enabled: true,
            media_button: true,
            use_ffmpeg: true,
            ffmpeg_path: None,
            auto_update: true,
            passwords_enabled: true,
            passwords_offer_save: true,
            autofill_suggest: true,
            autofill_usernames: true,
            autofill_passwords: true,
            autofill_on_load: false,
            autofill_same_site: true,
            password_generator: true,
            vault_ask_every_time: false,
            vault_lock_minutes: 15,
        }
    }
}

pub static CONFIG: LazyLock<RwLock<RuntimeConfig>> = LazyLock::new(|| RwLock::new(RuntimeConfig::default()));

pub fn config() -> RuntimeConfig {
    CONFIG.read().map(|c| c.clone()).unwrap_or_default()
}

static BLOCKER_ON: AtomicBool = AtomicBool::new(true);

/// Hot-path check used for every network request.
pub fn blocker_enabled() -> bool {
    BLOCKER_ON.load(Ordering::Relaxed)
}

pub fn apply_config(cfg: RuntimeConfig) {
    BLOCKER_ON.store(cfg.blocker_enabled, Ordering::Relaxed);
    let button = cfg.media_button;
    let changed = match CONFIG.write() {
        Ok(mut c) => {
            let changed = c.media_button != button;
            *c = cfg;
            changed
        }
        Err(_) => false,
    };
    if changed {
        if let Some(app) = APP.get() {
            let _ = app.run_on_main_thread(move || crate::media::sniff::apply_button_setting(button));
        }
    }
}

pub struct BlockerState {
    pub engine: Engine,
    pub disabled_sites: std::collections::HashSet<String>,
}

pub static BLOCKER: LazyLock<RwLock<BlockerState>> = LazyLock::new(|| {
    RwLock::new(BlockerState {
        engine: Engine::with_builtin(),
        disabled_sites: Default::default(),
    })
});

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
    /// While the chrome animates the page's left/top edge, the webview keeps the largest size it
    /// has during the animation and only moves with the edge; this is the part of it that shows
    /// (logical px from its top-left), the rest is hidden by a window region. Resizing a WebView2
    /// shows its old frame pinned to the new corner for a frame, moving it repaints nothing.
    #[serde(default)]
    pub vis_w: Option<f64>,
    #[serde(default)]
    pub vis_h: Option<f64>,
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

unsafe extern "system" fn subclass_proc(
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
                if wparam.0 as u32 != SIZE_MINIMIZED {
                    let w = (lparam.0 & 0xFFFF) as i32;
                    let h = ((lparam.0 >> 16) & 0xFFFF) as i32;
                    on_resize(hwnd, w, h);
                    // A window brought back from the taskbar or Win+D was activated while still
                    // minimized, when no webview can take the focus: it is left on the frame.
                    post(move || restore_focus(hwnd, None, true));
                }
                result
            }
            WM_ACTIVATE => {
                let state = (wparam.0 & 0xFFFF) as u32;
                let minimized = (wparam.0 >> 16) & 0xFFFF != 0;
                if state == WA_INACTIVE || minimized {
                    return DefSubclassProc(hwnd, msg, wparam, lparam);
                }
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

thread_local! {
    /// Set while `WM_ACTIVATE` runs, so its nested `WM_SETFOCUS` leaves the restore to it.
    static ACTIVATING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Label and remembered focus of the window `hwnd`, without waiting for a busy host.
fn win_state(hwnd: HWND) -> Option<(String, Focus)> {
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
fn restore_focus(hwnd: HWND, previous: Option<Focus>, only_if_lost: bool) {
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

// ---------------------------------------------------------------------------------------------
// Initialisation

/// Started-at instant, for the startup-time measurement (`ZENITH_PERF=1`).
pub static STARTED: OnceLock<std::time::Instant> = OnceLock::new();

/// `ZENITH_PROFILE_DIR` runs Zenith on a separate, self-contained profile (tests, measurements,
/// portable use). Such an instance also skips the single-instance redirect.
pub fn profile_override() -> Option<PathBuf> {
    std::env::var_os("ZENITH_PROFILE_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

pub fn init_services(app: &AppHandle) -> tauri::Result<()> {
    let (roaming, local) = match profile_override() {
        Some(dir) => (dir.join("roaming"), dir.join("local")),
        None => (app.path().app_data_dir()?, app.path().app_local_data_dir()?),
    };
    let _ = std::fs::create_dir_all(&roaming);
    let _ = std::fs::create_dir_all(&local);
    let paths = Paths { roaming, local };
    let _ = APP.set(app.clone());
    let _ = DB.set(Db::open(paths.db()));
    let _ = PATHS.set(paths);
    // Throwaway profiles of the separate-process incognito windows of earlier versions.
    crate::incognito::sweep_stale();
    // Media jobs do not survive a restart; drop their partial data.
    crate::media::job::cleanup_temp();
    db().exec(|c| {
        let _ = crate::store::history_prune(c, 0, 100_000);
    });
    Ok(())
}

/// Called once a window and its chrome webview exist (on the main thread).
pub fn attach_chrome(window: WebviewWindow, controller: ICoreWebView2Controller, env: ICoreWebView2Environment) {
    let label = window.label().to_string();
    let hwnd = window.hwnd().map(|h| HWND(h.0)).unwrap_or_default();
    let mut chrome_hwnd = HWND::default();
    let chrome_core = unsafe {
        let _ = controller.ParentWindow(&mut chrome_hwnd);
        controller.CoreWebView2().expect("chrome CoreWebView2")
    };
    unsafe {
        if let Ok(settings) = chrome_core.Settings() {
            // The chrome is an app surface: no browser reload/print/find accelerators,
            // no default context menu in release builds, no zoom.
            if let Ok(s3) = settings.cast::<ICoreWebView2Settings3>() {
                let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
            }
            let _ = settings.SetAreDefaultContextMenusEnabled(cfg!(debug_assertions));
            let _ = settings.SetIsZoomControlEnabled(false);
            let _ = settings.SetIsStatusBarEnabled(false);
            if let Ok(s5) = settings.cast::<ICoreWebView2Settings5>() {
                let _ = s5.SetIsPinchZoomEnabled(false);
            }
            if let Ok(s6) = settings.cast::<ICoreWebView2Settings6>() {
                let _ = s6.SetIsSwipeNavigationEnabled(false);
            }
        }
        attach_accelerators(&controller, None, &label);
        let mut token = 0i64;
        let focus_label = label.clone();
        let _ = controller.add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                let label = focus_label.clone();
                post(move || {
                    with_win(&label, |w| w.focus = Focus::Chrome);
                });
                Ok(())
            })),
            &mut token,
        );
        let crash_label = label.clone();
        // If the whole WebView2 browser process dies every view is gone; relaunch and let
        // session restore bring the tabs back.
        let _ = chrome_core.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    args.ProcessFailedKind(&mut kind)?;
                    if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                        eprintln!("[zenith] WebView2 browser process exited; restarting");
                        post(|| app().restart());
                    } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED {
                        let label = crash_label.clone();
                        post(move || {
                            with_win(&label, |w| {
                                let _ = w.chrome_core.Reload();
                            });
                        });
                    }
                }
                Ok(())
            })),
            &mut token,
        );
    }
    if MAIN_HWND.load(Ordering::SeqCst) == 0 {
        MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    }
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
    }
    // One chrome watches for stray page targets: discovery reports every target of the browser
    // process, so a second watcher would adopt each stray page twice.
    let first = HOST.with(|cell| cell.borrow().is_none());
    let target_watch = if first { watch_page_targets(&chrome_core) } else { None };
    let win = Win {
        window,
        hwnd,
        chrome: controller,
        chrome_core: chrome_core.clone(),
        chrome_hwnd,
        layout: LayoutSpec::default(),
        focus: Focus::Chrome,
        fullscreen_tab: None,
        chrome_raised: None,
        chrome_target_id: String::new(),
        _target_watch: target_watch,
    };
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        match guard.as_mut() {
            Some(h) => {
                h.wins.insert(label.clone(), win);
            }
            None => {
                *guard = Some(Host {
                    wins: HashMap::from([(label.clone(), win)]),
                    env,
                    tabs: HashMap::new(),
                    zombies: HashMap::new(),
                    page_bg: (32, 32, 36),
                    content_scheme: COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO,
                    permissions: HashMap::new(),
                    downloads: HashMap::new(),
                    target_owner: label.clone(),
                })
            }
        }
    });
    let id_label = label.clone();
    cdp_call(&chrome_core, "Target.getTargetInfo", "{}", move |r| {
        if let Some(id) = r.ok().as_deref().and_then(target_id_of) {
            with_win(&id_label, |w| w.chrome_target_id = id);
        }
    });
    if first {
        cdp_call(&chrome_core, "Target.setDiscoverTargets", r#"{"discover":true}"#, |r| {
            if let Err(e) = r {
                eprintln!("[zenith] target discovery unavailable: {e}");
            }
        });
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

/// Forgets a closing window and frees its pages (tabs with running downloads are kept until
/// they finish). The caller destroys the window itself afterwards.
pub fn detach_window(label: &str) {
    let views: Vec<Tab> = with_host(|h| {
        let Some(win) = h.wins.remove(label) else { return Vec::new() };
        let ids: Vec<String> = h.tabs.iter().filter(|(_, t)| t.window() == label).map(|(id, _)| id.clone()).collect();
        let mut freed = Vec::new();
        for id in ids {
            let Some(tab) = h.tabs.remove(&id) else { continue };
            h.permissions.retain(|_, p| p.tab != id);
            if h.downloads.values().any(|d| d.tab == id && d.record.state == "progress") {
                let _ = tab.view.set_visible(false);
                h.zombies.insert(id, tab);
            } else {
                freed.push(tab);
            }
        }
        if MAIN_HWND.load(Ordering::SeqCst) == win.hwnd.0 as isize {
            let next = h.wins.values().next().map(|w| w.hwnd.0 as isize).unwrap_or(0);
            MAIN_HWND.store(next, Ordering::SeqCst);
        }
        if h.target_owner == label {
            // The watcher went with the window; its replacement starts watching.
            if let Some((l, w)) = h.wins.iter_mut().next() {
                h.target_owner = l.clone();
                w._target_watch = watch_page_targets(&w.chrome_core);
                cdp_call(&w.chrome_core, "Target.setDiscoverTargets", r#"{"discover":true}"#, |_| {});
            }
        }
        freed
    })
    .unwrap_or_default();
    drop(views);
}

/// Drops every page webview. Called before exit so WebView2 shuts down cleanly.
pub fn shutdown() {
    let views: Vec<Tab> = with_host(|h| {
        let mut all: Vec<Tab> = h.tabs.drain().map(|(_, t)| t).collect();
        all.extend(h.zombies.drain().map(|(_, t)| t));
        h.permissions.clear();
        all
    })
    .unwrap_or_default();
    drop(views);
}

// ---------------------------------------------------------------------------------------------
// Layout, visibility and z-order

pub(crate) fn logical_rect(p: &PaneSpec) -> Rect {
    Rect {
        position: LogicalPosition::new(p.x, p.y).into(),
        size: LogicalSize::new(p.w.max(1.0), p.h.max(1.0)).into(),
    }
}

/// A logical rectangle in window pixels. The size comes from the rounded edges, so the right and
/// bottom edges never wobble by a pixel as the left edge moves.
fn px_rect(x: f64, y: f64, w: f64, h: f64, scale: f64) -> (i32, i32, i32, i32) {
    let left = (x * scale).round() as i32;
    let top = (y * scale).round() as i32;
    let pw = (((x + w) * scale).round() as i32 - left).max(1);
    let ph = (((y + h) * scale).round() as i32 - top).max(1);
    (left, top, pw, ph)
}

/// Places a tab's webview. The container window is moved synchronously, in step with the page
/// inside it (wry moves it asynchronously after resizing the page, which shows the page shifted
/// for a frame). A move that keeps the size leaves the page alone, so nothing is repainted.
fn place_view(view: &wry::WebView, p: &PaneSpec, prev: Option<(f64, f64, f64, f64)>, scale: f64) {
    let (left, top, w, h) = px_rect(p.x, p.y, p.w, p.h, scale);
    let same_size = prev.is_some_and(|(x, y, pw, ph)| {
        let (_, _, ow, oh) = px_rect(x, y, pw, ph, scale);
        (ow, oh) == (w, h)
    });
    unsafe {
        if same_size {
            let flags = SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE;
            let _ = SetWindowPos(view.hwnd(), None, left, top, 0, 0, flags);
        } else {
            let _ = SetWindowPos(view.hwnd(), None, left, top, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
            let _ = view.controller().SetBounds(RECT { left: 0, top: 0, right: w, bottom: h });
        }
    }
}

/// The window region (pixels from the webview's top-left) that shows only `vis_w`/`vis_h` of a
/// pane; None when the whole webview shows.
fn clip_of(p: &PaneSpec, scale: f64) -> Option<(i32, i32)> {
    if p.vis_w.is_none() && p.vis_h.is_none() {
        return None;
    }
    let (_, _, w, h) = px_rect(p.x, p.y, p.vis_w.unwrap_or(p.w), p.vis_h.unwrap_or(p.h), scale);
    Some((w, h))
}

/// Shows only the top-left `clip` pixels of a webview with a window region; None removes it.
fn clip_view(view: &wry::WebView, clip: Option<(i32, i32)>) {
    unsafe {
        let hwnd = view.hwnd();
        match clip {
            None => {
                let _ = SetWindowRgn(hwnd, None, true);
            }
            Some((w, h)) => {
                // The window owns the region from here on.
                let rgn = CreateRectRgn(0, 0, w, h);
                let _ = SetWindowRgn(hwnd, Some(rgn), true);
            }
        }
    }
}

impl Host {
    /// The WebView2 profile pages use: the user's profile (which every chrome uses too), or for
    /// incognito windows the InPrivate profile their tabs share (None while no such tab exists).
    pub fn page_profile(&self, private: bool) -> Option<ICoreWebView2Profile> {
        let core = if private {
            self.tabs.values().find(|t| t.shared.private)?.core()
        } else {
            self.wins.values().next()?.chrome_core.clone()
        };
        unsafe { core.cast::<ICoreWebView2_13>().and_then(|w| w.Profile()).ok() }
    }

    fn scale(&self, label: &str) -> f64 {
        let Some(win) = self.wins.get(label) else { return 1.0 };
        let dpi = unsafe { GetDpiForWindow(win.hwnd) };
        if dpi == 0 {
            1.0
        } else {
            dpi as f64 / 96.0
        }
    }

    pub fn pane_for(&self, label: &str, id: &str) -> Option<PaneSpec> {
        let layout = &self.wins.get(label)?.layout;
        layout
            .panes
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .or_else(|| layout.glance.clone().filter(|g| g.id == id))
    }


    pub fn apply_layout(&mut self, label: &str) {
        let scale = self.scale(label);
        let Some(win) = self.wins.get(label) else { return };
        let mut visible: HashMap<String, PaneSpec> = HashMap::new();
        if let Some(fs) = win.fullscreen_tab.clone().filter(|id| self.tabs.contains_key(id)) {
            visible.insert(
                fs.clone(),
                PaneSpec {
                    id: fs,
                    x: 0.0,
                    y: 0.0,
                    w: win.layout.viewport_w,
                    h: win.layout.viewport_h,
                    ..Default::default()
                },
            );
        } else {
            for p in &win.layout.panes {
                visible.insert(p.id.clone(), p.clone());
            }
            if let Some(g) = &win.layout.glance {
                visible.insert(g.id.clone(), g.clone());
            }
        }
        for (id, tab) in self.tabs.iter_mut() {
            if tab.shared.window != label {
                continue;
            }
            match visible.get(id) {
                Some(p) => {
                    // A region that is set or changed goes on before the webview grows or moves,
                    // so its hidden part never shows past the page edge; a region that comes off
                    // comes off after the webview has its final size.
                    let clip = clip_of(p, scale);
                    let clip_changed = tab.last_clip != clip;
                    if clip_changed && clip.is_some() {
                        clip_view(&tab.view, clip);
                    }
                    let rect = (p.x, p.y, p.w, p.h);
                    if tab.last_rect != Some(rect) {
                        place_view(&tab.view, p, tab.last_rect, scale);
                        tab.last_rect = Some(rect);
                    }
                    if clip_changed && clip.is_none() {
                        clip_view(&tab.view, None);
                    }
                    tab.last_clip = clip;
                    if !tab.visible {
                        let _ = tab.view.set_visible(true);
                        tab.visible = true;
                        tab.set_memory_level(false);
                    }
                }
                None => {
                    if tab.visible {
                        let _ = tab.view.set_visible(false);
                        tab.visible = false;
                        tab.set_memory_level(true);
                    }
                }
            }
        }
        self.apply_zorder(label);
    }

    pub fn apply_zorder(&mut self, label: &str) {
        let Some(win) = self.wins.get_mut(label) else { return };
        let raise = win.layout.overlay && win.fullscreen_tab.is_none();
        unsafe {
            if raise {
                let _ = SetWindowPos(
                    win.chrome_hwnd,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                if let Some(g) = &win.layout.glance {
                    if let Some(t) = self.tabs.get(&g.id) {
                        let _ = SetWindowPos(
                            t.view.hwnd(),
                            Some(HWND_TOP),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                }
            } else if win.chrome_raised != Some(false) {
                let _ = SetWindowPos(
                    win.chrome_hwnd,
                    Some(HWND_BOTTOM),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
        win.chrome_raised = Some(raise);
    }

    /// Re-derives pane rectangles for a new client size without waiting for the UI: margins to
    /// the window edges stay fixed and panes keep their relative split.
    fn relayout_for_size(&mut self, label: &str, width_px: i32, height_px: i32) {
        let scale = self.scale(label);
        let (new_w, new_h) = (width_px as f64 / scale, height_px as f64 / scale);
        let Some(win) = self.wins.get_mut(label) else { return };
        let old = win.layout.clone();
        if old.viewport_w <= 0.0 || old.viewport_h <= 0.0 {
            return;
        }
        let map_group = |panes: &[PaneSpec]| -> Vec<PaneSpec> {
            if panes.is_empty() {
                return Vec::new();
            }
            let min_x = panes.iter().map(|p| p.x).fold(f64::MAX, f64::min);
            let min_y = panes.iter().map(|p| p.y).fold(f64::MAX, f64::min);
            let max_x = panes.iter().map(|p| p.x + p.w).fold(f64::MIN, f64::max);
            let max_y = panes.iter().map(|p| p.y + p.h).fold(f64::MIN, f64::max);
            let (right, bottom) = (old.viewport_w - max_x, old.viewport_h - max_y);
            let (cw, ch) = ((max_x - min_x).max(1.0), (max_y - min_y).max(1.0));
            let (ncw, nch) = ((new_w - right - min_x).max(1.0), (new_h - bottom - min_y).max(1.0));
            panes
                .iter()
                .map(|p| PaneSpec {
                    id: p.id.clone(),
                    x: min_x + (p.x - min_x) / cw * ncw,
                    y: min_y + (p.y - min_y) / ch * nch,
                    w: p.w / cw * ncw,
                    h: p.h / ch * nch,
                    ..Default::default()
                })
                .collect()
        };
        win.layout.panes = map_group(&old.panes);
        if let Some(g) = &old.glance {
            let fx = new_w / old.viewport_w;
            let fy = new_h / old.viewport_h;
            win.layout.glance = Some(PaneSpec {
                id: g.id.clone(),
                x: g.x * fx,
                y: g.y * fy,
                w: g.w * fx,
                h: g.h * fy,
                ..Default::default()
            });
        }
        win.layout.viewport_w = new_w;
        win.layout.viewport_h = new_h;
        self.apply_layout(label);
    }
}

fn on_resize(hwnd: HWND, width_px: i32, height_px: i32) {
    with_host(|h| {
        let label = h.wins.iter().find(|(_, w)| w.hwnd == hwnd).map(|(l, _)| l.clone());
        if let Some(label) = label {
            h.relayout_for_size(&label, width_px, height_px);
        }
    });
}

pub fn set_layout(label: &str, spec: LayoutSpec) {
    with_host(|h| {
        if let Some(win) = h.wins.get_mut(label) {
            win.layout = spec;
            h.apply_layout(label);
        }
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugTab {
    id: String,
    target_id: String,
    requests: u32,
    blocked: u32,
    visible: bool,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugLayout {
    chrome_raised: bool,
    focus: String,
    tabs: Vec<DebugTab>,
    zombies: usize,
    downloads: usize,
    permissions: usize,
}

/// Read-only snapshot of one window's webview placement, used by the end-to-end test script.
pub fn debug_layout(label: &str) -> Option<DebugLayout> {
    with_host(|h| {
        let scale = h.scale(label);
        let win = h.wins.get(label)?;
        let mut tabs: Vec<DebugTab> = h
            .tabs
            .iter()
            .filter(|(_, t)| t.window() == label)
            .map(|(id, t)| {
                let b = t.view.bounds().ok();
                let (x, y, w, hh) = b
                    .map(|b| {
                        let p = b.position.to_logical::<f64>(scale);
                        let s = b.size.to_logical::<f64>(scale);
                        (p.x, p.y, s.width, s.height)
                    })
                    .unwrap_or_default();
                DebugTab {
                    id: id.clone(),
                    target_id: t.shared.target_id.borrow().clone(),
                    requests: t.shared.requests.get(),
                    blocked: t.shared.blocked.get(),
                    visible: t.visible,
                    x,
                    y,
                    w,
                    h: hh,
                }
            })
            .collect();
        tabs.sort_by(|a, b| a.id.cmp(&b.id));
        Some(DebugLayout {
            chrome_raised: win.chrome_raised.unwrap_or(false),
            focus: match &win.focus {
                Focus::Chrome => "chrome".into(),
                Focus::Tab(id) => id.clone(),
            },
            tabs,
            zombies: h.zombies.len(),
            downloads: h.downloads.len(),
            permissions: h.permissions.len(),
        })
    })
    .flatten()
}

/// Calls a DevTools protocol method on a webview's own DevTools session (no remote port).
pub fn cdp_call(core: &ICoreWebView2, method: &str, params: &str, done: impl FnOnce(Result<String, String>) + 'static) {
    let done = RefCell::new(Some(done));
    let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
        if let Some(done) = done.borrow_mut().take() {
            done(result.map(|_| json).map_err(|e| e.message().to_string()));
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) = core.CallDevToolsProtocolMethod(&HSTRING::from(method), &HSTRING::from(params), &handler) {
            eprintln!("[zenith] {method} failed: {e}");
        }
    }
}

/// Calls a DevTools protocol method on a chrome webview's own session (the target watcher's).
pub fn chrome_cdp(method: &str, params: &str, done: impl FnOnce(Result<String, String>) + 'static) {
    match with_host(|h| h.wins.get(&h.target_owner).or_else(|| h.wins.values().next()).map(|w| w.chrome_core.clone())).flatten() {
        Some(core) => cdp_call(&core, method, params, done),
        None => done(Err("browser is not ready".into())),
    }
}

pub(crate) fn target_id_of(json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json).ok()?["targetInfo"]["targetId"]
        .as_str()
        .map(str::to_string)
}

/// WebView2 opens some pages on its own, outside any Zenith view: an extension's welcome,
/// options or uninstall page (`chrome.tabs.create`). They would run invisibly, so each new page
/// target is checked shortly after it appears; one that belongs to no Zenith webview is closed and
/// its address reopened as a normal tab. While any view's identity is still unknown, nothing is
/// touched.
fn on_target_created(json: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return };
    let info = &v["targetInfo"];
    if info["type"].as_str() != Some("page") {
        return;
    }
    let Some(id) = info["targetId"].as_str().map(str::to_string) else { return };
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = app().run_on_main_thread(move || check_stray_target(id));
    });
}

fn check_stray_target(target_id: String) {
    let stray = with_host(|h| {
        // A window still being created has a chrome page that is not registered yet.
        if app().webview_windows().len() != h.wins.len() {
            return false;
        }
        for w in h.wins.values() {
            if w.chrome_target_id.is_empty() || w.chrome_target_id == target_id {
                return false;
            }
        }
        for t in h.tabs.values().chain(h.zombies.values()) {
            let known = t.shared.target_id.borrow();
            if known.is_empty() || *known == target_id {
                return false;
            }
        }
        true
    })
    .unwrap_or(false);
    if !stray {
        return;
    }
    chrome_cdp("Target.getTargets", "{}", move |r| {
        let Ok(json) = r else { return };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else { return };
        let Some(info) = v["targetInfos"]
            .as_array()
            .and_then(|all| all.iter().find(|t| t["targetId"].as_str() == Some(target_id.as_str())))
        else {
            return;
        };
        let url = info["url"].as_str().unwrap_or_default().to_string();
        let adoptable = url.starts_with("https://") || url.starts_with("http://") || url.starts_with("chrome-extension://");
        if info["type"].as_str() != Some("page") || !adoptable || is_chrome_url(&url) {
            return;
        }
        chrome_cdp("Target.closeTarget", &serde_json::json!({ "targetId": target_id }).to_string(), |_| {});
        // Extensions belong to the user's profile, so their pages open in the normal window.
        emit_to_window(MAIN, "tab-open-request", serde_json::json!({ "url": url, "openerId": null, "kind": "foreground" }));
    });
}

/// A page of Zenith's own UI (a window's chrome), never a stray page to adopt.
fn is_chrome_url(url: &str) -> bool {
    let Ok(url) = tauri::Url::parse(url) else { return false };
    let origin = url.origin();
    app()
        .webview_windows()
        .values()
        .filter_map(|w| w.url().ok())
        .any(|u| u.origin() == origin)
}

fn watch_page_targets(core: &ICoreWebView2) -> Option<ICoreWebView2DevToolsProtocolEventReceiver> {
    unsafe {
        let receiver = core.GetDevToolsProtocolEventReceiver(&HSTRING::from("Target.targetCreated")).ok()?;
        let mut token = 0i64;
        let _ = receiver.add_DevToolsProtocolEventReceived(
            &DevToolsProtocolEventReceivedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    let mut json = PWSTR::null();
                    args.ParameterObjectAsJson(&mut json)?;
                    on_target_created(&take_pwstr(json));
                }
                Ok(())
            })),
            &mut token,
        );
        Some(receiver)
    }
}

/// A page entered (`on`) or left HTML fullscreen: its window follows.
pub fn set_fullscreen_tab(tab_id: &str, on: bool) {
    let label = window_of_tab(tab_id);
    let window = with_win(&label, |w| {
        w.fullscreen_tab = on.then(|| tab_id.to_string());
        w.window.clone()
    });
    if let Some(window) = window {
        let _ = window.set_fullscreen(on);
    }
    with_host(|h| h.apply_layout(&label));
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

// ---------------------------------------------------------------------------------------------
// Keyboard shortcuts

thread_local! {
    static BINDINGS: RefCell<Bindings> = RefCell::new(Bindings::default());
}

/// Applies the shortcut bindings, including the system-wide ones. Returns the system-wide
/// shortcuts that could not be registered.
pub fn set_bindings(specs: Vec<BindingSpec>) -> Vec<String> {
    let bindings = Bindings::from_specs(&specs);
    BINDINGS.with(|b| *b.borrow_mut() = bindings);
    crate::hotkey::set(&specs)
}

/// Runs a browser action in window `label`'s chrome UI, as if its shortcut was pressed there.
pub fn emit_shortcut(label: &str, action: &str) {
    emit_to_window(label, "shortcut", ShortcutEvent { action: action.to_string(), repeat: false, source: None });
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ShortcutEvent {
    action: String,
    repeat: bool,
    source: Option<String>,
}

fn key_down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    unsafe { GetKeyState(vk.0 as i32) < 0 }
}

/// The browser action bound to a chord, if any.
fn bound_action(ctrl: bool, alt: bool, shift: bool, vk: u32) -> Option<String> {
    // AltGr is reported as Ctrl+Alt: leave those chords to text input.
    if ctrl && alt && key_down(VK_RMENU) {
        return None;
    }
    let chord = Chord { ctrl, alt, shift, vk };
    BINDINGS.with(|b| b.borrow().lookup(&chord).map(str::to_string))
}

/// Routes browser shortcuts from a webview (the chrome when `tab` is `None`) to the chrome UI of
/// window `label`. Matching keys are marked handled so the page never sees them; everything else
/// passes through untouched.
pub fn attach_accelerators(controller: &ICoreWebView2Controller, tab: Option<std::rc::Rc<TabShared>>, label: &str) {
    let label = label.to_string();
    let mut token = 0i64;
    unsafe {
        let _ = controller.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                    && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                {
                    return Ok(());
                }
                let mut vk = 0u32;
                args.VirtualKey(&mut vk)?;
                let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
                args.PhysicalKeyStatus(&mut status)?;
                let ctrl = key_down(VK_CONTROL);
                let shift = key_down(VK_SHIFT);
                let alt = status.IsMenuKeyDown.as_bool() || key_down(VK_MENU);
                let repeat = status.WasKeyDown.as_bool();
                let source = tab.as_ref().map(|t| t.id.clone());

                if let Some(t) = &tab {
                    if t.glance.get() && vk == VK_ESCAPE.0 as u32 && !ctrl && !alt && !shift {
                        args.SetHandled(true)?;
                        emit_to_window(&label, "shortcut", ShortcutEvent { action: "closeGlance".into(), repeat, source });
                        return Ok(());
                    }
                }
                if let Some(action) = bound_action(ctrl, alt, shift, vk) {
                    args.SetHandled(true)?;
                    emit_to_window(&label, "shortcut", ShortcutEvent { action, repeat, source });
                }
                Ok(())
            })),
            &mut token,
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Appearance

pub fn set_page_background(rgb: (u8, u8, u8)) {
    with_host(|h| {
        h.page_bg = rgb;
        for t in h.tabs.values().filter(|t| !t.shared.light_bg) {
            let _ = t.view.set_background_color((rgb.0, rgb.1, rgb.2, 255));
        }
    });
}

/// Sets `prefers-color-scheme` for web content ("system", "light" or "dark").
pub fn set_content_color_scheme(scheme: &str) {
    let value = match scheme {
        "light" => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT,
        "dark" => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK,
        _ => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO,
    };
    with_host(|h| {
        h.content_scheme = value;
        // InPrivate tabs (incognito windows) live in their own profile.
        for private in [false, true] {
            if let Some(profile) = h.page_profile(private) {
                unsafe {
                    let _ = profile.SetPreferredColorScheme(value);
                }
            }
        }
    });
}

/// Clears WebView2 browsing data of the user's profile, or of the InPrivate profile when asked
/// from an incognito window (`private`).
pub fn clear_browsing_data(
    private: bool,
    kinds: &[String],
    since_ms: Option<i64>,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let mut mask = COREWEBVIEW2_BROWSING_DATA_KINDS(0);
    for k in kinds {
        mask.0 |= match k.as_str() {
            "cookies" => COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES.0,
            "cache" => COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE.0 | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE.0,
            "storage" => COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_DOM_STORAGE.0,
            "siteData" => COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_SITE.0,
            "downloads" => COREWEBVIEW2_BROWSING_DATA_KINDS_DOWNLOAD_HISTORY.0,
            "autofill" => COREWEBVIEW2_BROWSING_DATA_KINDS_GENERAL_AUTOFILL.0
                | COREWEBVIEW2_BROWSING_DATA_KINDS_PASSWORD_AUTOSAVE.0,
            "permissions" => COREWEBVIEW2_BROWSING_DATA_KINDS_SETTINGS.0,
            _ => 0,
        };
    }
    let profile = with_host(|h| h.page_profile(private))
        .flatten()
        .and_then(|p| p.cast::<ICoreWebView2Profile2>().ok());
    let Some(profile) = profile else {
        // Incognito windows without open pages have nothing stored to clear.
        if private {
            done(Ok(()));
        } else {
            done(Err("profile unavailable".into()));
        }
        return;
    };
    let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |result| {
        done(result.map_err(|e| e.to_string()));
        Ok(())
    }));
    unsafe {
        let r = match since_ms {
            Some(since) => profile.ClearBrowsingDataInTimeRange(
                mask,
                since as f64 / 1000.0,
                crate::store::now_ms() as f64 / 1000.0 + 60.0,
                &handler,
            ),
            None => profile.ClearBrowsingData(mask, &handler),
        };
        if let Err(e) = r {
            eprintln!("[zenith] clear browsing data failed: {e}");
        }
    }
}
