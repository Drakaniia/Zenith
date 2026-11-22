//! Typed invoke/event protocol, local untrusted-page HTTP fixture, and Phase 1 probe HTML.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};

pub const IPC_INVOKE_MSG: &str = "zenith.invoke";
pub const IPC_RESPONSE_MSG: &str = "zenith.response";
pub const IPC_EVENT_MSG: &str = "zenith.event";

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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TrustedProbeSummary {
    pub trusted_invoke_ok: bool,
    pub engine_version: String,
    pub bad_token_rejected: bool,
    pub unknown_cmd_rejected: bool,
    pub backend_event_received: bool,
    pub iframe_bridge_undefined: bool,
    pub iframe_parent_blocked: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UntrustedProbeSummary {
    pub page_ran: bool,
    pub bridge_undefined: bool,
    pub native_send_undefined: bool,
    pub zenith_scheme_fetch_blocked: bool,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Phase1Report {
    pub cef_version: String,
    pub trusted: Option<TrustedProbeSummary>,
    pub untrusted: Option<UntrustedProbeSummary>,
    pub trusted_nav_to_untrusted_blocked: bool,
    pub crash_detected: bool,
    pub stale_generation_rejected: bool,
    pub crash_recovered: bool,
}

impl Phase1Report {
    pub fn all_passed(&self) -> bool {
        let trusted_ok = self.trusted.as_ref().is_some_and(|t| {
            t.trusted_invoke_ok
                && t.bad_token_rejected
                && t.unknown_cmd_rejected
                && t.backend_event_received
                && t.iframe_bridge_undefined
                && t.iframe_parent_blocked
        });
        let untrusted_ok = self.untrusted.as_ref().is_some_and(|u| {
            u.page_ran
                && u.bridge_undefined
                && u.native_send_undefined
                && u.zenith_scheme_fetch_blocked
        });
        trusted_ok
            && untrusted_ok
            && self.trusted_nav_to_untrusted_blocked
            && self.crash_detected
            && self.stale_generation_rejected
            && self.crash_recovered
    }
}

/// Tiny loopback HTTP server serving untrusted page, iframe probe, and post-crash recovery target.
pub struct FixtureServer {
    pub addr: SocketAddr,
}

impl FixtureServer {
    pub fn start(
        untrusted_slot: Arc<Mutex<Option<UntrustedProbeSummary>>>,
        recovered_slot: Arc<Mutex<bool>>,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("fixture server bind failed: {e}"))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("fixture local_addr failed: {e}"))?;

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                handle_http(stream, &untrusted_slot, &recovered_slot);
            }
        });

        Ok(Self { addr })
    }
}

fn handle_http(
    mut stream: TcpStream,
    untrusted_slot: &Arc<Mutex<Option<UntrustedProbeSummary>>>,
    recovered_slot: &Arc<Mutex<bool>>,
) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let mut raw = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let mut header_end = None;
    let mut content_len = 0usize;

    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&chunk[..n]);
                if header_end.is_none() {
                    if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = Some(pos + 4);
                        let hdr = String::from_utf8_lossy(&raw[..pos]);
                        for line in hdr.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.eq_ignore_ascii_case("content-length") {
                                    content_len = v.trim().parse::<usize>().unwrap_or(0);
                                }
                            }
                        }
                    }
                }
                if let Some(end) = header_end {
                    if raw.len() >= end + content_len {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    if raw.is_empty() {
        return;
    }
    let req = String::from_utf8_lossy(&raw);
    let first = req.lines().next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");

    match (method, path) {
        ("GET", "/untrusted") => write_response(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            UNTRUSTED_PAGE_HTML,
        ),
        ("GET", "/recovered") => {
            if let Ok(mut flag) = recovered_slot.lock() {
                *flag = true;
            }
            write_response(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                "<!doctype html><html><body><h1>Recovered</h1></body></html>",
            );
        }
        ("POST", "/untrusted-result") => {
            if let Some((_, body)) = req.split_once("\r\n\r\n") {
                if let Ok(parsed) = serde_json::from_str::<UntrustedProbeSummary>(body.trim()) {
                    if let Ok(mut slot) = untrusted_slot.lock() {
                        *slot = Some(parsed);
                    }
                }
            }
            write_response(&mut stream, "200 OK", "application/json", "{\"ok\":true}");
        }
        _ => write_response(&mut stream, "404 Not Found", "text/plain", "not found"),
    }
}

fn write_response(stream: &mut TcpStream, status: &str, mime: &str, body: &str) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body.as_bytes());
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
    } else if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".wasm") {
        "application/wasm"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else {
        "application/octet-stream"
    }
}

