//! Trusted UI custom scheme handler (`zenith://app/`), authenticated V8 bridge shim
//! (`__ZENITH_BRIDGE__` + `@tauri-apps/api` compatibility), and event emission.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use cef::wrapper::byte_read_handler::{ByteReadHandler, ByteStream};
use cef::wrapper::stream_resource_handler::StreamResourceHandler;
use cef::*;
use serde::{Deserialize, Serialize};

use super::runtime::{post_ui, AppHandle, WindowHandle};
use super::security::{
    authorize_invoke, is_trusted_origin, resolve_asset_path, BridgeToken, GenerationTable, TRUSTED_BASE_URL,
};

pub const IPC_INVOKE_MSG: &str = "zenith.invoke";
pub const IPC_RESPONSE_MSG: &str = "zenith.response";
pub const IPC_EVENT_MSG: &str = "zenith.event";

static BRIDGE_TOKEN: OnceLock<BridgeToken> = OnceLock::new();
pub static GENERATIONS: LazyLock<Mutex<GenerationTable>> = LazyLock::new(Default::default);

pub fn bridge_token() -> &'static BridgeToken {
    BRIDGE_TOKEN.get_or_init(|| BridgeToken::generate().expect("bridge token entropy"))
}

pub fn trusted_ui_url() -> String {
    #[cfg(debug_assertions)]
    if let Ok(dev) = std::env::var("ZENITH_DEV_URL") {
        let t = dev.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    TRUSTED_BASE_URL.to_string()
}

/// Locates the built Svelte `dist` directory for `zenith://app/`.
pub fn ui_dist_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("ZENITH_UI_DIR") {
        let p = PathBuf::from(custom);
        if p.join("index.html").is_file() {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let packaged = dir.join("ui");
            if packaged.join("index.html").is_file() {
                return packaged;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("dist")
}

pub fn mime_for_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") {
        "text/html"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript"
    } else if lower.ends_with(".css") {
        "text/css"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".wasm") {
        "application/wasm"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else if lower.ends_with(".woff") {
        "font/woff"
    } else if lower.ends_with(".ttf") {
        "font/ttf"
    } else {
        "application/octet-stream"
    }
}

wrap_scheme_handler_factory! {
    pub struct ZenithSchemeFactory {
        ui_dir: PathBuf,
    }

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = request
                .map(|r| CefString::from(&r.url()).to_string())
                .unwrap_or_default();
            if !is_trusted_origin(&url) {
                return None;
            }
            let path = resolve_asset_path(&self.ui_dir, &url)?;
            let bytes = fs::read(&path).ok()?;
            let mime = mime_for_path(&path.to_string_lossy()).to_string();
            let mut reader = ByteReadHandler::new(Arc::new(Mutex::new(ByteStream::new(bytes))));
            let stream = stream_reader_create_for_handler(Some(&mut reader))?;
            Some(StreamResourceHandler::new_with_stream(mime, stream))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct InvokeRequest {
    pub id: u64,
    pub token: String,
    pub cmd: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct InvokeResponse {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EventEnvelope {
    pub event: String,
    pub payload: serde_json::Value,
}

/// V8 init script installed ONLY into `TrustedUi` main frames. Exposes both `__ZENITH_BRIDGE__`
/// and the exact `@tauri-apps/api` v2 internals surface (`invoke`, `transformCallback`,
/// `metadata`, event plugin unregistration, and `data-tauri-drag-region`).
pub fn bridge_init_script(token: &str, window_label: &str, incognito: bool) -> String {
    let tok_json = serde_json::to_string(token).unwrap_or_else(|_| "\"\"".into());
    let label_json = serde_json::to_string(window_label).unwrap_or_else(|_| "\"main\"".into());
    let incog_line = if incognito { "window.__ZENITH_INCOGNITO__ = true;" } else { "" };
    format!(
        r#"(function(TOK, LABEL) {{
  {incog_line}
  const pending = new Map();
  const callbacks = new Map();
  const eventListeners = new Map();
  let nextId = 1;
  let nextCbId = 1;
  let nextEvId = 1;

  function rawInvoke(cmd, args) {{
    return new Promise((resolve, reject) => {{
      const id = nextId++;
      pending.set(id, {{ resolve, reject }});
      window.__zenith_native_send(JSON.stringify({{ id, token: TOK, cmd, args: args ?? {{}} }}));
    }});
  }}

  function transformCallback(callback, once) {{
    const id = nextCbId++;
    callbacks.set(id, {{ fn: callback, once: !!once }});
    return id;
  }}

  function invoke(cmd, args) {{
    const a = args ?? {{}};
    if (cmd === "plugin:event|listen") {{
      const event = a.event;
      const handlerId = a.handler;
      const evId = nextEvId++;
      if (!eventListeners.has(event)) eventListeners.set(event, new Map());
      eventListeners.get(event).set(evId, handlerId);
      return Promise.resolve(evId);
    }}
    if (cmd === "plugin:event|unlisten") {{
      const set = eventListeners.get(a.event);
      if (set) {{
        const cbId = set.get(a.eventId);
        set.delete(a.eventId);
        if (cbId !== undefined) callbacks.delete(cbId);
      }}
      return Promise.resolve();
    }}
    if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") {{
      return rawInvoke(cmd, a);
    }}
    return rawInvoke(cmd, a);
  }}

  const bridge = Object.freeze({{
    invoke,
    listen(event, cb) {{
      const cbId = transformCallback((ev) => cb(ev.payload), false);
      const evId = nextEvId++;
      if (!eventListeners.has(event)) eventListeners.set(event, new Map());
      eventListeners.get(event).set(evId, cbId);
      return () => {{
        eventListeners.get(event)?.delete(evId);
        callbacks.delete(cbId);
      }};
    }},
    _recvResponse(resp) {{
      const slot = pending.get(resp.id);
      if (!slot) return;
      pending.delete(resp.id);
      if (resp.ok) slot.resolve(resp.data);
      else slot.reject(resp.error ?? "invoke failed");
    }},
    _recvEvent(env) {{
      const map = eventListeners.get(env.event);
      if (!map) return;
      for (const [evId, cbId] of [...map.entries()]) {{
        const entry = callbacks.get(cbId);
        if (!entry) continue;
        if (entry.once) {{
          callbacks.delete(cbId);
          map.delete(evId);
        }}
        try {{
          entry.fn({{ event: env.event, id: evId, payload: env.payload }});
        }} catch (e) {{
          console.error("[zenith-bridge] event handler error", e);
        }}
      }}
    }},
  }});

  Object.defineProperty(window, "__ZENITH_BRIDGE__", {{
    value: bridge, writable: false, configurable: false,
  }});
  Object.defineProperty(window, "__TAURI_INTERNALS__", {{
    value: Object.freeze({{
      invoke,
      transformCallback,
      convertFileSrc: (p) => p,
      metadata: {{
        currentWindow: {{ label: LABEL }},
        currentWebview: {{ label: LABEL, windowLabel: LABEL }},
      }},
    }}),
    writable: false, configurable: false,
  }});
  Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", {{
    value: Object.freeze({{
      unregisterListener(event, eventId) {{
        const map = eventListeners.get(event);
        if (!map) return;
        const cbId = map.get(eventId);
        map.delete(eventId);
        if (cbId !== undefined) callbacks.delete(cbId);
      }},
    }}),
    writable: false, configurable: false,
  }});

  // Native window drag / double-click maximize for elements marked `data-tauri-drag-region`.
  document.addEventListener("mousedown", (e) => {{
    if (e.button !== 0 || e.buttons !== 1) return;
    let el = e.target;
    if (!(el instanceof Element)) return;
    if (!el.hasAttribute("data-tauri-drag-region")) {{
      const deep = el.closest('[data-tauri-drag-region="deep"]');
      if (!deep) return;
      if (el.closest('button, a, input, textarea, select, [role="button"], [contenteditable="true"]')) return;
    }}
    if (e.detail === 2) {{
      void invoke("plugin:window|internal_toggle_maximize", {{ label: LABEL }});
    }} else {{
      void invoke("plugin:window|start_dragging", {{ label: LABEL }});
    }}
  }}, true);
}})({tok_json}, {label_json});"#
    )
}

