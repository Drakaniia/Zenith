//! Window lifecycle: process-wide service initialization (with pre-CEF backup), creating the
//! trusted CEF Alloy chrome surface, detaching closing windows, and clean CEF shutdown.

use std::path::PathBuf;
use std::sync::OnceLock;

use cef::*;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

use super::cef_app::cef_str;
use super::devtools::{cdp_call, open_devtools, target_id_of};
use super::state::{APP, DB, PATHS};
use super::{track_browser, untrack_browser, with_host, with_win, Focus, LayoutSpec, Win};
use crate::app::bridge::{bridge_token, handle_invoke_message, trusted_ui_url, GENERATIONS, IPC_INVOKE_MSG};
use crate::app::runtime::{AppHandle, WindowHandle};
use crate::app::security::{is_trusted_origin, BrowserRole};
use crate::storage::{Db, Paths};

pub static STARTED: OnceLock<std::time::Instant> = OnceLock::new();

pub fn profile_override() -> Option<PathBuf> {
    std::env::var_os("ZENITH_PROFILE_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

pub fn init_services(app: &AppHandle) -> Result<(), String> {
    let paths: Paths = crate::profiles::active().paths();
    let _ = std::fs::create_dir_all(&paths.roaming);
    let _ = std::fs::create_dir_all(&paths.local);
    // Preserve application-owned data before the first CEF startup without touching WebView2/.
    if let Err(e) = crate::storage::migration::ensure_pre_cef_backup(&paths) {
        eprintln!("[zenith] pre-cef backup warning: {e}");
    }
    let _ = APP.set(app.clone());
    let _ = DB.set(Db::open(paths.db()));
    let _ = PATHS.set(paths);
    crate::incognito::sweep_stale();
    crate::media::job::cleanup_temp();
    crate::torrents::restore();
    crate::security::init_safebrowsing();
    super::db().exec(|c| {
        let _ = crate::history::history_prune(c, 0, 100_000);
    });
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Trusted UI Alloy CEF client (strictly isolated from untrusted origins and popups)

wrap_life_span_handler! {
    struct ChromeLifeSpanHandler {
        label: String,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // Never allow popups from the trusted chrome UI.
            1
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let id = browser.identifier();
            track_browser(browser);
            if let Ok(mut g) = GENERATIONS.lock() {
                g.register(id, BrowserRole::TrustedUi { window: self.label.clone() });
            }
            let chrome_hwnd = browser
                .host()
                .map(|h| HWND(h.window_handle().0.cast()))
                .unwrap_or_default();
            let label = self.label.clone();
            with_host(|h| {
                if let Some(win) = h.wins.get_mut(&label) {
                    win.chrome = Some(browser.clone());
                    win.chrome_hwnd = chrome_hwnd;
                    win.chrome_browser_id = id;
                    let mut r = RECT::default();
                    unsafe {
                        let _ = GetClientRect(win.hwnd, &mut r);
                        if !chrome_hwnd.is_invalid() {
                            let _ = SetWindowPos(
                                chrome_hwnd,
                                None,
                                0,
                                0,
                                (r.right - r.left).max(1),
                                (r.bottom - r.top).max(1),
                                SWP_NOACTIVATE | SWP_NOZORDER,
                            );
                        }
                    }
                    h.apply_zorder(&label);
                }
            });
            let id_label = self.label.clone();
            cdp_call(browser, "Target.getTargetInfo", "{}", move |r| {
                if let Some(tid) = r.ok().as_deref().and_then(target_id_of) {
                    with_win(&id_label, |w| w.chrome_target_id = tid);
                }
            });
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                let id = browser.identifier();
                if let Ok(mut g) = GENERATIONS.lock() {
                    g.tombstone(id);
                }
                untrack_browser(id);
            }
        }
    }
}

wrap_request_handler! {
    struct ChromeRequestHandler {
        label: String,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: ::std::os::raw::c_int,
            _is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let is_main = frame.is_some_and(|f| f.is_main() != 0);
            if !is_main {
                return 0;
            }
            let url = request.map(|r| cef_str(r.url())).unwrap_or_default();
            // Fail closed: trusted UI may never navigate to an untrusted origin.
            if !is_trusted_origin(&url) {
                eprintln!("[zenith] blocked trusted UI navigation to untrusted origin: {url}");
                return 1;
            }
            0
        }

        fn on_render_process_terminated(
            &self,
            browser: Option<&mut Browser>,
            _status: TerminationStatus,
            _error_code: ::std::os::raw::c_int,
            _error_string: Option<&CefString>,
        ) {
            if let Some(browser) = browser {
                let id = browser.identifier();
                if let Ok(mut g) = GENERATIONS.lock() {
                    let _ = g.bump_generation(id);
                }
                eprintln!("[zenith] trusted UI renderer terminated in {}; reloading", self.label);
                browser.reload();
            }
        }
    }
}

