//! CEF App, BrowserProcessHandler, RenderProcessHandler, SchemeHandlerFactory, and Client handlers.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cef::wrapper::byte_read_handler::{ByteReadHandler, ByteStream};
use cef::wrapper::stream_resource_handler::StreamResourceHandler;
use cef::*;

use crate::bridge::{
    mime_for_path, trusted_selftest_html, EventEnvelope, InvokeRequest, InvokeResponse,
    Phase1Report, TrustedProbeSummary, UntrustedProbeSummary, IPC_EVENT_MSG, IPC_INVOKE_MSG,
    IPC_RESPONSE_MSG,
};
use crate::security::{
    authorize_invoke, is_trusted_origin, resolve_asset_path, BridgeToken, BrowserRole,
    GenerationTable, TRUSTED_HOST, TRUSTED_SCHEME,
};

pub const CEF_PINNED_VERSION: &str = "156.0.31+ge603d97+chromium-156.0.8078.100";

/// Shared browser-process state for the Phase 1 spike.
pub struct SpikeState {
    pub self_test: bool,
    pub ui_dir: Option<PathBuf>,
    pub fixture_port: u16,
    pub token: BridgeToken,
    pub table: Mutex<GenerationTable>,
    pub browsers: Mutex<HashMap<i32, Browser>>,
    pub trusted_browser_id: Mutex<Option<i32>>,
    pub untrusted_browser_id: Mutex<Option<i32>>,
    pub untrusted_gen_before_crash: Mutex<Option<u64>>,
    pub untrusted_slot: Arc<Mutex<Option<UntrustedProbeSummary>>>,
    pub recovered_slot: Arc<Mutex<bool>>,
    pub report: Mutex<Phase1Report>,
    pub phase2_triggered: Mutex<bool>,
    pub closing: Mutex<bool>,
}

impl SpikeState {
    pub fn new(
        self_test: bool,
        ui_dir: Option<PathBuf>,
        fixture_port: u16,
        token: BridgeToken,
        untrusted_slot: Arc<Mutex<Option<UntrustedProbeSummary>>>,
        recovered_slot: Arc<Mutex<bool>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            self_test,
            ui_dir,
            fixture_port,
            token,
            table: Mutex::new(GenerationTable::default()),
            browsers: Mutex::new(HashMap::new()),
            trusted_browser_id: Mutex::new(None),
            untrusted_browser_id: Mutex::new(None),
            untrusted_gen_before_crash: Mutex::new(None),
            untrusted_slot,
            recovered_slot,
            report: Mutex::new(Phase1Report {
                cef_version: CEF_PINNED_VERSION.to_string(),
                ..Default::default()
            }),
            phase2_triggered: Mutex::new(false),
            closing: Mutex::new(false),
        })
    }

    fn close_all_browsers(&self) {
        if let Ok(mut closing) = self.closing.lock() {
            if *closing {
                return;
            }
            *closing = true;
        }
        let list: Vec<Browser> = self
            .browsers
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        if list.is_empty() {
            quit_message_loop();
            return;
        }
        for b in list {
            if let Some(host) = b.host() {
                host.close_browser(1);
            }
        }
    }
}

fn cef_str_to_string(s: CefStringUserfree) -> String {
    CefString::from(&s).to_string()
}

fn opt_cef_str(s: Option<&CefString>) -> String {
    s.map(|v| v.to_string()).unwrap_or_default()
}

// ---- Scheme handler serving `zenith://app/` packaged assets or the self-test UI.

