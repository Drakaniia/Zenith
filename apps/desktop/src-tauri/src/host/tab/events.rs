//! CEF `Client` and per-tab handlers: lifecycle, navigation, display/favicons/fullscreen,
//! focus, keyboard accelerators, request/resource interception, permissions, downloads, find,
//! and context menus.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use base64::Engine as _;
use cef::*;
use windows::Win32::Foundation::HWND;

use super::context_menu::TabContextMenuHandler;
use super::find::TabFindHandler;
use super::new_window::handle_before_popup;
use super::permissions::TabPermissionHandler;
use super::requests::TabResourceRequestHandler;
use super::{is_web, zoom_factor_to_level, TabPatch, TabRef, TabShared};
use crate::app::bridge::GENERATIONS;
use crate::app::security::BrowserRole;
use crate::host::cef_app::{cef_str, opt_cef_str};
use crate::host::{
    cdp_call, config, db, emit_tab, handle_pre_key_event, maybe_drop_incognito_context, post,
    set_fullscreen_tab, target_id_of, track_browser, untrack_browser, with_host, with_win, Focus,
};
use crate::platform::urls::host_of;

wrap_download_image_callback! {
    struct FaviconDownloadCb {
        tab_id: String,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(
            &self,
            _image_url: Option<&CefString>,
            http_status_code: ::std::os::raw::c_int,
            image: Option<&mut Image>,
        ) {
            if http_status_code != 200 {
                return;
            }
            let Some(img) = image else { return };
            let (mut pw, mut ph) = (0, 0);
            if let Some(bin) = img.as_png(1.0, 1, Some(&mut pw), Some(&mut ph)) {
                let len = bin.size();
                if len > 0 && len <= 512 * 1024 {
                    let mut buf = vec![0u8; len];
                    let copied = bin.data(Some(&mut buf), 0);
                    if copied > 0 {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&buf[..copied]);
                        TabPatch { favicon: Some(format!("data:image/png;base64,{b64}")), ..TabPatch::new(&self.tab_id) }.send();
                    }
                }
            }
        }
    }
}