wrap_focus_handler! {
    struct ChromeFocusHandler {
        label: String,
    }

    impl FocusHandler {
        fn on_got_focus(&self, _browser: Option<&mut Browser>) {
            let label = self.label.clone();
            super::post(move || {
                with_win(&label, |w| w.focus = Focus::Chrome);
            });
        }
    }
}

wrap_keyboard_handler! {
    struct ChromeKeyboardHandler {
        label: String,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut cef::sys::MSG>,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let Some(ev) = event else { return 0 };
            if super::handle_pre_key_event(ev, None, &self.label) { 1 } else { 0 }
        }
    }
}

wrap_context_menu_handler! {
    struct ChromeContextMenuHandler;

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            // Suppress default Chromium context menu on the app chrome in release builds.
            #[cfg(not(debug_assertions))]
            if let Some(m) = model {
                let _ = m.clear();
            }
            #[cfg(debug_assertions)]
            let _ = model;
        }
    }
}

wrap_client! {
    struct ChromeClient {
        label: String,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(ChromeLifeSpanHandler::new(self.label.clone()))
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(ChromeRequestHandler::new(self.label.clone()))
        }
        fn focus_handler(&self) -> Option<FocusHandler> {
            Some(ChromeFocusHandler::new(self.label.clone()))
        }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(ChromeKeyboardHandler::new(self.label.clone()))
        }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(ChromeContextMenuHandler::new())
        }
        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            if source_process != ProcessId::RENDERER {
                return 0;
            }
            let (Some(browser), Some(frame), Some(msg)) = (browser, frame, message) else {
                return 0;
            };
            if cef_str(msg.name()) != IPC_INVOKE_MSG {
                return 0;
            }
            let raw = msg
                .argument_list()
                .map(|a| cef_str(a.string(0)))
                .unwrap_or_default();
            handle_invoke_message(browser, frame, &raw);
            1
        }
    }
}

/// Registers a newly created Win32 window and launches its Alloy CEF trusted UI child browser.
pub fn create_chrome_window(window: WindowHandle, private: bool) -> Result<(), String> {
    let label = window.label().to_string();
    let hwnd = window.hwnd();
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
    }
    let w = (r.right - r.left).max(1);
    let h = (r.bottom - r.top).max(1);

    with_host(|host| {
        if host.wins.is_empty() {
            host.target_owner = label.clone();
        }
        host.wins.insert(
            label.clone(),
            Win {
                window,
                hwnd,
                chrome: None,
                chrome_hwnd: HWND::default(),
                chrome_browser_id: -1,
                layout: LayoutSpec::default(),
                focus: Focus::Chrome,
                fullscreen_tab: None,
                chrome_raised: None,
                chrome_rgn_key: None,
                chrome_target_id: String::new(),
            },
        );
    });

    let mut win_info = WindowInfo::default().set_as_child(
        cef::sys::HWND(hwnd.0.cast()),
        &Rect { x: 0, y: 0, width: w, height: h },
    );
    // Alloy style for trusted Svelte UI: lighter footprint and excluded from Chrome tab lists.
    win_info.runtime_style = RuntimeStyle::ALLOY;

    let mut extra = dictionary_value_create().ok_or("dictionary_value_create failed")?;
    extra.set_string(Some(&CefString::from("zenith_role")), Some(&CefString::from("trusted_ui")));
    extra.set_string(
        Some(&CefString::from("zenith_token")),
        Some(&CefString::from(bridge_token().as_str())),
    );
    extra.set_string(Some(&CefString::from("zenith_window")), Some(&CefString::from(label.as_str())));
    extra.set_string(
        Some(&CefString::from("zenith_incognito")),
        Some(&CefString::from(if private { "1" } else { "0" })),
    );

    let mut client = ChromeClient::new(label);
    let mut settings = BrowserSettings::default();
    settings.background_color = 0xFF131313;
    let url = trusted_ui_url();
    if browser_host_create_browser(
        Some(&win_info),
        Some(&mut client),
        Some(&CefString::from(url.as_str())),
        Some(&settings),
        Some(&mut extra),
        None,
    ) == 0
    {
        return Err("browser_host_create_browser failed for trusted UI".into());
    }
    Ok(())
}