wrap_scheme_handler_factory! {
    struct ZenithSchemeFactory {
        state: Arc<SpikeState>,
    }

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = request.map(|r| cef_str_to_string(r.url())).unwrap_or_default();
            if !is_trusted_origin(&url) {
                return None;
            }

            let (mime, bytes) = if let Some(ui_dir) = &self.state.ui_dir {
                let path = resolve_asset_path(ui_dir, &url)?;
                let data = fs::read(&path).ok()?;
                let mime = mime_for_path(&path.to_string_lossy()).to_string();
                (mime, data)
            } else {
                (
                    "text/html".to_string(),
                    trusted_selftest_html().as_bytes().to_vec(),
                )
            };

            let mut reader = ByteReadHandler::new(Arc::new(Mutex::new(ByteStream::new(bytes))));
            let stream = stream_reader_create_for_handler(Some(&mut reader))?;
            Some(StreamResourceHandler::new_with_stream(mime, stream))
        }
    }
}

wrap_display_handler! {
    struct SpikeDisplayHandler;

    impl DisplayHandler {
        fn on_console_message(
            &self,
            browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let id = browser.map(|b| b.identifier()).unwrap_or(-1);
            eprintln!("[console id={id} L{line}] {}", opt_cef_str(message));
            0
        }
    }
}

// ---- Renderer process V8 bridge (injected ONLY into the trusted main frame).

wrap_v8_handler! {
    struct BridgeSendHandler {
        browser: Browser,
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
                .map(|v| cef_str_to_string(v.string_value()))
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
            let _ = &self.browser;
            1
        }
    }
}