wrap_life_span_handler! {
    struct TabLifeSpanHandler {
        shared: Arc<TabShared>,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: ::std::os::raw::c_int,
            popup_features: Option<&PopupFeatures>,
            window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            settings: Option<&mut BrowserSettings>,
            extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            handle_before_popup(
                &self.shared,
                browser,
                frame,
                &opt_cef_str(target_url),
                &opt_cef_str(target_frame_name),
                target_disposition,
                user_gesture != 0,
                popup_features,
                window_info,
                client,
                settings,
                extra_info,
            )
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let bid = browser.identifier();
            track_browser(browser);
            let (id, window) = (self.shared.id(), self.shared.window());
            let role = BrowserRole::UntrustedPage { tab_id: id.clone(), window: window.clone() };
            let gen = GENERATIONS.lock().map(|mut g| g.register(bid, role)).unwrap_or(0);
            self.shared.generation.store(gen, Ordering::SeqCst);

            let browser_hwnd = browser.host().map(|h| HWND(h.window_handle().0.cast())).unwrap_or_default();
            let scheme = with_host(|h| {
                let scheme = h.content_scheme.clone();
                if self.shared.is_warm() {
                    if let Some(warm) = h.warm_tab.as_mut().filter(|w| Arc::ptr_eq(&w.shared, &self.shared)) {
                        warm.browser = Some(browser.clone());
                        warm.browser_hwnd = browser_hwnd;
                        return scheme;
                    }
                }
                if let Some(tab) = h.tabs.get_mut(&id) {
                    tab.browser = Some(browser.clone());
                    tab.browser_hwnd = browser_hwnd;
                    if let Some(host) = browser.host() {
                        if tab.pending_muted {
                            host.set_audio_muted(1);
                        }
                        if let Some(z) = tab.pending_zoom.take() {
                            host.set_zoom_level(zoom_factor_to_level(z));
                        }
                    }
                    if let Some(url) = tab.pending_url.take() {
                        if let Some(frame) = browser.main_frame() {
                            frame.load_url(Some(&CefString::from(url.as_str())));
                        }
                    }
                    tab.content_px = None;
                    h.apply_layout(&window);
                }
                scheme
            })
            .unwrap_or_default();

            if scheme == "light" || scheme == "dark" {
                let params = format!(r#"{{"features":[{{"name":"prefers-color-scheme","value":"{scheme}"}}]}}"#);
                cdp_call(browser, "Emulation.setEmulatedMedia", &params, |_| {});
            }

            let sh = self.shared.clone();
            cdp_call(browser, "Target.getTargetInfo", "{}", move |r| {
                if let Some(tid) = r.ok().as_deref().and_then(target_id_of) {
                    if let Ok(mut slot) = sh.target_id.write() {
                        *slot = tid;
                    }
                }
            });
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            if !self.shared.closed.load(Ordering::SeqCst) {
                if self.shared.is_warm() {
                    return 1;
                }
                let id = self.shared.id();
                // Page requested `window.close()` itself: ask the chrome UI to close the tab.
                // Return 1 so CEF does NOT send WM_CLOSE to the top-level Zenith window.
                emit_tab(&id, "tab-close-request", TabRef { id: id.clone() });
                return 1;
            }
            // Tab close initiated by Zenith: destroy only this child browser's HWND on the next
            // UI tick and return 1 so CEF does NOT send WM_CLOSE to our top-level parent window.
            if let Some(raw) = browser
                .and_then(|b| b.host())
                .map(|h| h.window_handle().0 as isize)
                .filter(|&p| p != 0)
            {
                post(move || {
                    let hwnd = HWND(raw as *mut _);
                    if !hwnd.is_invalid() {
                        unsafe {
                            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
                        }
                    }
                });
            }
            1
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                let bid = browser.identifier();
                if let Ok(mut g) = GENERATIONS.lock() {
                    g.tombstone(bid);
                }
                untrack_browser(bid);
            }
            let raw = self.shared.container_hwnd.swap(0, Ordering::SeqCst);
            if raw != 0 {
                post(move || {
                    let hwnd = HWND(raw as *mut _);
                    if !hwnd.is_invalid() {
                        unsafe {
                            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
                        }
                    }
                    maybe_drop_incognito_context();
                });
            } else {
                maybe_drop_incognito_context();
            }
        }
    }
}

wrap_load_handler! {
    struct TabLoadHandler {
        shared: Arc<TabShared>,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            if self.shared.closed.load(Ordering::SeqCst) || self.shared.is_warm() {
                return;
            }
            let loading = is_loading != 0;
            let url = self.shared.url();
            let title = self.shared.title();
            let id = self.shared.id();
            TabPatch {
                loading: Some(loading),
                can_go_back: Some(can_go_back != 0),
                can_go_forward: Some(can_go_forward != 0),
                blocked: (!loading).then(|| self.shared.blocked()),
                ..TabPatch::new(&id)
            }
            .send();

            if !loading {
                if is_web(&url) && config().record_history && !self.shared.private {
                    let u = url.clone();
                    let t = title.clone();
                    db().exec(move |c| {
                        let _ = crate::history::history_add_visit(c, &u, &t);
                    });
                }
                if let Some((x, y)) = self.shared.restore_scroll.lock().ok().and_then(|mut s| s.take()) {
                    if let Some(frame) = browser.and_then(|b| b.main_frame()) {
                        let js = format!("window.scrollTo({x}, {y})");
                        frame.execute_java_script(
                            Some(&CefString::from(js.as_str())),
                            Some(&CefString::from("zenith://internal/scroll.js")),
                            1,
                        );
                    }
                }
            }
        }

        fn on_load_start(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _transition_type: TransitionType,
        ) {
            if self.shared.closed.load(Ordering::SeqCst) || self.shared.is_warm() {
                return;
            }
            if let Some(frame) = frame {
                super::scripts::inject_ext_compat(frame, &self.shared);
            }
        }

        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            if self.shared.closed.load(Ordering::SeqCst) || self.shared.is_warm() {
                return;
            }
            if let Some(frame) = frame {
                if frame.is_main() != 0 {
                    let url = cef_str(frame.url());
                    super::requests::inject_cosmetic_css(frame, &url);
                    crate::media::sniff::sync_frame_button(frame, &url);
                }
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            _failed_url: Option<&CefString>,
        ) {
            if self.shared.closed.load(Ordering::SeqCst) || self.shared.is_warm() {
                return;
            }
            if frame.is_some_and(|f| f.is_main() == 0) {
                return;
            }
            if error_code == Errorcode::ABORTED {
                return;
            }
            let err = opt_cef_str(error_text);
            let id = self.shared.id();
            TabPatch {
                loading: Some(false),
                error: Some(if err.is_empty() { format!("{error_code:?}") } else { err }),
                ..TabPatch::new(&id)
            }
            .send();
        }
    }
}