pub fn send_json_message<T: Serialize>(frame: &Frame, msg_name: &str, payload: &T) {
    let Ok(json) = serde_json::to_string(payload) else { return };
    if let Some(mut msg) = process_message_create(Some(&CefString::from(msg_name))) {
        if let Some(args) = msg.argument_list() {
            args.set_string(0, Some(&CefString::from(json.as_str())));
        }
        frame.send_process_message(ProcessId::RENDERER, Some(&mut msg));
    }
}

pub fn send_event_to_browser(browser: &Browser, event: &str, payload: serde_json::Value) {
    if let Some(frame) = browser.main_frame() {
        send_json_message(&frame, IPC_EVENT_MSG, &EventEnvelope { event: event.to_string(), payload });
    }
}

/// Authorizes and dispatches a renderer `zenith.invoke` message on the CEF UI thread.
pub fn handle_invoke_message(browser: &mut Browser, frame: &mut Frame, raw: &str) {
    let req: InvokeRequest = match serde_json::from_str(raw) {
        Ok(r) => r,
        Err(_) => return,
    };
    let frame_url = CefString::from(&frame.url()).to_string();
    let window_label = {
        let Ok(table) = GENERATIONS.lock() else { return };
        match authorize_invoke(
            &table,
            browser.identifier(),
            frame.is_main() != 0,
            &frame_url,
            bridge_token(),
            &req.token,
        ) {
            Ok(label) => label.to_string(),
            Err(e) => {
                send_json_message(
                    frame,
                    IPC_RESPONSE_MSG,
                    &InvokeResponse { id: req.id, ok: false, data: None, error: Some(e.code().to_string()) },
                );
                return;
            }
        }
    };

    let app = AppHandle;
    let Some(win) = app.get_window(&window_label) else {
        send_json_message(
            frame,
            IPC_RESPONSE_MSG,
            &InvokeResponse { id: req.id, ok: false, data: None, error: Some("window not found".into()) },
        );
        return;
    };

    if req.cmd.starts_with("plugin:") {
        let result = handle_plugin_command(&app, &win, &req.cmd, &req.args);
        let resp = match result {
            Ok(data) => InvokeResponse { id: req.id, ok: true, data: Some(data), error: None },
            Err(e) => InvokeResponse { id: req.id, ok: false, data: None, error: Some(e) },
        };
        send_json_message(frame, IPC_RESPONSE_MSG, &resp);
        return;
    }

    let browser_id = browser.identifier();
    let req_id = req.id;
    let cmd = req.cmd;
    let args = req.args;
    super::runtime::spawn(async move {
        let result = super::handlers::dispatch(app, win, &cmd, args).await;
        let resp = match result {
            Ok(data) => InvokeResponse { id: req_id, ok: true, data: Some(data), error: None },
            Err(e) => InvokeResponse { id: req_id, ok: false, data: None, error: Some(e) },
        };
        let _ = post_ui(move || {
            crate::host::send_chrome_response(browser_id, resp);
        });
    });
}

