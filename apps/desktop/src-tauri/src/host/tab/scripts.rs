//! Scripts injected into tab pages: link routing (Glance, background tabs), the Chrome Web
//! Store install button and the toolbar-popup `tabs.query` shim for extensions.

/// Injected into every frame of every page. It cannot reach the app (tabs have no IPC bridge):
/// it only rewrites Alt+click into a Glance request and Ctrl/middle-click into a background-tab
/// request, using named `window.open` calls that NewWindowRequested recognises and cancels.
pub(super) const PAGE_SCRIPT: &str = r#"(() => {
  const open = window.open.bind(window);
  const link = (e) => {
    for (const n of e.composedPath()) {
      if (n instanceof HTMLAnchorElement && n.href) return n;
    }
    return null;
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
    if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) route(e, '_zenith_glance');
    else if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey) route(e, '_zenith_bg');
  });
  addEventListener('auxclick', (e) => {
    if (e.defaultPrevented || e.button !== 1 || !e.isTrusted) return;
    route(e, '_zenith_bg');
  });
})();"#;

/// Chrome Web Store pages (main frame only, normal windows): the store's own "Add to Chrome"
/// button installs into Zenith. It keeps its label and is taken over when it is there; otherwise
/// a floating "Add to Chrome" button appears. A real click calls `window.open` with the reserved `_zenith_ext`
/// name, which NewWindowRequested accepts only from the store and turns into an install prompt.
/// Built with DOM calls only and CSSOM styles (the store uses Trusted Types and a strict CSP).
pub(super) const STORE_SCRIPT: &str = r#"(() => {
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

/// Runs in an extension's toolbar popup. In Chrome a popup is not a tab, so
/// `tabs.query({ active: true, currentWindow: true })` finds the page under it. Every WebView2
/// view is its own browser window, so that query would return the popup itself ("not a
/// website"). Window-scoped queries are answered with the page the popup was opened over.
pub(super) const EXT_POPUP_SCRIPT: &str = r#"(() => {
  const OPENER = __OPENER__;
  const tabs = globalThis.chrome && chrome.tabs;
  if (!tabs || typeof tabs.query !== 'function' || window.top !== window) return;
  const query = tabs.query.bind(tabs);
  const getCurrent = typeof tabs.getCurrent === 'function' ? tabs.getCurrent.bind(tabs) : null;
  const call = (fn, ...a) => new Promise((res) => {
    try { fn(...a, (r) => { void chrome.runtime.lastError; res(r); }); } catch { res(undefined); }
  });
  const isExt = (t) => typeof t.url === 'string' && /^chrome-extension:/.test(t.url);
  const origin = (u) => { try { return new URL(u).origin; } catch { return ''; } };
  // The opener among `list`: same address, then same site, then the most recently used page.
  const pick = async (list) => {
    const self = getCurrent ? await call(getCurrent) : undefined;
    const pages = (list || []).filter((t) => (!self || t.id !== self.id) && !isExt(t));
    const recent = (a, b) => (b.lastAccessed || 0) - (a.lastAccessed || 0);
    return (
      pages.find((t) => OPENER && t.url === OPENER) ||
      pages.find((t) => OPENER && t.url && origin(t.url) === origin(OPENER)) ||
      pages.sort(recent)[0]
    );
  };
  const scoped = (q) => q && (q.currentWindow || q.lastFocusedWindow);
  const answer = async (q) => {
    if (!scoped(q)) return (await call(query, q || {})) || [];
    const rest = { ...q };
    delete rest.currentWindow;
    delete rest.lastFocusedWindow;
    delete rest.windowId;
    delete rest.active;
    delete rest.highlighted;
    const found = (await call(query, rest)) || [];
    if (q.active || q.highlighted) {
      const t = await pick(found);
      return t ? [t] : [];
    }
    return found.filter((t) => !isExt(t));
  };
  const wrapped = function (q, cb) {
    const p = answer(q);
    if (typeof cb === 'function') { p.then(cb, () => cb([])); return; }
    return p;
  };
  const wrappedCurrent = function (cb) {
    // A popup is not a tab.
    if (typeof cb === 'function') { setTimeout(() => cb(undefined)); return; }
    return Promise.resolve(undefined);
  };
  const set = (obj, key, fn) => {
    try { obj[key] = fn; } catch {}
    if (obj[key] !== fn) { try { Object.defineProperty(obj, key, { value: fn, configurable: true, writable: true }); } catch {} }
  };
  set(tabs, 'query', wrapped);
  if (getCurrent) set(tabs, 'getCurrent', wrappedCurrent);
  const windows = chrome.windows;
  if (windows && typeof windows.get === 'function') {
    const get = windows.get.bind(windows);
    const current = async (opts) => {
      const t = await pick((await call(query, {})) || []);
      if (t) return await call(get, t.windowId, opts || {});
      return undefined;
    };
    for (const key of ['getCurrent', 'getLastFocused']) {
      if (typeof windows[key] !== 'function') continue;
      const orig = windows[key].bind(windows);
      set(windows, key, function (a, b) {
        const cb = typeof a === 'function' ? a : b;
        const opts = typeof a === 'function' ? undefined : a;
        const p = current(opts).then((w) => w || call(orig, opts || {}));
        if (typeof cb === 'function') { p.then(cb); return; }
        return p;
      });
    }
  }
})();"#;
