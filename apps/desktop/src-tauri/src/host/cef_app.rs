//! CEF `App`, `BrowserProcessHandler`, and `RenderProcessHandler`.
//!
//! - Disables unsolicited Google telemetry/background services and external Chrome registry
//!   extensions.
//! - Registers the `zenith://app/` custom scheme for the trusted Svelte UI.
//! - Injects the authenticated V8 bridge ONLY into `TrustedUi` main frames; ordinary web pages
//!   receive ONLY Zenith's unprivileged page scripts (modified-click, autofill, Web Store,
//!   media button, extension popup helper).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cef::*;

thread_local! {
    /// Registered V8 message callbacks for `TrustedUi` main frames on `TID_RENDERER`, avoiding
    /// per-message `execute_java_script` JS source compilation on IPC replies and events.
    static RECV_CBS: RefCell<HashMap<i32, (V8Context, V8Value)>> = RefCell::new(HashMap::new());
}

use crate::app::bridge::{
    bridge_init_script, ui_dist_dir, ZenithSchemeFactory, IPC_EVENT_MSG, IPC_INVOKE_MSG,
    IPC_RESPONSE_MSG,
};
use crate::app::security::{is_trusted_origin, TRUSTED_HOST, TRUSTED_SCHEME};

static INITIAL_WINDOW: Mutex<Option<String>> = Mutex::new(None);

pub fn set_initial_window(label: String) {
    if let Ok(mut slot) = INITIAL_WINDOW.lock() {
        *slot = Some(label);
    }
}

fn take_initial_window() -> Option<String> {
    INITIAL_WINDOW.lock().ok().and_then(|mut s| s.take())
}

pub fn cef_str(s: CefStringUserfree) -> String {
    CefString::from(&s).to_string()
}

pub fn opt_cef_str(s: Option<&CefString>) -> String {
    s.map(|v| v.to_string()).unwrap_or_default()
}

#[derive(Clone)]
struct TrustedMeta {
    token: String,
    window: String,
    incognito: bool,
}

#[derive(Clone, Default)]
struct PageMeta {
    private: bool,
    ext_popup_opener: Option<String>,
    media_button: bool,
}

#[derive(Default)]
struct RenderState {
    trusted: HashMap<i32, TrustedMeta>,
    pages: HashMap<i32, PageMeta>,
}

wrap_v8_handler! {
    struct NativeSendHandler {
        frame: Frame,
    }

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            let payload = arguments
                .and_then(|a| a.first())
                .and_then(|v| v.as_ref())
                .map(|v| cef_str(v.string_value()))
                .unwrap_or_default();
            if let Some(mut msg) = process_message_create(Some(&CefString::from(IPC_INVOKE_MSG))) {
                if let Some(args) = msg.argument_list() {
                    args.set_string(0, Some(&CefString::from(payload.as_str())));
                }
                self.frame.send_process_message(ProcessId::BROWSER, Some(&mut msg));
            }
            if let Some(ret) = retval {
                *ret = v8_value_create_undefined();
            }
            1
        }
    }
}

wrap_v8_handler! {
    struct NativeRecvHandler {
        browser_id: i32,
        context: V8Context,
    }

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            if let Some(cb) = arguments.and_then(|a| a.first()).and_then(|v| v.as_ref()).filter(|v| v.is_function() != 0) {
                RECV_CBS.with(|m| {
                    m.borrow_mut().insert(self.browser_id, (self.context.clone(), cb.clone()));
                });
            }
            if let Some(ret) = retval {
                *ret = v8_value_create_undefined();
            }
            1
        }
    }
}