pub fn open_chrome_devtools(label: &str) -> Result<(), String> {
    let browser = with_host(|h| h.wins.get(label).and_then(|w| w.chrome.clone()))
        .flatten()
        .ok_or("chrome UI is not ready")?;
    open_devtools(&browser)
}

/// Detaches a closing window and closes its chrome and tab CEF browsers.
pub fn detach_window(label: &str) {
    if let Ok(mut b) = crate::extensions::bridge::BRIDGE.write() {
        b.drop_window(label);
    }
    let to_close: Vec<Browser> = with_host(|h| {
        let mut browsers = Vec::new();
        if let Some(win) = h.wins.remove(label) {
            if let Some(b) = win.chrome {
                browsers.push(b);
            }
        }
        let ids: Vec<String> = h
            .tabs
            .iter()
            .filter(|(_, t)| t.window() == label)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            h.permissions.retain(|_, p| p.tab != id);
            h.cert_errors.retain(|_, p| p.tab != id);
            h.protocol_prompts.retain(|_, p| p.tab != id);
            h.pending_downloads.retain(|_, p| p.tab != id);
            if let Ok(mut sb) = crate::security::SAFE_BROWSING.write() {
                sb.drop_tab(&id);
            }
            if let Some(tab) = h.tabs.remove(&id) {
                tab.shared.closed.store(true, std::sync::atomic::Ordering::SeqCst);
                if let Some(b) = tab.browser {
                    browsers.push(b);
                } else {
                    tab.destroy_container();
                }
            }
        }
        if h.target_owner == label {
            if let Some(next) = h.wins.keys().next().cloned() {
                h.target_owner = next;
            }
        }
        browsers
    })
    .unwrap_or_default();

    for b in to_close {
        if let Some(host) = b.host() {
            host.close_browser(1);
        }
    }
    super::maybe_drop_incognito_context();
}

/// Closes all tab browsers and pending permissions on app quit.
pub fn shutdown() {
    let browsers: Vec<Browser> = with_host(|h| {
        h.permissions.clear();
        h.cert_errors.clear();
        h.protocol_prompts.clear();
        h.pending_downloads.clear();
        let mut out = Vec::new();
        for (_, tab) in h.tabs.drain() {
            tab.shared.closed.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(b) = tab.browser {
                out.push(b);
            } else {
                tab.destroy_container();
            }
        }
        h.incognito_ctx = None;
        out
    })
    .unwrap_or_default();
    for b in browsers {
        if let Some(host) = b.host() {
            host.close_browser(1);
        }
    }
}

/// Closes every remaining CEF browser (chrome, tabs, extension-created) and quits the CEF loop.
pub fn close_all_for_exit() {
    let (browsers, empty) = with_host(|h| {
        h.shutting_down = true;
        let list: Vec<Browser> = h.all_browsers.values().cloned().collect();
        let empty = list.is_empty();
        (list, empty)
    })
    .unwrap_or((Vec::new(), true));

    if empty {
        cef::quit_message_loop();
        return;
    }
    for b in browsers {
        if let Some(host) = b.host() {
            host.close_browser(1);
        }
    }
    // Also ask Chromium to close any Chrome-owned windows created outside our client handlers.
    super::browser_cdp("Browser.close", "{}", |_| {});
    // Safety net: quit the CEF message loop if any external browser stalls close.
    crate::app::runtime::post_ui_delayed(2500, || {
        cef::quit_message_loop();
    });
}
