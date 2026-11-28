//! Scripts injected into tab pages: link routing (Glance, background tabs), the Chrome Web
//! Store install button and the toolbar-popup `tabs.query` shim for extensions.

use cef::*;

use super::TabShared;
use crate::host::cef_app::cef_str;

/// Injects the extension page/popup compatibility shim on `chrome-extension://` main frames.
pub(super) fn inject_ext_compat(frame: &mut Frame, shared: &TabShared) {
    if frame.is_main() == 0 {
        return;
    }
    let url = cef_str(frame.url());
    if !url.starts_with("chrome-extension://") {
        return;
    }
    let opener_json = serde_json::to_string(&shared.opener_url).unwrap_or_else(|_| "null".into());
    let script = EXT_POPUP_SCRIPT
        .replace("__OPENER__", &opener_json)
        .replace("__IS_POPUP__", if shared.is_glance() { "true" } else { "false" });
    frame.execute_java_script(
        Some(&CefString::from(script.as_str())),
        Some(&CefString::from("zenith://internal/ext-popup.js")),
        1,
    );
}

/// Pushes updated click-modifier trigger config to every live page tab without reloading.
pub fn apply_triggers(triggers_json: &str) {
    let js = format!("globalThis.__z_trig={triggers_json};");
    let browsers: Vec<Browser> = crate::host::with_host(|h| {
        h.tabs.values().filter_map(|t| t.browser.clone()).collect()
    })
    .unwrap_or_default();
    for b in browsers {
        if let Some(frame) = b.main_frame() {
            frame.execute_java_script(
                Some(&CefString::from(js.as_str())),
                Some(&CefString::from("zenith://internal/triggers.js")),
                1,
            );
        }
    }
}

/// Injected into every frame of every page. It cannot reach the app (tabs have no IPC bridge):
/// it routes configured modifier clicks (Glance, Split View, Little Window) and Ctrl/middle-click
/// (background tab) via named `window.open` calls that NewWindowRequested recognises and cancels.
pub const PAGE_SCRIPT: &str = r#"(() => {
  const open = window.open.bind(window);
  const link = (e) => {
    for (const n of e.composedPath()) {
      if (n instanceof HTMLAnchorElement && n.href) return n;
    }
    return null;
  };
  const matchMod = (e, spec) => {
    if (!spec || spec === 'none') return false;
    const parts = spec.split('+');
    const want = { c: false, a: false, s: false };
    for (const p of parts) {
      if (p === 'ctrl') want.c = true;
      else if (p === 'alt') want.a = true;
      else if (p === 'shift') want.s = true;
    }
    return (e.ctrlKey || e.metaKey) === want.c && e.altKey === want.a && e.shiftKey === want.s;
  };
  const route = (e, target) => {
    const a = link(e);
    if (!a || a.hasAttribute('download') || !/^https?:/i.test(a.href)) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    open(a.href, target);
  };
  addEventListener('click', (e) => {
    if (e.defaultPrevented || e.button !== 0 || !e.isTrusted) return;
    const t = globalThis.__z_trig || { g: 'shift', s: 'alt', l: 'ctrl+alt' };
    const checks = [
      [t.l, '_zenith_little'],
      [t.g, '_zenith_glance:' + Math.round(e.clientX) + ',' + Math.round(e.clientY)],
      [t.s, '_zenith_split'],
    ].sort((a, b) => (b[0] || '').split('+').length - (a[0] || '').split('+').length);
    for (const [spec, name] of checks) {
      if (matchMod(e, spec)) return route(e, name);
    }
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey) route(e, '_zenith_bg');
  });
  addEventListener('auxclick', (e) => {
    if (e.defaultPrevented || e.button !== 1 || !e.isTrusted) return;
    route(e, '_zenith_bg');
  });
  // shortcut: per-frame audio tracking via media/AudioContext hooks because CEF 156 has no
  // non-muting OnAudioStateChanged callback; replace with a CEF patch in Phase 4.
  const media = new Set();
  const ctxs = new Set();
  let lastAudio = false;
  const syncAudio = () => {
    for (const m of media) if (m.paused || m.ended || m.muted || m.volume === 0) media.delete(m);
    for (const c of ctxs) if (c.state === 'closed') ctxs.delete(c);
    let playing = media.size > 0;
    if (!playing) for (const c of ctxs) if (c.state === 'running') { playing = true; break; }
    if (playing !== lastAudio) {
      lastAudio = playing;
      open('about:blank#' + (playing ? '1' : '0'), '_zenith_audio');
    }
  };
  const trackMedia = (el) => {
    if (!(el instanceof HTMLMediaElement)) return;
    if (!el.paused && !el.ended && !el.muted && el.volume > 0) media.add(el);
    else media.delete(el);
    syncAudio();
  };
  for (const ev of ['play', 'playing', 'pause', 'ended', 'volumechange', 'emptied']) {
    addEventListener(ev, (e) => trackMedia(e.target), true);
  }
  if (typeof HTMLMediaElement !== 'undefined' && HTMLMediaElement.prototype.play) {
    const origPlay = HTMLMediaElement.prototype.play;
    HTMLMediaElement.prototype.play = function (...a) {
      const p = origPlay.apply(this, a);
      const watch = () => trackMedia(this);
      this.addEventListener('pause', watch);
      this.addEventListener('ended', watch);
      this.addEventListener('volumechange', watch);
      if (p && typeof p.then === 'function') p.then(watch, () => {});
      else watch();
      return p;
    };
  }
  for (const key of ['AudioContext', 'webkitAudioContext']) {
    const Orig = globalThis[key];
    if (typeof Orig !== 'function') continue;
    globalThis[key] = new Proxy(Orig, {
      construct(T, args, nt) {
        const ctx = Reflect.construct(T, args, nt);
        ctxs.add(ctx);
        ctx.addEventListener('statechange', syncAudio);
        syncAudio();
        setTimeout(syncAudio, 60);
        return ctx;
      },
    });
  }
})();"#;