wrap_render_process_handler! {
    struct ZenithRenderHandler {
        state: Arc<Mutex<RenderState>>,
    }

    impl RenderProcessHandler {
        fn on_browser_created(
            &self,
            browser: Option<&mut Browser>,
            extra_info: Option<&mut DictionaryValue>,
        ) {
            let (Some(browser), Some(info)) = (browser, extra_info) else { return };
            let id = browser.identifier();
            let role = cef_str(info.string(Some(&CefString::from("zenith_role"))));
            let Ok(mut st) = self.state.lock() else { return };
            if role == "trusted_ui" {
                let token = cef_str(info.string(Some(&CefString::from("zenith_token"))));
                let window = cef_str(info.string(Some(&CefString::from("zenith_window"))));
                let incognito = cef_str(info.string(Some(&CefString::from("zenith_incognito")))) == "1";
                if !token.is_empty() {
                    st.trusted.insert(id, TrustedMeta { token, window, incognito });
                }
            } else if role == "page" {
                let private = cef_str(info.string(Some(&CefString::from("zenith_private")))) == "1";
                let media_button = cef_str(info.string(Some(&CefString::from("zenith_media_button")))) == "1";
                let opener = cef_str(info.string(Some(&CefString::from("zenith_ext_opener"))));
                let ext_popup_opener = if info.has_key(Some(&CefString::from("zenith_ext_popup"))) != 0 {
                    Some(opener)
                } else {
                    None
                };
                st.pages.insert(id, PageMeta { private, ext_popup_opener, media_button });
            }
        }

        fn on_browser_destroyed(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let id = browser.identifier();
            RECV_CBS.with(|m| {
                m.borrow_mut().remove(&id);
            });
            if let Ok(mut st) = self.state.lock() {
                st.trusted.remove(&id);
                st.pages.remove(&id);
            }
        }

        fn on_context_released(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _context: Option<&mut V8Context>,
        ) {
            if frame.is_some_and(|f| f.is_main() != 0) {
                if let Some(browser) = browser {
                    let id = browser.identifier();
                    RECV_CBS.with(|m| {
                        m.borrow_mut().remove(&id);
                    });
                }
            }
        }

        fn on_context_created(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            let (Some(browser), Some(frame), Some(context)) = (browser, frame, context) else {
                return;
            };
            let id = browser.identifier();
            let is_main = frame.is_main() != 0;
            let url = cef_str(frame.url());

            let (trusted, page) = match self.state.lock() {
                Ok(st) => (st.trusted.get(&id).cloned(), st.pages.get(&id).cloned()),
                Err(_) => return,
            };

            if let Some(meta) = trusted {
                if !is_main || !is_trusted_origin(&url) {
                    return;
                }
                let Some(global) = context.global() else { return };
                let mut handler = NativeSendHandler::new(frame.clone());
                let fn_name = CefString::from("__zenith_native_send");
                if let Some(mut func) = v8_value_create_function(Some(&fn_name), Some(&mut handler)) {
                    global.set_value_bykey(Some(&fn_name), Some(&mut func), V8Propertyattribute::default());
                }
                let mut recv_handler = NativeRecvHandler::new(id, context.clone());
                let recv_name = CefString::from("__zenith_native_on_msg");
                if let Some(mut func) = v8_value_create_function(Some(&recv_name), Some(&mut recv_handler)) {
                    global.set_value_bykey(Some(&recv_name), Some(&mut func), V8Propertyattribute::default());
                }
                let init_js = bridge_init_script(&meta.token, &meta.window, meta.incognito);
                eval_v8(context, &init_js, "zenith://internal/bridge.js");
                return;
            }

            // Untrusted page browser: NEVER install privileged IPC. Run only unprivileged page scripts.
            let page = page.unwrap_or_default();
            eval_v8(context, super::tab::PAGE_SCRIPT, "zenith://internal/page.js");
            if is_main {
                eval_v8(context, super::autofill::PAGE_SCRIPT, "zenith://internal/autofill.js");
                if !page.private {
                    eval_v8(context, super::tab::STORE_SCRIPT, "zenith://internal/store.js");
                }
                if page.media_button {
                    eval_v8(context, crate::media::detect::BUTTON_SCRIPT, "zenith://internal/media-btn.js");
                }
                if let Some(opener) = page.ext_popup_opener {
                    let opener_opt = (!opener.is_empty()).then_some(opener);
                    let opener_json = serde_json::to_string(&opener_opt).unwrap_or_else(|_| "null".into());
                    let script = super::tab::EXT_POPUP_SCRIPT.replace("__OPENER__", &opener_json);
                    eval_v8(context, &script, "zenith://internal/ext-popup.js");
                }
            }
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            let (Some(frame), Some(msg)) = (frame, message) else { return 0 };
            let name = cef_str(msg.name());
            if name != IPC_RESPONSE_MSG && name != IPC_EVENT_MSG {
                return 0;
            }
            let payload = msg
                .argument_list()
                .map(|a| cef_str(a.string(0)))
                .unwrap_or_default();

            if let Some(bid) = browser.map(|b| b.identifier()) {
                let entry = RECV_CBS.with(|m| m.borrow().get(&bid).cloned());
                if let Some((mut ctx, cb)) = entry {
                    if ctx.enter() != 0 {
                        let kind_v = v8_value_create_string(Some(&CefString::from(name.as_str())));
                        let data_v = v8_value_create_string(Some(&CefString::from(payload.as_str())));
                        let _ = cb.execute_function_with_context(
                            Some(&mut ctx),
                            None,
                            Some(&[kind_v, data_v]),
                        );
                        let _ = ctx.exit();
                        return 1;
                    }
                }
            }

            let js = if name == IPC_RESPONSE_MSG {
                format!("window.__ZENITH_BRIDGE__ && window.__ZENITH_BRIDGE__._recvResponse({payload});")
            } else {
                format!("window.__ZENITH_BRIDGE__ && window.__ZENITH_BRIDGE__._recvEvent({payload});")
            };
            frame.execute_java_script(
                Some(&CefString::from(js.as_str())),
                Some(&CefString::from("zenith://internal/dispatch.js")),
                1,
            );
            1
        }
    }
}

