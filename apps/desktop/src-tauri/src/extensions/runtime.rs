//! Live Chrome extension runtime bridge over CEF's in-process browser CDP pipe.
//!
//! - Auto-attaches to `chrome-extension://` MV3 service workers before their first line executes
//!   so `chrome.tabs.query({ windowType: "normal" })` and `chrome.windows.*` work seamlessly with
//!   CEF's child-window `RuntimeStyle::CHROME` tabs.
//! - Hooks `chrome.action.setBadgeText` / `setBadgeBackgroundColor` / `setBadgeTextColor` via
//!   `Runtime.addBinding` to stream real-time action badges into Zenith's UI.
//! - Handles `trigger_action`: focuses the active Chrome tab (`Page.bringToFront`), checks live
//!   `chrome.action.getPopup`, and dispatches `chrome.action.onClicked` for popup-less extensions.

use std::cell::RefCell;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::bridge::ActionBadge;
use super::{info_for, load_registry};
use crate::host::{browser_cdp, browser_cdp_session, cdp_call, set_browser_cdp_event_handler, with_host};

const BADGE_BINDING: &str = "__zenithExtBadge";

thread_local! {
    /// Active CDP sessions for running `chrome-extension://` service workers (`ext_id -> session_id`).
    static SW_SESSIONS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// Reverse index (`session_id -> ext_id`) for fast cleanup on detach.
    static SESSION_EXT: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ActionOutcome {
    Popup { url: String },
    Dispatched,
    Fallback,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BadgePayload {
    ext_id: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    background_color: Option<String>,
    #[serde(default)]
    text_color: Option<String>,
}

/// Shared compatibility shim injected into every `chrome-extension://` service worker.
/// Normalizes CEF child `windowType: "popup"` -> `"normal"` and streams `chrome.action` badges.
const EXT_SW_SCRIPT: &str = r#"(() => {
  const fmtColor = (c) => {
    if (!c) return null;
    if (typeof c === 'string') return c || null;
    if (Array.isArray(c) && c.length >= 3) {
      const a = c.length >= 4 ? c[3] : 255;
      if (a === 0) return null;
      return a === 255 ? `rgb(${c[0]},${c[1]},${c[2]})` : `rgba(${c[0]},${c[1]},${c[2]},${(a / 255).toFixed(3)})`;
    }
    return null;
  };
  const emitBadge = () => {
    const action = globalThis.chrome?.action || globalThis.chrome?.browserAction;
    const id = globalThis.chrome?.runtime?.id || '';
    if (!action || !id || typeof globalThis.__zenithExtBadge !== 'function') return;
    Promise.all([
      action.getBadgeText ? Promise.resolve(action.getBadgeText({})).catch(() => '') : '',
      action.getBadgeBackgroundColor ? Promise.resolve(action.getBadgeBackgroundColor({})).catch(() => null) : null,
      action.getBadgeTextColor ? Promise.resolve(action.getBadgeTextColor({})).catch(() => null) : null,
    ]).then(([text, bg, fg]) => {
      try {
        globalThis.__zenithExtBadge(JSON.stringify({
          extId: id,
          text: String(text || ''),
          backgroundColor: fmtColor(bg),
          textColor: fmtColor(fg),
        }));
      } catch {}
    }).catch(() => {});
  };
  const install = (attempt = 0) => {
    if (globalThis.__zenithExtCompat) return;
    const c = globalThis.chrome;
    if (!c || !c.runtime?.id || (!c.tabs?.query && !c.action && !c.browserAction)) {
      if (attempt < 40) setTimeout(() => install(attempt + 1), 25);
      return;
    }
    globalThis.__zenithExtCompat = true;
    const normWin = (w) => (w && w.type === 'popup' ? { ...w, type: 'normal' } : w);
    const fixWinOpts = (o) => {
      if (!o || !Array.isArray(o.windowTypes)) return o;
      if (o.windowTypes.includes('normal') && !o.windowTypes.includes('popup')) {
        return { ...o, windowTypes: [...o.windowTypes, 'popup'] };
      }
      return o;
    };
    const tabs = c.tabs;
    if (tabs && typeof tabs.query === 'function') {
      const origQ = tabs.query.bind(tabs);
      tabs.query = function (q, cb) {
        let nq = q;
        if (q && q.windowType === 'normal') {
          nq = { ...q };
          delete nq.windowType;
        }
        if (typeof cb === 'function') return origQ(nq || {}, cb);
        return origQ(nq || {});
      };
    }
    const wins = c.windows;
    if (wins) {
      for (const k of ['getLastFocused', 'getCurrent', 'get']) {
        if (typeof wins[k] !== 'function') continue;
        const orig = wins[k].bind(wins);
        wins[k] = function (...args) {
          const cb = typeof args[args.length - 1] === 'function' ? args.pop() : null;
          if (k === 'get') { if (args[1]) args[1] = fixWinOpts(args[1]); }
          else if (args[0]) args[0] = fixWinOpts(args[0]);
          if (cb) return orig(...args, (w) => cb(normWin(w)));
          return Promise.resolve(orig(...args)).then(normWin);
        };
      }
      if (typeof wins.getAll === 'function') {
        const origAll = wins.getAll.bind(wins);
        wins.getAll = function (...args) {
          const cb = typeof args[args.length - 1] === 'function' ? args.pop() : null;
          if (args[0]) args[0] = fixWinOpts(args[0]);
          if (cb) return origAll(...args, (ws) => cb((ws || []).map(normWin)));
          return Promise.resolve(origAll(...args)).then((ws) => (ws || []).map(normWin));
        };
      }
    }
    const action = c.action || c.browserAction;
    if (action) {
      for (const k of ['setBadgeText', 'setBadgeBackgroundColor', 'setBadgeTextColor']) {
        if (typeof action[k] !== 'function') continue;
        const orig = action[k].bind(action);
        action[k] = function (...args) {
          const r = orig(...args);
          if (r && typeof r.then === 'function') r.then(emitBadge, () => {});
          else setTimeout(emitBadge, 20);
          return r;
        };
      }
      emitBadge();
      setTimeout(emitBadge, 200);
    }
  };
  install(0);
})();"#;