/// Built-in trusted UI harness that verifies typed invoke/events, token checks, and iframe isolation.
pub fn trusted_selftest_html() -> &'static str {
    r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>Zenith CEF Phase 1 Trusted UI</title></head>
<body style="font-family:system-ui;background:#111;color:#eee;padding:16px">
  <h1>Zenith CEF Phase 1 Spike</h1>
  <pre id="status">Running probes...</pre>
  <script>
    (async () => {
      const out = {
        trusted_invoke_ok: false,
        engine_version: "",
        bad_token_rejected: false,
        unknown_cmd_rejected: false,
        backend_event_received: false,
        iframe_bridge_undefined: false,
        iframe_parent_blocked: false
      };
      try {
        const iframeResultPromise = new Promise((resolve) => {
          window.addEventListener("message", (ev) => {
            if (ev.data && ev.data.kind === "iframe-probe") resolve(ev.data);
          });
          const f = document.createElement("iframe");
          f.setAttribute("sandbox", "allow-scripts");
          f.style.cssText = "width:1px;height:1px;border:0";
          f.srcdoc = `<!doctype html><html><body><script>
            let parentBlocked = false;
            try { parentBlocked = typeof window.parent.__ZENITH_BRIDGE__ === "undefined"; }
            catch (_) { parentBlocked = true; }
            window.parent.postMessage({
              kind: "iframe-probe",
              bridge_undefined: typeof window.__ZENITH_BRIDGE__ === "undefined"
                && typeof window.__zenith_native_send === "undefined",
              parent_blocked: parentBlocked
            }, "*");
          <\/script></body></html>`;
          document.body.appendChild(f);
        });

        const info = await window.__ZENITH_BRIDGE__.invoke("get_engine_info", {});
        out.trusted_invoke_ok = Boolean(info && info.engine === "cef" && info.webview2 === false);
        out.engine_version = String(info?.version || "");

        try {
          await window.__ZENITH_BRIDGE__.invokeWithToken("bad-token", "ping", {});
        } catch (e) {
          out.bad_token_rejected = String(e).includes("invalid_token");
        }

        try {
          await window.__ZENITH_BRIDGE__.invoke("not_allowed_cmd", {});
        } catch (e) {
          out.unknown_cmd_rejected = String(e).includes("unknown_command");
        }

        const eventPromise = new Promise((resolve) => {
          window.__ZENITH_BRIDGE__.listen("spike-ping", (payload) => {
            resolve(Boolean(payload && payload.seq === 42));
          });
        });
        await window.__ZENITH_BRIDGE__.invoke("trigger_event", { seq: 42 });
        out.backend_event_received = await eventPromise;

        const iframeResult = await iframeResultPromise;
        out.iframe_bridge_undefined = Boolean(iframeResult.bridge_undefined);
        out.iframe_parent_blocked = Boolean(iframeResult.parent_blocked);

        document.getElementById("status").textContent = JSON.stringify(out, null, 2);
        await window.__ZENITH_BRIDGE__.invoke("probe_complete", out);
      } catch (err) {
        document.getElementById("status").textContent = "ERROR: " + err;
      }
    })();
  </script>
</body>
</html>"#
}

const UNTRUSTED_PAGE_HTML: &str = r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>Untrusted Page</title></head>
<body>
  <h1>Untrusted Page</h1>
  <script>
    (async () => {
      let fetchBlocked = false;
      try {
        const resp = await fetch("zenith://app/index.html");
        fetchBlocked = !resp.ok;
      } catch (_) {
        fetchBlocked = true;
      }
      const report = {
        page_ran: true,
        bridge_undefined: typeof window.__ZENITH_BRIDGE__ === "undefined",
        native_send_undefined: typeof window.__zenith_native_send === "undefined",
        zenith_scheme_fetch_blocked: fetchBlocked
      };
      await fetch("/untrusted-result", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(report)
      });
    })();
  </script>
</body>
</html>"#;
