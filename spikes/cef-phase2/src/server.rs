//! Minimal std-only loopback HTTP server serving test pages and collecting extension probe reports.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtensionSignals {
    pub extension_id: Option<String>,
    pub mv3_worker_alive: bool,
    pub content_script_injected: bool,
    pub content_script_isolated: bool,
    pub action_badge_text: Option<String>,
    pub action_badge_color: Option<Vec<u32>>,
    pub action_badge_ok: bool,
    pub direct_tab_id: Option<i64>,
    pub direct_tab_loaded: bool,
    pub popup_ok: bool,
    pub popup_tabs_count: Option<usize>,
    pub options_ok: bool,
    pub native_messaging_ok: bool,
    pub native_messaging_error: Option<String>,
}

pub struct FixtureServer {
    pub addr: SocketAddr,
}

impl FixtureServer {
    pub fn start(signals: Arc<Mutex<ExtensionSignals>>) -> Result<Self, String> {
        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|e| format!("bind fixture server: {e}"))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("local_addr: {e}"))?;
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let sig = signals.clone();
                thread::spawn(move || handle_conn(stream, sig));
            }
        });
        Ok(Self { addr })
    }
}

fn handle_conn(mut stream: TcpStream, signals: Arc<Mutex<ExtensionSignals>>) {
    let mut buf = [0u8; 16384];
    let Ok(n) = stream.read(&mut buf) else {
        return;
    };
    let req = String::from_utf8_lossy(&buf[..n]);
    let first_line = req.lines().next().unwrap_or("");
    let path = first_line.split_whitespace().nth(1).unwrap_or("/");
    let body_raw = req.split("\r\n\r\n").nth(1).unwrap_or("");

    if path.starts_with("/report/page") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body_raw) {
            if let Ok(mut s) = signals.lock() {
                if v["injected"].as_bool() == Some(true) {
                    s.content_script_injected = true;
                }
                if v["isolated"].as_bool() == Some(true) {
                    s.content_script_isolated = true;
                }
            }
        }
        write_http(&mut stream, "200 OK", "application/json", r#"{"ok":true}"#);
        return;
    }

    if path.starts_with("/report/sw") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body_raw) {
            if let Ok(mut s) = signals.lock() {
                if let Some(id) = v["extensionId"].as_str() {
                    s.extension_id = Some(id.to_string());
                }
                if v["workerAlive"].as_bool() == Some(true) {
                    s.mv3_worker_alive = true;
                }
                if let Some(txt) = v["badgeText"].as_str() {
                    s.action_badge_text = Some(txt.to_string());
                }
                if let Some(arr) = v["badgeColor"].as_array() {
                    s.action_badge_color = Some(
                        arr.iter()
                            .filter_map(|x| x.as_u64().map(|n| n as u32))
                            .collect(),
                    );
                }
                if v["badgeOk"].as_bool() == Some(true) {
                    s.action_badge_ok = true;
                }
                if let Some(tid) = v["directTabId"].as_i64() {
                    s.direct_tab_id = Some(tid);
                }
                if v["nativeOk"].as_bool() == Some(true) {
                    s.native_messaging_ok = true;
                }
                if let Some(err) = v["nativeError"].as_str() {
                    s.native_messaging_error = Some(err.to_string());
                }
            }
        }
        write_http(&mut stream, "200 OK", "application/json", r#"{"ok":true}"#);
        return;
    }

    if path.starts_with("/report/popup") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body_raw) {
            if let Ok(mut s) = signals.lock() {
                s.popup_ok = v["ok"].as_bool().unwrap_or(false);
                s.popup_tabs_count = v["tabsCount"].as_u64().map(|n| n as usize);
            }
        }
        write_http(&mut stream, "200 OK", "application/json", r#"{"ok":true}"#);
        return;
    }

    if path.starts_with("/report/options") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body_raw) {
            if let Ok(mut s) = signals.lock() {
                s.options_ok = v["ok"].as_bool().unwrap_or(false);
            }
        }
        write_http(&mut stream, "200 OK", "application/json", r#"{"ok":true}"#);
        return;
    }

    if path.starts_with("/ext-created") {
        if let Ok(mut s) = signals.lock() {
            s.direct_tab_loaded = true;
        }
        write_http(
            &mut stream,
            "200 OK",
            "text/html",
            "<!doctype html><html><body><h1>Extension Created Tab OK</h1></body></html>",
        );
        return;
    }

    if path.starts_with("/pane/") {
        let html = format!(
            "<!doctype html><html><body style='background:#18181b;color:#e4e4e7;font-family:sans-serif'><h2>Pane {path}</h2></body></html>"
        );
        write_http(&mut stream, "200 OK", "text/html", &html);
        return;
    }

    // Default `/page`: checks isolated-world content script injection and reports back.
    let page_html = r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>Phase 2 Probe Page</title></head>
<body style="background:#111;color:#eee;font-family:sans-serif">
  <h1>Phase 2 Extension &amp; Child-Window Probe</h1>
  <script>
    (function probe() {
      const check = () => {
        const injected = document.documentElement.dataset.zenithExt === "injected";
        const isolated = typeof window.__EXT_ISOLATED_SECRET === "undefined";
        fetch("/report/page", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ injected, isolated })
        }).catch(() => {});
      };
      setTimeout(check, 250);
      setTimeout(check, 1200);
    })();
  </script>
</body>
</html>"#;
    write_http(&mut stream, "200 OK", "text/html", page_html);
}

fn write_http(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}