fn ext_id_from_url(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("chrome-extension://")?;
    let id = rest.split('/').next()?;
    if id.len() == 32 && id.bytes().all(|b| b.is_ascii_lowercase()) {
        Some(id)
    } else {
        None
    }
}

fn inject_sw(sid: &str) {
    let eval_params = serde_json::json!({ "expression": EXT_SW_SCRIPT }).to_string();
    browser_cdp_session(sid, "Runtime.evaluate", &eval_params, |r| {
        if let Err(e) = r {
            eprintln!("[zenith] extension SW inject failed: {e}");
        }
    });
}

fn attach_target(target_id: &str) {
    let attach = serde_json::json!({ "targetId": target_id, "flatten": true }).to_string();
    browser_cdp("Target.attachToTarget", &attach, |_| {});
}

fn register_and_patch_session(ext_id: String, sid: String, waiting: bool) {
    SW_SESSIONS.with(|m| {
        m.borrow_mut().insert(ext_id.clone(), sid.clone());
    });
    SESSION_EXT.with(|m| {
        m.borrow_mut().insert(sid.clone(), ext_id);
    });

    let bind_params = serde_json::json!({ "name": BADGE_BINDING }).to_string();
    browser_cdp_session(&sid, "Runtime.addBinding", &bind_params, |_| {});
    browser_cdp_session(&sid, "Runtime.enable", "{}", |_| {});
    inject_sw(&sid);
    if waiting {
        browser_cdp_session(&sid, "Runtime.runIfWaitingForDebugger", "{}", |_| {});
    }
}

/// Scans live CDP targets and attaches to any running extension service worker not yet tracked.
pub fn scan_targets() {
    browser_cdp("Target.getTargets", "{}", |res| {
        let Ok(json) = res else { return };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else { return };
        let Some(list) = v.get("targetInfos").and_then(|x| x.as_array()) else { return };
        for info in list {
            let ttype = info.get("type").and_then(|x| x.as_str()).unwrap_or("");
            if ttype != "service_worker" && ttype != "background_page" {
                continue;
            }
            let url = info.get("url").and_then(|x| x.as_str()).unwrap_or("");
            let Some(ext_id) = ext_id_from_url(url) else { continue };
            let already = SW_SESSIONS.with(|m| m.borrow().contains_key(ext_id));
            if already {
                continue;
            }
            if let Some(tid) = info.get("targetId").and_then(|x| x.as_str()) {
                attach_target(tid);
            }
        }
    });
}