fn eval_v8(context: &mut V8Context, script: &str, name: &str) {
    let mut rv = None;
    let mut exc = None;
    let _ = context.eval(
        Some(&CefString::from(script)),
        Some(&CefString::from(name)),
        1,
        Some(&mut rv),
        Some(&mut exc),
    );
}

wrap_life_span_handler! {
    struct ExternalLifeSpanHandler;

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                super::track_browser(browser);
                if let Some(frame) = browser.main_frame() {
                    let url = cef_str(frame.url());
                    if !url.is_empty() && url != "about:blank" && !url.starts_with("devtools://") {
                        crate::extensions::bridge::adopt_external_url(url);
                        if let Some(host) = browser.host() {
                            host.close_browser(1);
                        }
                    }
                }
            }
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                super::untrack_browser(browser.identifier());
            }
        }
    }
}

wrap_display_handler! {
    struct ExternalDisplayHandler;

    impl DisplayHandler {
        fn on_address_change(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if frame.is_some_and(|f| f.is_main() == 0) {
                return;
            }
            let uri = opt_cef_str(url);
            if uri.is_empty() || uri == "about:blank" || uri.starts_with("devtools://") {
                return;
            }
            crate::extensions::bridge::adopt_external_url(uri);
            if let Some(host) = browser.and_then(|b| b.host()) {
                host.close_browser(1);
            }
        }
    }
}

wrap_client! {
    struct ExternalClient;

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(ExternalLifeSpanHandler::new())
        }
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(ExternalDisplayHandler::new())
        }
    }
}