wrap_render_process_handler! {
    struct SpikeRenderHandler {
        trusted_tokens: Arc<Mutex<HashMap<i32, String>>>,
    }

    impl RenderProcessHandler {
        fn on_browser_created(
            &self,
            browser: Option<&mut Browser>,
            extra_info: Option<&mut DictionaryValue>,
        ) {
            let (Some(browser), Some(info)) = (browser, extra_info) else {
                return;
            };
            let role = cef_str_to_string(info.string(Some(&CefString::from("zenith_role"))));
            if role != "trusted_ui" {
                return;
            }
            let token = cef_str_to_string(info.string(Some(&CefString::from("zenith_token"))));
            if !token.is_empty() {
                if let Ok(mut map) = self.trusted_tokens.lock() {
                    map.insert(browser.identifier(), token);
                }
            }
        }

        fn on_browser_destroyed(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                if let Ok(mut map) = self.trusted_tokens.lock() {
                    map.remove(&browser.identifier());
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
            if frame.is_main() == 0 {
                return;
            }
            let url = cef_str_to_string(frame.url());
            if !is_trusted_origin(&url) {
                return;
            }
            let token = {
                let Ok(map) = self.trusted_tokens.lock() else {
                    return;
                };
                match map.get(&browser.identifier()) {
                    Some(t) => t.clone(),
                    None => return,
                }
            };

            let Some(global) = context.global() else {
                return;
            };
            let mut handler = BridgeSendHandler::new(browser.clone(), frame.clone());
            let fn_name = CefString::from("__zenith_native_send");
            if let Some(mut func) = v8_value_create_function(Some(&fn_name), Some(&mut handler)) {
                global.set_value_bykey(Some(&fn_name), Some(&mut func), V8Propertyattribute::default());
            }

            // Install the frozen typed bridge synchronously before page scripts run.
            let token_json = serde_json::to_string(&token).unwrap_or_else(|_| "\"\"".to_string());
            let init_js = format!(
                r#"(function(TOK) {{
  const pending = new Map();
  const listeners = new Map();
  let nextId = 1;
  function send(token, cmd, args) {{
    return new Promise((resolve, reject) => {{
      const id = nextId++;
      pending.set(id, {{ resolve, reject }});
      window.__zenith_native_send(JSON.stringify({{ id, token, cmd, args: args ?? {{}} }}));
    }});
  }}
  const bridge = Object.freeze({{
    invoke(cmd, args) {{ return send(TOK, cmd, args); }},
    invokeWithToken(tok, cmd, args) {{ return send(tok, cmd, args); }},
    listen(event, cb) {{
      if (!listeners.has(event)) listeners.set(event, new Set());
      listeners.get(event).add(cb);
      return () => listeners.get(event)?.delete(cb);
    }},
    _recvResponse(resp) {{
      const slot = pending.get(resp.id);
      if (!slot) return;
      pending.delete(resp.id);
      if (resp.ok) slot.resolve(resp.data);
      else slot.reject(new Error(resp.error || "invoke failed"));
    }},
    _recvEvent(env) {{
      const set = listeners.get(env.event);
      if (!set) return;
      for (const cb of set) cb(env.payload);
    }}
  }});
  Object.defineProperty(window, "__ZENITH_BRIDGE__", {{
    value: bridge, writable: false, configurable: false
  }});
  // Minimal Tauri v2 internals shim so packaged Svelte UI calls can route through CEF.
  if (!window.__TAURI_INTERNALS__) {{
    Object.defineProperty(window, "__TAURI_INTERNALS__", {{
      value: Object.freeze({{
        invoke: (cmd, args) => bridge.invoke(cmd, args),
        metadata: {{ currentWindow: {{ label: "main" }}, currentWebview: {{ label: "main" }} }}
      }}),
      writable: false,
      configurable: false
    }});
  }}
}})({token_json});"#
            );
            let mut rv = None;
            let mut exc = None;
            let ok = context.eval(
                Some(&CefString::from(init_js.as_str())),
                Some(&CefString::from("zenith://internal/bridge.js")),
                1,
                Some(&mut rv),
                Some(&mut exc),
            );
            if ok == 0 {
                let msg = exc.map(|e| cef_str_to_string(e.message())).unwrap_or_default();
                eprintln!("[renderer] bridge eval failed: {msg}");
            }
        }

        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            let (Some(frame), Some(msg)) = (frame, message) else {
                return 0;
            };
            let name = cef_str_to_string(msg.name());
            let payload = msg
                .argument_list()
                .map(|a| cef_str_to_string(a.string(0)))
                .unwrap_or_default();

            let js = if name == IPC_RESPONSE_MSG {
                format!("window.__ZENITH_BRIDGE__ && window.__ZENITH_BRIDGE__._recvResponse({payload});")
            } else if name == IPC_EVENT_MSG {
                format!("window.__ZENITH_BRIDGE__ && window.__ZENITH_BRIDGE__._recvEvent({payload});")
            } else {
                return 0;
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

// ---- Browser client handlers (lifecycle, navigation fail-closed, crash recovery, IPC).

wrap_life_span_handler! {
    struct SpikeLifeSpanHandler {
        state: Arc<SpikeState>,
        role: BrowserRole,
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
            // Never allow popups to inherit trusted UI context.
            if matches!(self.role, BrowserRole::TrustedUi) {
                return 1;
            }
            0
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let id = browser.identifier();
            let gen = self
                .state
                .table
                .lock()
                .map(|mut t| t.register(id, self.role.clone()))
                .unwrap_or(0);
            if let Ok(mut map) = self.state.browsers.lock() {
                map.insert(id, browser.clone());
            }
            match self.role {
                BrowserRole::TrustedUi => {
                    if let Ok(mut slot) = self.state.trusted_browser_id.lock() {
                        *slot = Some(id);
                    }
                }
                BrowserRole::UntrustedPage { .. } => {
                    if let Ok(mut slot) = self.state.untrusted_browser_id.lock() {
                        *slot = Some(id);
                    }
                    if let Ok(mut slot) = self.state.untrusted_gen_before_crash.lock() {
                        *slot = Some(gen);
                    }
                }
            }
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                let id = browser.identifier();
                if let Ok(mut t) = self.state.table.lock() {
                    t.tombstone(id);
                }
                if let Ok(mut map) = self.state.browsers.lock() {
                    map.remove(&id);
                    if map.is_empty() {
                        quit_message_loop();
                    }
                }
            }
        }
    }
}