fn handle_plugin_command(
    app: &AppHandle,
    caller: &WindowHandle,
    cmd: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let target = args
        .get("label")
        .and_then(|v| v.as_str())
        .and_then(|l| app.get_window(l))
        .unwrap_or_else(|| caller.clone());
    match cmd {
        "plugin:window|minimize" => {
            target.minimize()?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|unminimize" => {
            target.unminimize()?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|maximize" => {
            target.maximize()?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|unmaximize" => {
            target.unmaximize()?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|toggle_maximize" | "plugin:window|internal_toggle_maximize" => {
            target.toggle_maximize()?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|is_maximized" => Ok(serde_json::json!(target.is_maximized())),
        "plugin:window|is_minimized" => Ok(serde_json::json!(target.is_minimized())),
        "plugin:window|is_focused" => Ok(serde_json::json!(target.is_focused())),
        "plugin:window|is_fullscreen" => Ok(serde_json::json!(target.is_fullscreen())),
        "plugin:window|set_fullscreen" => {
            let on = args.get("value").and_then(|v| v.as_bool()).unwrap_or(false);
            target.set_fullscreen(on)?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|show" => {
            target.show()?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|hide" => {
            target.hide()?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|set_focus" => {
            target.set_focus()?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|start_dragging" => {
            target.start_dragging()?;
            Ok(serde_json::Value::Null)
        }
        "plugin:window|close" => {
            super::on_window_close_requested(target.label());
            Ok(serde_json::Value::Null)
        }
        "plugin:window|destroy" => {
            super::close_window(app, target.label());
            Ok(serde_json::Value::Null)
        }
        "plugin:window|get_all_windows" => {
            let labels: Vec<String> = app.windows().keys().cloned().collect();
            Ok(serde_json::json!(labels))
        }
        "plugin:event|emit" => {
            if let Some(ev) = args.get("event").and_then(|v| v.as_str()) {
                let payload = args.get("payload").cloned().unwrap_or(serde_json::Value::Null);
                crate::host::emit(ev, payload);
            }
            Ok(serde_json::Value::Null)
        }
        "plugin:event|emit_to" => {
            if let (Some(label), Some(ev)) = (
                args.pointer("/target/label").and_then(|v| v.as_str()),
                args.get("event").and_then(|v| v.as_str()),
            ) {
                let payload = args.get("payload").cloned().unwrap_or(serde_json::Value::Null);
                crate::host::emit_to_window(label, ev, payload);
            }
            Ok(serde_json::Value::Null)
        }
        _ => Err(format!("unknown plugin command {cmd}")),
    }
}
