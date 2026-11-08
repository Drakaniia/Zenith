//! DevTools protocol calls on Zenith's own webviews, and adopting pages WebView2 opened outside
//! any Zenith view (stray targets).

use std::cell::RefCell;
use std::result::Result;

use tauri::Manager;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{HSTRING, PWSTR};

use super::{app, emit_to_window, primary_window, with_host};

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
        // Extensions belong to the user's profile, so their pages open in a normal window.
        emit_to_window(&primary_window(), "tab-open-request", serde_json::json!({ "url": url, "openerId": null, "kind": "foreground" }));
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

pub(super) fn watch_page_targets(core: &ICoreWebView2) -> Option<ICoreWebView2DevToolsProtocolEventReceiver> {
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