wrap_request_handler! {
    struct SpikeRequestHandler {
        state: Arc<SpikeState>,
        role: BrowserRole,
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
            if !matches!(self.role, BrowserRole::TrustedUi) {
                return 0;
            }
            let is_main = frame.is_some_and(|f| f.is_main() != 0);
            if !is_main {
                return 0;
            }
            let url = request.map(|r| cef_str_to_string(r.url())).unwrap_or_default();
            if !is_trusted_origin(&url) {
                if let Ok(mut rep) = self.state.report.lock() {
                    rep.trusted_nav_to_untrusted_blocked = true;
                }
                return 1; // Cancel navigation: trusted surface fails closed.
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
            let Some(browser) = browser else { return };
            let id = browser.identifier();
            let old_gen = self
                .state
                .untrusted_gen_before_crash
                .lock()
                .ok()
                .and_then(|g| *g)
                .unwrap_or(0);

            let stale_rejected = if let Ok(mut t) = self.state.table.lock() {
                let _ = t.bump_generation(id);
                !t.is_live_generation(id, old_gen)
            } else {
                false
            };

            if let Ok(mut rep) = self.state.report.lock() {
                rep.crash_detected = true;
                rep.stale_generation_rejected = stale_rejected;
            }

            if self.state.self_test && matches!(self.role, BrowserRole::UntrustedPage { .. }) {
                if let Some(frame) = browser.main_frame() {
                    let recovery_url = format!("http://127.0.0.1:{}/recovered", self.state.fixture_port);
                    frame.load_url(Some(&CefString::from(recovery_url.as_str())));
                }
            }
        }
    }
}

wrap_client! {
    struct SpikeClient {
        state: Arc<SpikeState>,
        role: BrowserRole,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(SpikeDisplayHandler::new())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(SpikeLifeSpanHandler::new(self.state.clone(), self.role.clone()))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(SpikeRequestHandler::new(self.state.clone(), self.role.clone()))
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
            if cef_str_to_string(msg.name()) != IPC_INVOKE_MSG {
                return 0;
            }

            let raw = msg
                .argument_list()
                .map(|a| cef_str_to_string(a.string(0)))
                .unwrap_or_default();
            let req: InvokeRequest = match serde_json::from_str(&raw) {
                Ok(r) => r,
                Err(_) => return 1,
            };

            let frame_url = cef_str_to_string(frame.url());
            let auth = {
                let Ok(table) = self.state.table.lock() else {
                    return 1;
                };
                authorize_invoke(
                    &table,
                    browser.identifier(),
                    frame.is_main() != 0,
                    &frame_url,
                    &self.state.token,
                    &req.token,
                    &req.cmd,
                )
            };

            let resp = match auth {
                Err(e) => InvokeResponse {
                    id: req.id,
                    ok: false,
                    data: None,
                    error: Some(e.code().to_string()),
                },
                Ok(()) => handle_trusted_command(&self.state, frame, req),
            };

            send_json_message(frame, IPC_RESPONSE_MSG, &resp);
            1
        }
    }
}

fn handle_trusted_command(
    state: &Arc<SpikeState>,
    frame: &mut Frame,
    req: InvokeRequest,
) -> InvokeResponse {
    match req.cmd.as_str() {
        "ping" => InvokeResponse {
            id: req.id,
            ok: true,
            data: Some(serde_json::json!({ "pong": true })),
            error: None,
        },
        "get_engine_info" => InvokeResponse {
            id: req.id,
            ok: true,
            data: Some(serde_json::json!({
                "engine": "cef",
                "version": CEF_PINNED_VERSION,
                "webview2": false
            })),
            error: None,
        },
        "echo" => InvokeResponse {
            id: req.id,
            ok: true,
            data: Some(req.args),
            error: None,
        },
        "trigger_event" => {
            let env = EventEnvelope {
                event: "spike-ping".to_string(),
                payload: req.args,
            };
            send_json_message(frame, IPC_EVENT_MSG, &env);
            InvokeResponse {
                id: req.id,
                ok: true,
                data: Some(serde_json::json!({ "dispatched": true })),
                error: None,
            }
        }
        "probe_complete" => {
            if let Ok(summary) = serde_json::from_value::<TrustedProbeSummary>(req.args) {
                if let Ok(mut rep) = state.report.lock() {
                    rep.trusted = Some(summary);
                }
            }
            InvokeResponse {
                id: req.id,
                ok: true,
                data: Some(serde_json::json!({ "recorded": true })),
                error: None,
            }
        }
        _ => InvokeResponse {
            id: req.id,
            ok: false,
            data: None,
            error: Some("unknown_command".to_string()),
        },
    }
}