wrap_browser_process_handler! {
    struct ZenithBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            // Suppress Chromium background component updates, DoH auto-probing, and Google SB pings.
            if let Some(ctx) = request_context_get_global_context() {
                if let Some(mut v_false) = value_create() {
                    v_false.set_bool(0);
                    let mut err = CefString::default();
                    for pref in [
                        "component_updates.component_updates_enabled",
                        "safebrowsing.enabled",
                        "safebrowsing.enhanced",
                        "alternate_error_pages.enabled",
                        "search.suggest_enabled",
                        "net.network_prediction_options",
                    ] {
                        let _ = ctx.set_preference(Some(&CefString::from(pref)), Some(&mut v_false), Some(&mut err));
                    }
                }
                if let Some(mut v_off) = value_create() {
                    v_off.set_string(Some(&CefString::from("off")));
                    let mut err = CefString::default();
                    let _ = ctx.set_preference(Some(&CefString::from("dns_over_https.mode")), Some(&mut v_off), Some(&mut err));
                }
            }
            let mut factory = ZenithSchemeFactory::new(ui_dist_dir());
            register_scheme_handler_factory(
                Some(&CefString::from(TRUSTED_SCHEME)),
                Some(&CefString::from(TRUSTED_HOST)),
                Some(&mut factory),
            );
            crate::pdf::scheme::register(ui_dist_dir());
            if let Some(label) = take_initial_window() {
                crate::app::create_initial_window(&label);
            }
        }

        fn default_client(&self) -> Option<Client> {
            // Adopt Chrome-created extension windows (tabs.create, welcome/uninstall) into Zenith.
            Some(ExternalClient::new())
        }

        fn default_request_context_handler(&self) -> Option<RequestContextHandler> {
            // Worker-aware request blocking for service workers & shared workers on the IO thread.
            Some(super::tab::create_request_context_handler())
        }
    }
}

wrap_app! {
    struct ZenithCefApp {
        render_state: Arc<Mutex<RenderState>>,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(cmd) = command_line else { return };
            // Privacy & engine hardening: suppress unsolicited vendor/background traffic and
            // external Chrome registry extension leakage (Phase 1 finding #1).
            for switch in [
                "disable-background-networking",
                "disable-component-update",
                "disable-component-extensions-with-background-pages",
                "disable-default-apps",
                "disable-domain-reliability",
                "disable-field-trial-config",
                "disable-sync",
                "metrics-recording-only",
                "no-first-run",
                "no-default-browser-check",
                "no-pings",
                "safebrowsing-disable-auto-update",
                "disable-external-intent-requests",
                "disable-client-side-phishing-detection",
                "disable-popup-blocking",
            ] {
                cmd.append_switch(Some(&CefString::from(switch)));
            }
            cmd.append_switch_with_value(
                Some(&CefString::from("component-updater")),
                Some(&CefString::from("disable")),
            );
            cmd.append_switch_with_value(
                Some(&CefString::from("disable-features")),
                Some(&CefString::from(
                    "AutofillServerCommunication,PasswordManagerOnboarding,MediaRouter,\
                     OptimizationHints,OptimizationGuideModelDownloading,OptimizationHintsFetching,\
                     Translate,NetworkTimeServiceQuerying,PrivacySandboxSettings4,\
                     CertificateTransparencyComponentUpdater,InterestFeedContentSuggestions,\
                     DnsOverHttps,AimServerEligibility,LensOverlay,ChromeWhatsNewUI",
                )),
            );
            // Browser process: attach the in-process browser-target CDP pipe and load enabled extensions.
            if opt_cef_str(process_type).is_empty() {
                if let Some(pipes) = super::browser_cdp_pipe_arg() {
                    cmd.append_switch(Some(&CefString::from("remote-debugging-pipe")));
                    cmd.append_switch_with_value(
                        Some(&CefString::from("remote-debugging-io-pipes")),
                        Some(&CefString::from(pipes)),
                    );
                }
                let ext_dirs = crate::extensions::enabled_extension_dirs();
                if !ext_dirs.is_empty() {
                    let joined = ext_dirs
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join(",");
                    cmd.append_switch_with_value(
                        Some(&CefString::from("load-extension")),
                        Some(&CefString::from(joined.as_str())),
                    );
                }
            }
        }

        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            let Some(reg) = registrar else { return };
            let flags = SchemeOptions::STANDARD.get_raw()
                | SchemeOptions::SECURE.get_raw()
                | SchemeOptions::DISPLAY_ISOLATED.get_raw()
                | SchemeOptions::FETCH_ENABLED.get_raw()
                | SchemeOptions::CORS_ENABLED.get_raw();
            reg.add_custom_scheme(Some(&CefString::from(TRUSTED_SCHEME)), flags as i32);
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(ZenithBrowserProcessHandler::new())
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(ZenithRenderHandler::new(self.render_state.clone()))
        }
    }
}

pub fn create_app() -> App {
    ZenithCefApp::new(Arc::new(Mutex::new(RenderState::default())))
}
