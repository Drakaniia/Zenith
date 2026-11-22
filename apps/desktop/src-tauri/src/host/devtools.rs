//! Native CEF DevTools protocol bridge (`execute_dev_tools_method` + `DevToolsMessageObserver`).
//! Used for `cdp_call`, `eval_js` with JSON return values, `Target.getTargetInfo`, screenshots,
//! responsive emulation, and opening DevTools windows without needing a remote debugging port.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};

use cef::*;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Pipes::CreatePipe;

use super::with_host;

type CdpDone = Box<dyn FnOnce(Result<String, String>)>;

thread_local! {
    static PENDING: RefCell<HashMap<i32, CdpDone>> = RefCell::new(HashMap::new());
    static BROWSER_PENDING: RefCell<HashMap<i32, CdpDone>> = RefCell::new(HashMap::new());
    /// Keeps each browser's `Registration` alive until the browser closes.
    static REGISTRATIONS: RefCell<HashMap<i32, Registration>> = RefCell::new(HashMap::new());
}

static NEXT_MSG_ID: AtomicI32 = AtomicI32::new(1);
static BROWSER_PIPE_ARG: OnceLock<String> = OnceLock::new();
static BROWSER_PIPE_TX: Mutex<Option<File>> = Mutex::new(None);

wrap_dev_tools_message_observer! {
    struct CdpObserver;

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: ::std::os::raw::c_int,
            success: ::std::os::raw::c_int,
            result: Option<&[u8]>,
        ) {
            let done = PENDING.with(|p| p.borrow_mut().remove(&message_id));
            let Some(done) = done else { return };
            let text = result
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_else(|| "{}".into());
            if success != 0 {
                done(Ok(text));
            } else {
                done(Err(text));
            }
        }
    }
}

pub fn attach_observer(browser: &Browser) {
    let id = browser.identifier();
    let exists = REGISTRATIONS.with(|r| r.borrow().contains_key(&id));
    if exists {
        return;
    }
    if let Some(host) = browser.host() {
        let mut obs = CdpObserver::new();
        if let Some(reg) = host.add_dev_tools_message_observer(Some(&mut obs)) {
            REGISTRATIONS.with(|r| {
                r.borrow_mut().insert(id, reg);
            });
        }
    }
}

pub fn detach_observer(browser_id: i32) {
    REGISTRATIONS.with(|r| {
        r.borrow_mut().remove(&browser_id);
    });
}

/// Calls a CDP method directly on a CEF browser's built-in DevTools agent.
pub fn cdp_call(
    browser: &Browser,
    method: &str,
    params_json: &str,
    done: impl FnOnce(Result<String, String>) + 'static,
) {
    attach_observer(browser);
    let Some(host) = browser.host() else {
        return done(Err("browser host unavailable".into()));
    };
    let msg_id = NEXT_MSG_ID.fetch_add(1, Ordering::Relaxed);
    let mut dict = json_to_dictionary(params_json);
    PENDING.with(|p| {
        p.borrow_mut().insert(msg_id, Box::new(done));
    });
    let sent = host.execute_dev_tools_method(
        msg_id,
        Some(&CefString::from(method)),
        dict.as_mut(),
    );
    if sent == 0 {
        if let Some(cb) = PENDING.with(|p| p.borrow_mut().remove(&msg_id)) {
            cb(Err(format!("execute_dev_tools_method failed for {method}")));
        }
    }
}

/// Evaluates JavaScript in a browser's main context via `Runtime.evaluate` and returns the
/// JSON-serialized result value.
pub fn eval_js(
    browser: &Browser,
    expression: &str,
    await_promise: bool,
    user_gesture: bool,
    done: impl FnOnce(Result<serde_json::Value, String>) + 'static,
) {
    let params = serde_json::json!({
        "expression": expression,
        "awaitPromise": await_promise,
        "userGesture": user_gesture,
        "returnByValue": true,
    })
    .to_string();
    cdp_call(browser, "Runtime.evaluate", &params, move |res| {
        let out = res.and_then(|json| {
            let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            if let Some(exc) = v.get("exceptionDetails") {
                return Err(exc["text"].as_str().unwrap_or("JS exception").to_string());
            }
            Ok(v["result"].get("value").cloned().unwrap_or(serde_json::Value::Null))
        });
        done(out);
    });
}

/// Calls a CDP method on the primary window's chrome browser.
pub fn chrome_cdp(method: &str, params: &str, done: impl FnOnce(Result<String, String>) + 'static) {
    let browser = with_host(|h| {
        h.wins
            .get(&h.target_owner)
            .or_else(|| h.wins.values().next())
            .and_then(|w| w.chrome.clone())
    })
    .flatten();
    match browser {
        Some(b) => cdp_call(&b, method, params, done),
        None => done(Err("browser is not ready".into())),
    }
}

pub(crate) fn target_id_of(json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json).ok()?["targetInfo"]["targetId"]
        .as_str()
        .map(str::to_string)
}

/// Opens a popup DevTools window inspecting `browser`.
pub fn open_devtools(browser: &Browser) -> Result<(), String> {
    let Some(host) = browser.host() else {
        return Err("browser host unavailable".into());
    };
    let info = WindowInfo::default().set_as_popup(cef::sys::HWND(std::ptr::null_mut()), "DevTools");
    let settings = BrowserSettings::default();
    let inspect_at = Point::default();
    host.show_dev_tools(Some(&info), None, Some(&settings), Some(&inspect_at));
    Ok(())
}