/// Starts auto-attaching to extension MV3 service workers over the in-process browser CDP pipe.
pub fn init() {
    set_browser_cdp_event_handler(on_browser_cdp_event);
    let params = serde_json::json!({
        "autoAttach": true,
        "waitForDebuggerOnStart": false,
        "flatten": true,
        "filter": [
            { "type": "service_worker", "exclude": false },
            { "type": "background_page", "exclude": false },
            { "exclude": true }
        ]
    })
    .to_string();
    browser_cdp("Target.setAutoAttach", &params, |r| {
        if let Err(e) = r {
            eprintln!("[zenith] extension SW auto-attach failed: {e}");
        }
    });
    // Also discover dynamically loaded extension workers (e.g. via `Extensions.loadUnpacked`).
    browser_cdp("Target.setDiscoverTargets", r#"{"discover":true}"#, |_| {});
    scan_targets();
}

fn on_browser_cdp_event(method: &str, session_id: Option<&str>, params: &serde_json::Value) {
    match method {
        "Target.targetCreated" | "Target.targetInfoChanged" => {
            let Some(info) = params.get("targetInfo") else { return };
            let ttype = info.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ttype != "service_worker" && ttype != "background_page" {
                return;
            }
            let url = info.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let Some(ext_id) = ext_id_from_url(url) else { return };
            let already = SW_SESSIONS.with(|m| m.borrow().contains_key(ext_id));
            if already {
                return;
            }
            let Some(tid) = info.get("targetId").and_then(|v| v.as_str()) else { return };
            attach_target(tid);
        }
        "Target.attachedToTarget" => {
            let Some(sid) = params.get("sessionId").and_then(|v| v.as_str()) else { return };
            if SESSION_EXT.with(|m| m.borrow().contains_key(sid)) {
                return;
            }
            let url = params
                .pointer("/targetInfo/url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let waiting = params
                .get("waitingForDebugger")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let Some(ext_id) = ext_id_from_url(url) else {
                // Non-extension worker (ordinary website SW): resume and detach immediately.
                if waiting {
                    browser_cdp_session(sid, "Runtime.runIfWaitingForDebugger", "{}", |_| {});
                }
                let detach = serde_json::json!({ "sessionId": sid }).to_string();
                browser_cdp("Target.detachFromTarget", &detach, |_| {});
                return;
            };

            register_and_patch_session(ext_id.to_string(), sid.to_string(), waiting);
        }
        "Runtime.executionContextCreated" => {
            let Some(sid) = session_id else { return };
            if !SESSION_EXT.with(|m| m.borrow().contains_key(sid)) {
                return;
            }
            inject_sw(sid);
        }
        "Target.detachedFromTarget" => {
            let Some(sid) = params.get("sessionId").and_then(|v| v.as_str()) else { return };
            if let Some(ext_id) = SESSION_EXT.with(|m| m.borrow_mut().remove(sid)) {
                SW_SESSIONS.with(|m| {
                    let mut map = m.borrow_mut();
                    if map.get(&ext_id).map(String::as_str) == Some(sid) {
                        map.remove(&ext_id);
                    }
                });
            }
        }
        "Runtime.bindingCalled" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name != BADGE_BINDING {
                return;
            }
            let payload_str = params.get("payload").and_then(|v| v.as_str()).unwrap_or("");
            let Ok(p) = serde_json::from_str::<BadgePayload>(payload_str) else { return };
            let ext_id = if !p.ext_id.is_empty() {
                p.ext_id
            } else if let Some(sid) = session_id {
                SESSION_EXT
                    .with(|m| m.borrow().get(sid).cloned())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            if ext_id.is_empty() {
                return;
            }
            let badge = ActionBadge {
                text: p.text,
                background_color: p.background_color,
                text_color: p.text_color,
            };
            if let Ok(mut b) = super::bridge::BRIDGE.write() {
                let ev = b.set_badge(ext_id, None, badge);
                crate::host::emit("ext-badge", ev);
            }
        }
        _ => {}
    }
}

fn manifest_popup_for(ext_id: &str) -> Option<String> {
    let reg = load_registry();
    let enabled = !reg.disabled.contains(ext_id);
    info_for(ext_id.to_string(), String::new(), enabled, &reg).popup_url
}

fn eval_action_in_session(
    sid: String,
    fallback_popup: Option<String>,
    done: impl FnOnce(Result<ActionOutcome, String>) + 'static,
) {
    const EXPR: &str = r#"(async () => {
      const action = globalThis.chrome?.action || globalThis.chrome?.browserAction;
      const tabs = globalThis.chrome?.tabs;
      let tab = null;
      if (tabs?.query) {
        const list = await tabs.query({ active: true, lastFocusedWindow: true }).catch(() => []);
        tab = list[0] || (await tabs.query({ active: true }).catch(() => []))[0] || null;
      }
      if (action?.getPopup) {
        const pop = await action.getPopup(tab?.id ? { tabId: tab.id } : {}).catch(() => '');
        if (pop) return { kind: 'popup', url: pop };
      }
      if (action?.onClicked && typeof action.onClicked.dispatch === 'function') {
        if (tab) {
          action.onClicked.dispatch(tab);
          return { kind: 'dispatched' };
        }
      }
      return { kind: 'fallback' };
    })()"#;

    let combined = format!("{EXT_SW_SCRIPT}\n{EXPR}");
    let params = serde_json::json!({
        "expression": combined,
        "awaitPromise": true,
        "userGesture": true,
        "returnByValue": true,
    })
    .to_string();

    browser_cdp_session(&sid, "Runtime.evaluate", &params, move |res| {
        let val = res.ok().and_then(|s| {
            let v: serde_json::Value = serde_json::from_str(&s).ok()?;
            v.pointer("/result/value").cloned()
        });
        match val.as_ref().and_then(|v| v.get("kind").and_then(|k| k.as_str())) {
            Some("popup") => {
                if let Some(url) = val.as_ref().and_then(|v| v.get("url").and_then(|u| u.as_str())) {
                    return done(Ok(ActionOutcome::Popup { url: url.to_string() }));
                }
            }
            Some("dispatched") => return done(Ok(ActionOutcome::Dispatched)),
            _ => {}
        }
        match fallback_popup {
            Some(url) => done(Ok(ActionOutcome::Popup { url })),
            None => done(Ok(ActionOutcome::Fallback)),
        }
    });
}