fn send_json_message<T: serde::Serialize>(frame: &Frame, msg_name: &str, payload: &T) {
    let Ok(json) = serde_json::to_string(payload) else {
        return;
    };
    if let Some(mut msg) = process_message_create(Some(&CefString::from(msg_name))) {
        if let Some(args) = msg.argument_list() {
            args.set_string(0, Some(&CefString::from(json.as_str())));
        }
        frame.send_process_message(ProcessId::RENDERER, Some(&mut msg));
    }
}

// ---- Self-test driver task on CEF UI thread.

wrap_task! {
    struct SelfTestTickTask {
        state: Arc<SpikeState>,
        ticks_left: u32,
    }

    impl Task {
        fn execute(&self) {
            // Sync untrusted and recovery slots into the report.
            if let Ok(u) = self.state.untrusted_slot.lock() {
                if u.is_some() {
                    if let Ok(mut rep) = self.state.report.lock() {
                        rep.untrusted = u.clone();
                    }
                }
            }
            if let Ok(rec) = self.state.recovered_slot.lock() {
                if *rec {
                    if let Ok(mut rep) = self.state.report.lock() {
                        rep.crash_recovered = true;
                    }
                }
            }

            let ready_for_phase2 = self
                .state
                .report
                .lock()
                .map(|r| r.trusted.is_some() && r.untrusted.is_some())
                .unwrap_or(false);

            if ready_for_phase2 {
                let should_trigger = self
                    .state
                    .phase2_triggered
                    .lock()
                    .map(|mut f| {
                        let first = !*f;
                        *f = true;
                        first
                    })
                    .unwrap_or(false);

                if should_trigger {
                    // 1) Try navigating trusted UI to untrusted HTTP origin (must be blocked).
                    if let Some(tid) = self.state.trusted_browser_id.lock().ok().and_then(|v| *v) {
                        if let Some(b) = self.state.browsers.lock().ok().and_then(|m| m.get(&tid).cloned()) {
                            if let Some(frame) = b.main_frame() {
                                let bad_url = format!("http://127.0.0.1:{}/untrusted", self.state.fixture_port);
                                frame.load_url(Some(&CefString::from(bad_url.as_str())));
                            }
                        }
                    }
                    // 2) Crash untrusted renderer to prove termination detection + recovery.
                    if let Some(uid) = self.state.untrusted_browser_id.lock().ok().and_then(|v| *v) {
                        if let Some(b) = self.state.browsers.lock().ok().and_then(|m| m.get(&uid).cloned()) {
                            if let Some(frame) = b.main_frame() {
                                frame.load_url(Some(&CefString::from("chrome://crash")));
                            }
                        }
                    }
                }
            }

            let done = self.state.report.lock().map(|r| r.all_passed()).unwrap_or(false);
            if done || self.ticks_left == 0 {
                self.state.close_all_browsers();
                return;
            }

            let mut next = SelfTestTickTask::new(self.state.clone(), self.ticks_left - 1);
            post_delayed_task(ThreadId::UI, Some(&mut next), 250);
        }
    }
}

// ---- Browser process handler + App.