/// Chrome Web Store pages (main frame only, normal windows): the store's own "Add to Chrome"
/// button installs into Zenith. It keeps its label and is taken over when it is there; otherwise
/// a floating "Add to Chrome" button appears. A real click calls `window.open` with the reserved `_zenith_ext`
/// name, which NewWindowRequested accepts only from the store and turns into an install prompt.
/// Built with DOM calls only and CSSOM styles (the store uses Trusted Types and a strict CSP).
pub const STORE_SCRIPT: &str = r#"(() => {
  if (location.hostname !== 'chromewebstore.google.com') return;
  const open = window.open.bind(window);
  const idOf = () => {
    const m = location.pathname.match(/\/detail\/(?:[^/]+\/)?([a-p]{32})(?:[/?#]|$)/);
    return m ? m[1] : null;
  };
  const install = (e) => {
    e.preventDefault();
    e.stopImmediatePropagation();
    if (!e.isTrusted) return;
    const id = idOf();
    if (id) open('https://chromewebstore.google.com/detail/' + id, '_zenith_ext');
  };
  let host = null;
  const pill = () => {
    if (host) return host;
    host = document.createElement('div');
    Object.assign(host.style, { position: 'fixed', right: '24px', bottom: '24px', zIndex: '2147483647', display: 'none' });
    const root = host.attachShadow({ mode: 'closed' });
    const b = document.createElement('button');
    b.textContent = 'Add to Chrome';
    Object.assign(b.style, {
      font: '600 14px/1 "Segoe UI Variable", "Segoe UI", system-ui, sans-serif', padding: '12px 20px',
      borderRadius: '999px', border: '0', color: '#fff', background: '#7c6cf0', cursor: 'pointer',
      boxShadow: '0 6px 24px rgba(40, 20, 120, .35)',
    });
    b.addEventListener('click', install);
    root.appendChild(b);
    return host;
  };
  const nativeButtons = () => [...document.querySelectorAll('button')].filter((b) => b.dataset.zenith ||
    /^(add to (chrome|edge)|get)$/i.test((b.textContent || '').trim()));
  const tick = () => {
    const id = idOf();
    let taken = false;
    if (id) {
      for (const b of nativeButtons()) {
        if (!b.dataset.zenith) {
          b.dataset.zenith = '1';
          b.addEventListener('click', install, true);
        }
        b.disabled = false;
        b.removeAttribute('aria-disabled');
        if (b.offsetParent) taken = true;
      }
    }
    const p = pill();
    if (!p.isConnected && document.documentElement) document.documentElement.appendChild(p);
    p.style.display = id && !taken ? 'block' : 'none';
  };
  setInterval(tick, 800);
  addEventListener('DOMContentLoaded', tick);
})();"#;

/// Runs in `chrome-extension://` main frames (popups and extension pages).
/// - Normalizes CEF's child-window `windowType: "popup"` -> `"normal"` for `chrome.tabs.query`
///   and `chrome.windows.*` so extensions filtering by `windowType: "normal"` find Zenith's tabs.
/// - For toolbar popups (`IS_POPUP`), ensures `chrome.tabs.getCurrent()` returns `undefined`
///   (per `cef-extension-contract.md`) and resolves window-scoped active-tab queries to the opener.
pub const EXT_POPUP_SCRIPT: &str = r#"(() => {
  if (globalThis.__zenithExtPageCompat || window.top !== window) return;
  const OPENER = __OPENER__;
  const IS_POPUP = __IS_POPUP__;
  const tabs = globalThis.chrome && chrome.tabs;
  if (!tabs || typeof tabs.query !== 'function') return;
  globalThis.__zenithExtPageCompat = true;
  const origQuery = tabs.query.bind(tabs);
  const call = (fn, ...a) => new Promise((res) => {
    try { fn(...a, (r) => { void chrome.runtime.lastError; res(r); }); } catch { res(undefined); }
  });
  const isExt = (t) => typeof t.url === 'string' && /^chrome-extension:/.test(t.url);
  const origin = (u) => { try { return new URL(u).origin; } catch { return ''; } };
  const normQuery = (q) => {
    if (!q || q.windowType !== 'normal') return q || {};
    const nq = { ...q };
    delete nq.windowType;
    return nq;
  };
  const pick = (list) => {
    const pages = (list || []).filter((t) => !isExt(t));
    const recent = (a, b) => (b.lastAccessed || 0) - (a.lastAccessed || 0);
    return (
      pages.find((t) => OPENER && t.url === OPENER) ||
      pages.find((t) => OPENER && t.url && origin(t.url) === origin(OPENER)) ||
      pages.sort(recent)[0]
    );
  };
  const answer = async (q) => {
    const nq = normQuery(q);
    const direct = (await call(origQuery, nq)) || [];
    if (!IS_POPUP) return direct;
    const scoped = nq.currentWindow || nq.lastFocusedWindow;
    if (!scoped) return direct.filter((t) => !isExt(t));
    if (nq.active || nq.highlighted) {
      const hit = direct.find((t) => !isExt(t) && (!OPENER || t.url === OPENER));
      if (hit) return [hit];
      const rest = { ...nq };
      delete rest.currentWindow;
      delete rest.lastFocusedWindow;
      delete rest.windowId;
      delete rest.active;
      delete rest.highlighted;
      const found = (await call(origQuery, rest)) || [];
      const t = pick(found);
      return t ? [t] : [];
    }
    return direct.filter((t) => !isExt(t));
  };
  const set = (obj, key, fn) => {
    try { obj[key] = fn; } catch {}
    if (obj[key] !== fn) { try { Object.defineProperty(obj, key, { value: fn, configurable: true, writable: true }); } catch {} }
  };
  set(tabs, 'query', function (q, cb) {
    const p = answer(q);
    if (typeof cb === 'function') { p.then(cb, () => cb([])); return; }
    return p;
  });
  if (IS_POPUP && typeof tabs.getCurrent === 'function') {
    set(tabs, 'getCurrent', function (cb) {
      if (typeof cb === 'function') { setTimeout(() => cb(undefined)); return; }
      return Promise.resolve(undefined);
    });
  }
  const wins = chrome.windows;
  if (wins) {
    const normWin = (w) => (w && w.type === 'popup' ? { ...w, type: 'normal' } : w);
    const fixOpts = (o) => {
      if (!o || !Array.isArray(o.windowTypes)) return o;
      if (o.windowTypes.includes('normal') && !o.windowTypes.includes('popup')) {
        return { ...o, windowTypes: [...o.windowTypes, 'popup'] };
      }
      return o;
    };
    for (const k of ['getLastFocused', 'getCurrent', 'get']) {
      if (typeof wins[k] !== 'function') continue;
      const orig = wins[k].bind(wins);
      set(wins, k, function (...args) {
        const cb = typeof args[args.length - 1] === 'function' ? args.pop() : null;
        if (k === 'get') { if (args[1]) args[1] = fixOpts(args[1]); }
        else if (args[0]) args[0] = fixOpts(args[0]);
        const p = call(orig, ...args).then(normWin);
        if (cb) { p.then(cb); return; }
        return p;
      });
    }
    if (typeof wins.getAll === 'function') {
      const origAll = wins.getAll.bind(wins);
      set(wins, 'getAll', function (...args) {
        const cb = typeof args[args.length - 1] === 'function' ? args.pop() : null;
        if (args[0]) args[0] = fixOpts(args[0]);
        const p = call(origAll, ...args).then((ws) => (ws || []).map(normWin));
        if (cb) { p.then(cb); return; }
        return p;
      });
    }
  }
})();"#;