wrap_display_handler! {
    struct TabDisplayHandler {
        shared: Arc<TabShared>,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if self.shared.closed.load(Ordering::SeqCst) || self.shared.is_warm() || frame.is_some_and(|f| f.is_main() == 0) {
                return;
            }
            let uri = opt_cef_str(url);
            if let Ok(mut u) = self.shared.url.write() {
                *u = uri.clone();
            }
            let id = self.shared.id();
            TabPatch { url: Some(uri), ..TabPatch::new(&id) }.send();
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            if self.shared.closed.load(Ordering::SeqCst) || self.shared.is_warm() {
                return;
            }
            let title = opt_cef_str(title);
            if let Ok(mut t) = self.shared.title.write() {
                *t = title.clone();
            }
            let url = self.shared.url();
            if is_web(&url) && !title.is_empty() && config().record_history && !self.shared.private {
                let t = title.clone();
                db().exec(move |c| {
                    let _ = crate::history::history_set_title(c, &url, &t);
                });
            }
            let id = self.shared.id();
            TabPatch { title: Some(title), ..TabPatch::new(&id) }.send();
        }

        fn on_favicon_urlchange(
            &self,
            browser: Option<&mut Browser>,
            icon_urls: Option<&mut CefStringList>,
        ) {
            if self.shared.closed.load(Ordering::SeqCst) || self.shared.is_warm() {
                return;
            }
            let id = self.shared.id();
            let first = icon_urls
                .and_then(|list| list.clone().into_iter().next())
                .unwrap_or_default();
            if first.is_empty() {
                TabPatch { favicon: Some(String::new()), ..TabPatch::new(&id) }.send();
                return;
            }
            if let Some(host) = browser.and_then(|b| b.host()) {
                let mut cb = FaviconDownloadCb::new(id);
                host.download_image(Some(&CefString::from(first.as_str())), 1, 64, 0, Some(&mut cb));
            }
        }

        fn on_fullscreen_mode_change(
            &self,
            _browser: Option<&mut Browser>,
            fullscreen: ::std::os::raw::c_int,
        ) {
            if self.shared.is_warm() {
                return;
            }
            let on = fullscreen != 0;
            let id = self.shared.id();
            let id2 = id.clone();
            post(move || set_fullscreen_tab(&id2, on));
            TabPatch { fullscreen: Some(on), ..TabPatch::new(&id) }.send();
        }
    }
}

wrap_focus_handler! {
    struct TabFocusHandler {
        shared: Arc<TabShared>,
    }

    impl FocusHandler {
        fn on_got_focus(&self, _browser: Option<&mut Browser>) {
            if self.shared.is_warm() {
                return;
            }
            let id = self.shared.id();
            let focused = id.clone();
            let window = self.shared.window();
            post(move || {
                with_win(&window, |w| w.focus = Focus::Tab(focused));
            });
            emit_tab(&id.clone(), "tab-focused", TabRef { id });
        }
    }
}

wrap_keyboard_handler! {
    struct TabKeyboardHandler {
        shared: Arc<TabShared>,
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
            let window = self.shared.window();
            if handle_pre_key_event(ev, Some(&self.shared), &window) { 1 } else { 0 }
        }
    }
}