wrap_browser_process_handler! {
    struct SpikeBrowserProcessHandler {
        state: Arc<SpikeState>,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            let mut factory = ZenithSchemeFactory::new(self.state.clone());
            register_scheme_handler_factory(
                Some(&CefString::from(TRUSTED_SCHEME)),
                Some(&CefString::from(TRUSTED_HOST)),
                Some(&mut factory),
            );

            // Launch Trusted UI browser with extra_info carrying its role and per-launch token.
            let trusted_window = WindowInfo::default().set_as_popup(
                cef::sys::HWND(std::ptr::null_mut()),
                "Zenith CEF Phase 1 - Trusted UI",
            );
            let mut trusted_client = SpikeClient::new(self.state.clone(), BrowserRole::TrustedUi);
            let mut extra = dictionary_value_create().expect("extra_info dictionary");
            extra.set_string(
                Some(&CefString::from("zenith_role")),
                Some(&CefString::from("trusted_ui")),
            );
            extra.set_string(
                Some(&CefString::from("zenith_token")),
                Some(&CefString::from(self.state.token.as_str())),
            );
            let browser_settings = BrowserSettings::default();
            browser_host_create_browser(
                Some(&trusted_window),
                Some(&mut trusted_client),
                Some(&CefString::from(crate::security::TRUSTED_BASE_URL)),
                Some(&browser_settings),
                Some(&mut extra),
                None,
            );

            // Launch isolated untrusted page browser with NO extra_info and NO privileged token.
            if self.state.self_test {
                let page_window = WindowInfo::default().set_as_popup(
                    cef::sys::HWND(std::ptr::null_mut()),
                    "Zenith CEF Phase 1 - Untrusted Page",
                );
                let mut page_client = SpikeClient::new(
                    self.state.clone(),
                    BrowserRole::UntrustedPage {
                        tab_id: "spike-tab-1".to_string(),
                    },
                );
                let page_url = format!("http://127.0.0.1:{}/untrusted", self.state.fixture_port);
                browser_host_create_browser(
                    Some(&page_window),
                    Some(&mut page_client),
                    Some(&CefString::from(page_url.as_str())),
                    Some(&browser_settings),
                    None,
                    None,
                );

                // Poll up to 30 seconds (120 * 250ms) for all automated assertions to complete.
                let mut tick = SelfTestTickTask::new(self.state.clone(), 120);
                post_delayed_task(ThreadId::UI, Some(&mut tick), 250);
            }
        }
    }
}

wrap_app! {
    pub struct SpikeApp {
        state: Option<Arc<SpikeState>>,
        trusted_tokens: Arc<Mutex<HashMap<i32, String>>>,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            if opt_cef_str(process_type).is_empty() {
                if let Some(cmd) = command_line {
                    // Disable unsolicited Chromium background services per privacy spec.
                    cmd.append_switch(Some(&CefString::from("disable-background-networking")));
                    cmd.append_switch(Some(&CefString::from("disable-component-update")));
                    cmd.append_switch(Some(&CefString::from("disable-sync")));
                    cmd.append_switch(Some(&CefString::from("metrics-recording-only")));
                }
            }
        }

        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            if let Some(reg) = registrar {
                let opts = (cef::sys::cef_scheme_options_t::CEF_SCHEME_OPTION_STANDARD as i32)
                    | (cef::sys::cef_scheme_options_t::CEF_SCHEME_OPTION_SECURE as i32)
                    | (cef::sys::cef_scheme_options_t::CEF_SCHEME_OPTION_DISPLAY_ISOLATED as i32)
                    | (cef::sys::cef_scheme_options_t::CEF_SCHEME_OPTION_FETCH_ENABLED as i32);
                reg.add_custom_scheme(Some(&CefString::from(TRUSTED_SCHEME)), opts);
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            self.state
                .as_ref()
                .map(|s| SpikeBrowserProcessHandler::new(s.clone()))
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(SpikeRenderHandler::new(self.trusted_tokens.clone()))
        }
    }
}