fn json_to_dictionary(params_json: &str) -> Option<DictionaryValue> {
    let trimmed = params_json.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return None;
    }
    let val = parse_json(Some(&CefString::from(trimmed)), JsonParserOptions::default())?;
    val.dictionary()
}

// ---------------------------------------------------------------------------------------------
// In-process browser-target CDP over anonymous Windows pipes (`--remote-debugging-io-pipes`).
// Reaches browser-only domains (`Extensions.loadUnpacked`/`uninstall`) without opening a port.

pub(crate) fn init_browser_cdp_pipes() {
    if BROWSER_PIPE_ARG.get().is_some() {
        return;
    }
    let (mut cef_read, mut rust_write) = (HANDLE::default(), HANDLE::default());
    let (mut rust_read, mut cef_write) = (HANDLE::default(), HANDLE::default());
    unsafe {
        if CreatePipe(&mut cef_read, &mut rust_write, None, 0).is_err() {
            return;
        }
        if CreatePipe(&mut rust_read, &mut cef_write, None, 0).is_err() {
            let _ = File::from_raw_handle(cef_read.0);
            let _ = File::from_raw_handle(rust_write.0);
            return;
        }
    }
    let tx = unsafe { File::from_raw_handle(rust_write.0) };
    let rx = unsafe { File::from_raw_handle(rust_read.0) };
    if let Ok(mut guard) = BROWSER_PIPE_TX.lock() {
        *guard = Some(tx);
    }
    // Chromium takes ownership of `cef_read` and `cef_write` when it opens the debugging pipe.
    let _ = BROWSER_PIPE_ARG.set(format!("{},{}", cef_read.0 as usize, cef_write.0 as usize));

    std::thread::Builder::new()
        .name("zenith-cdp-pipe".into())
        .spawn(move || read_browser_cdp_loop(rx))
        .ok();
}

pub(crate) fn browser_cdp_pipe_arg() -> Option<&'static str> {
    BROWSER_PIPE_ARG.get().map(String::as_str)
}

pub(crate) fn close_browser_cdp_pipes() {
    if let Ok(mut guard) = BROWSER_PIPE_TX.lock() {
        guard.take();
    }
}

fn parse_browser_cdp_reply(frame: &[u8]) -> Option<(i32, Result<String, String>)> {
    let v: serde_json::Value = serde_json::from_slice(frame).ok()?;
    let id = v.get("id")?.as_i64()? as i32;
    if let Some(err) = v.get("error") {
        Some((id, Err(err.to_string())))
    } else {
        Some((id, Ok(v.get("result").map(|r| r.to_string()).unwrap_or_else(|| "{}".into()))))
    }
}

fn read_browser_cdp_loop(rx: File) {
    let mut reader = BufReader::new(rx);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(0, &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if buf.last() == Some(&0) {
                    buf.pop();
                }
                if let Some((id, res)) = parse_browser_cdp_reply(&buf) {
                    let _ = crate::app::runtime::post_ui(move || {
                        if let Some(done) = BROWSER_PENDING.with(|p| p.borrow_mut().remove(&id)) {
                            done(res);
                        }
                    });
                }
            }
        }
    }
    let _ = crate::app::runtime::post_ui(|| {
        let pending: Vec<CdpDone> = BROWSER_PENDING.with(|p| p.borrow_mut().drain().map(|(_, cb)| cb).collect());
        for cb in pending {
            cb(Err("browser CDP pipe closed".into()));
        }
    });
}

/// Calls a browser-target CDP method over the in-process anonymous debugging pipe.
pub fn browser_cdp(method: &str, params_json: &str, done: impl FnOnce(Result<String, String>) + 'static) {
    let params: serde_json::Value = serde_json::from_str(params_json).unwrap_or_else(|_| serde_json::json!({}));
    let msg_id = NEXT_MSG_ID.fetch_add(1, Ordering::Relaxed);
    let mut payload = serde_json::json!({ "id": msg_id, "method": method, "params": params }).to_string();
    payload.push('\0');

    BROWSER_PENDING.with(|p| {
        p.borrow_mut().insert(msg_id, Box::new(done));
    });
    let write_res = BROWSER_PIPE_TX
        .lock()
        .map_err(|_| "browser CDP pipe lock poisoned".to_string())
        .and_then(|mut guard| match guard.as_mut() {
            Some(tx) => tx.write_all(payload.as_bytes()).and_then(|_| tx.flush()).map_err(|e| e.to_string()),
            None => Err("browser CDP pipe unavailable".into()),
        });
    if let Err(e) = write_res {
        if let Some(cb) = BROWSER_PENDING.with(|p| p.borrow_mut().remove(&msg_id)) {
            cb(Err(e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_browser_cdp_reply;

    #[test]
    fn browser_cdp_reply_parsing_handles_result_error_and_events() {
        assert_eq!(
            parse_browser_cdp_reply(br#"{"id":7,"result":{"id":"abc"}}"#),
            Some((7, Ok(r#"{"id":"abc"}"#.into())))
        );
        assert_eq!(
            parse_browser_cdp_reply(br#"{"id":8,"error":{"code":-32000,"message":"bad"}}"#),
            Some((8, Err(r#"{"code":-32000,"message":"bad"}"#.into())))
        );
        assert_eq!(parse_browser_cdp_reply(br#"{"method":"Target.targetCreated","params":{}}"#), None);
    }
}