fn dispatch_or_resolve_popup(ext_id: String, done: impl FnOnce(Result<ActionOutcome, String>) + 'static) {
    let fallback_popup = manifest_popup_for(&ext_id);
    if let Some(sid) = SW_SESSIONS.with(|m| m.borrow().get(&ext_id).cloned()) {
        return eval_action_in_session(sid, fallback_popup, done);
    }

    // Worker may have started/restarted after init or while auto-attach missed its URL: look it up.
    browser_cdp("Target.getTargets", "{}", move |res| {
        let tid = res.ok().and_then(|json| {
            let v: serde_json::Value = serde_json::from_str(&json).ok()?;
            v.get("targetInfos")?.as_array()?.iter().find_map(|info| {
                let ttype = info.get("type")?.as_str()?;
                if ttype != "service_worker" && ttype != "background_page" {
                    return None;
                }
                let url = info.get("url")?.as_str()?;
                if ext_id_from_url(url) == Some(ext_id.as_str()) {
                    info.get("targetId")?.as_str().map(str::to_string)
                } else {
                    None
                }
            })
        });
        let Some(tid) = tid else {
            return match fallback_popup {
                Some(url) => done(Ok(ActionOutcome::Popup { url })),
                None => done(Ok(ActionOutcome::Fallback)),
            };
        };
        let attach = serde_json::json!({ "targetId": tid, "flatten": true }).to_string();
        browser_cdp("Target.attachToTarget", &attach, move |att| {
            let sid = att.ok().and_then(|json| {
                let v: serde_json::Value = serde_json::from_str(&json).ok()?;
                v.get("sessionId")?.as_str().map(str::to_string)
            });
            match sid {
                Some(sid) => {
                    register_and_patch_session(ext_id, sid.clone(), false);
                    eval_action_in_session(sid, fallback_popup, done);
                }
                None => match fallback_popup {
                    Some(url) => done(Ok(ActionOutcome::Popup { url })),
                    None => done(Ok(ActionOutcome::Fallback)),
                },
            }
        });
    });
}

/// Activates the focused web tab in Chromium (`Page.bringToFront`), then resolves the extension's
/// live action: dynamic/default popup URL or `chrome.action.onClicked` dispatch.
pub fn trigger_action(
    ext_id: String,
    active_tab_id: Option<String>,
    done: impl FnOnce(Result<ActionOutcome, String>) + 'static,
) {
    let browser = active_tab_id.and_then(|tid| {
        with_host(|h| {
            h.tabs
                .get(&tid)
                .filter(|t| !t.shared.is_glance())
                .and_then(|t| t.browser.clone())
        })
        .flatten()
    });
    if let Some(b) = browser {
        cdp_call(&b, "Page.bringToFront", "{}", move |_| {
            dispatch_or_resolve_popup(ext_id, done);
        });
    } else {
        dispatch_or_resolve_popup(ext_id, done);
    }
}

#[cfg(test)]
mod tests {
    use super::ext_id_from_url;

    #[test]
    fn parses_extension_id_only_for_valid_chrome_extension_urls() {
        assert_eq!(
            ext_id_from_url("chrome-extension://eimadpbcbfnmbkopoojfekhnkhdbieeh/background/index.js"),
            Some("eimadpbcbfnmbkopoojfekhnkhdbieeh")
        );
        assert_eq!(ext_id_from_url("https://example.com/sw.js"), None);
        assert_eq!(ext_id_from_url("chrome-extension://short/sw.js"), None);
    }
}