wrap_request_handler! {
    struct TabRequestHandler {
        shared: Arc<TabShared>,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            user_gesture: ::std::os::raw::c_int,
            is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let is_main = frame.is_some_and(|f| f.is_main() != 0);
            if !is_main || self.shared.is_warm() {
                return 0;
            }
            let uri = request.map(|r| cef_str(r.url())).unwrap_or_default();
            let tab_id = self.shared.id();
            let window = self.shared.window();
            if super::new_window::intercept_pinned_external(&self.shared, &uri, user_gesture != 0, is_redirect != 0)
                || crate::torrents::open_from_page(&uri, &tab_id, self.shared.private, &window)
                || crate::security::intercept_navigation(&tab_id, &uri)
            {
                return 1;
            }
            if let Ok(mut h) = self.shared.page_host.write() {
                *h = host_of(&uri).unwrap_or("").to_ascii_lowercase();
            }
            if let Ok(mut n) = self.shared.nav_uri.write() {
                *n = uri;
            }
            self.shared.blocked.store(0, Ordering::Relaxed);
            self.shared.requests.store(0, Ordering::Relaxed);
            if let Ok(mut media) = self.shared.media.lock() {
                media.reset();
            }
            crate::media::sniff::notify(&self.shared);
            let nav_tab_id = tab_id.clone();
            post(move || crate::host::autofill::on_navigation(&nav_tab_id));
            TabPatch {
                loading: Some(true),
                audible: Some(false),
                blocked: Some(0),
                crashed: Some(String::new()),
                error: Some(String::new()),
                ..TabPatch::new(&tab_id)
            }
            .send();
            0
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            Some(TabResourceRequestHandler::new(self.shared.clone()))
        }

        fn on_certificate_error(
            &self,
            _browser: Option<&mut Browser>,
            cert_error: Errorcode,
            request_url: Option<&CefString>,
            _ssl_info: Option<&mut Sslinfo>,
            callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            let id = self.shared.id();
            match callback {
                Some(cb) if crate::security::queue_cert_error(&id, cert_error, opt_cef_str(request_url), cb.clone()) => 1,
                _ => 0,
            }
        }

        fn on_render_process_terminated(
            &self,
            browser: Option<&mut Browser>,
            status: TerminationStatus,
            _error_code: ::std::os::raw::c_int,
            _error_string: Option<&CefString>,
        ) {
            if let Some(browser) = browser {
                if let Ok(mut g) = GENERATIONS.lock() {
                    if let Some(gen) = g.bump_generation(browser.identifier()) {
                        self.shared.generation.store(gen, Ordering::SeqCst);
                    }
                }
            }
            if self.shared.is_warm() {
                return;
            }
            let state = match status {
                TerminationStatus::PROCESS_WAS_KILLED | TerminationStatus::PROCESS_CRASHED | TerminationStatus::PROCESS_OOM => "crashed",
                _ => "crashed",
            };
            let id = self.shared.id();
            TabPatch {
                crashed: Some(state.into()),
                loading: Some(false),
                ..TabPatch::new(&id)
            }
            .send();
        }
    }
}

wrap_client! {
    struct TabClient {
        shared: Arc<TabShared>,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(TabLifeSpanHandler::new(self.shared.clone()))
        }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(TabLoadHandler::new(self.shared.clone()))
        }
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(TabDisplayHandler::new(self.shared.clone()))
        }
        fn focus_handler(&self) -> Option<FocusHandler> {
            Some(TabFocusHandler::new(self.shared.clone()))
        }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(TabKeyboardHandler::new(self.shared.clone()))
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(TabRequestHandler::new(self.shared.clone()))
        }
        fn permission_handler(&self) -> Option<PermissionHandler> {
            Some(TabPermissionHandler::new(self.shared.clone()))
        }
        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(crate::host::downloads::create_download_handler(self.shared.clone()))
        }
        fn find_handler(&self) -> Option<FindHandler> {
            Some(TabFindHandler::new(self.shared.clone()))
        }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(TabContextMenuHandler::new(self.shared.clone()))
        }
    }
}

pub(crate) fn create_tab_client(shared: Arc<TabShared>) -> Client {
    TabClient::new(shared)
}
