// End-to-end test harness: the CDP connection to the isolated Zenith test instance (see
// run.ps1), starting and stopping it, and the helpers the scenarios in cases/ drive it with.
// Paths like src-tauri/target/debug/zenith.exe and scripts/dev/*.ps1 are relative to apps/desktop
// (the directory the suite runs from).
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawn, spawnSync } from "node:child_process";

export const PORT = Number(process.env.ZENITH_REMOTE_DEBUG_PORT || 9444);
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function targets() {
  const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
  return (await res.json()).filter((t) => t.type === "page");
}
// The bundled UI (tauri.localhost), or the Vite dev server when the binary came from `tauri dev`.
export const isChrome = (t) =>
  /^https?:\/\/(tauri\.localhost|127\.0\.0\.1:1420|localhost:1420)/.test(t.url);

export const sessions = new Map();
export async function session(target) {
  if (sessions.has(target.id)) return sessions.get(target.id);
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((res, rej) => {
    ws.onopen = res;
    ws.onerror = rej;
  });
  let id = 0;
  const pending = new Map();
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  };
  ws.onclose = () => {
    sessions.delete(target.id);
    for (const resolve of pending.values())
      resolve({ error: { message: "target closed" } });
    pending.clear();
  };
  const s = {
    send: (method, params = {}, timeout = 8000) =>
      new Promise((resolve, reject) => {
        const mid = ++id;
        const timer = setTimeout(() => {
          pending.delete(mid);
          reject(new Error(`CDP ${method} timed out`));
        }, timeout);
        pending.set(mid, (msg) => {
          clearTimeout(timer);
          resolve(msg);
        });
        try {
          ws.send(JSON.stringify({ id: mid, method, params }));
        } catch (e) {
          clearTimeout(timer);
          reject(e);
        }
      }),
    close: () => ws.close(),
  };
  sessions.set(target.id, s);
  return s;
}

export async function evalIn(target, expression, timeout) {
  const s = await session(target);
  const r = await s.send(
    "Runtime.evaluate",
    { expression, awaitPromise: true, returnByValue: true },
    timeout,
  );
  if (r.error) throw new Error(r.error.message);
  if (r.result?.exceptionDetails)
    throw new Error(
      r.result.exceptionDetails.exception?.description ?? "eval failed",
    );
  return r.result?.result?.value;
}

/** The normal window's chrome UI (incognito windows have their own chrome target). */
export async function chrome() {
  const all = (await targets()).filter(isChrome);
  if (!all.length) throw new Error("chrome UI target not found");
  if (all.length === 1) return all[0];
  for (const t of all)
    if (!(await evalIn(t, "!!window.__ZENITH_INCOGNITO__"))) return t;
  return all[0];
}
/** Chrome UI targets of incognito windows. */
export async function incognitoChromes() {
  const out = [];
  for (const t of (await targets()).filter(isChrome))
    if (await evalIn(t, "!!window.__ZENITH_INCOGNITO__")) out.push(t);
  return out;
}
export const resizeWindow = (w, h) =>
  spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "scripts/dev/resize.ps1",
      "-Width",
      String(w),
      "-Height",
      String(h),
    ],
    { stdio: "ignore" },
  );
export const ui = async (expr, timeout) => evalIn(await chrome(), expr, timeout);
export const invoke = (cmd, a = {}, timeout) =>
  ui(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(a)})`,
    timeout,
  );

export async function waitFor(fn, what, timeout = 10000) {
  const until = Date.now() + timeout;
  let last;
  while (Date.now() < until) {
    try {
      last = await fn();
      if (last) return last;
    } catch (e) {
      last = String(e);
    }
    await sleep(200);
  }
  throw new Error(
    `timed out waiting for ${what} (last: ${JSON.stringify(last)})`,
  );
}
export const waitUi = (expr, what, timeout) => waitFor(() => ui(expr), what, timeout);

export async function page(match) {
  return waitFor(
    async () =>
      (await targets()).find((t) => !isChrome(t) && t.url.includes(match)),
    `page ${match}`,
  );
}

export async function click(target, x, y, modifiers = 0, button = "left") {
  const s = await session(target);
  await s.send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x,
    y,
    modifiers,
  });
  await s.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x,
    y,
    button,
    clickCount: 1,
    modifiers,
  });
  await s.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x,
    y,
    button,
    clickCount: 1,
    modifiers,
  });
}

export async function clickSelector(target, selector, modifiers = 0) {
  const r = await evalIn(
    target,
    `(() => { const e = document.querySelector(${JSON.stringify(selector)}); if (!e) return null; const b = e.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`,
  );
  if (!r) throw new Error(`no element ${selector}`);
  await click(target, r.x, r.y, modifiers);
}

export const typeInPalette = (text, key = "Enter", extra = "") =>
  ui(`(async () => {
    const i = document.querySelector('.palette input');
    i.value = ${JSON.stringify(text)};
    i.dispatchEvent(new Event('input', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 450));
    i.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(key)}, bubbles: true ${extra} }));
    return true;
  })()`);

export async function openUrl(url, newTab = true) {
  await ui(
    `document.querySelector(${JSON.stringify(newTab ? ".new-tab" : ".pill .main")}).click()`,
  );
  await waitUi(`!!document.querySelector('.palette input')`, "palette");
  await typeInPalette(url);
}

export const tabTitles = () =>
  ui(`[...document.querySelectorAll('.list .tab')].map((t) => t.title)`);
export const layout = () => invoke("debug_layout");
export const activeTabId = () =>
  ui(`document.querySelector('.list .tab.active')?.dataset.tabId ?? null`);
export const tabSel = (id, extra = "") => `.tab[data-tab-id="${id}"]${extra}`;

/** The DevTools target of a specific tab (several tabs may show the same URL). */
export async function pageOf(tabId) {
  return waitFor(async () => {
    const t = (await layout()).tabs.find((x) => x.id === tabId);
    return t?.targetId
      ? (await targets()).find((x) => x.id === t.targetId)
      : null;
  }, `target of tab ${tabId}`);
}

export async function appExited() {
  return waitFor(
    async () => {
      try {
        await targets();
        return false;
      } catch {
        return true;
      }
    },
    "app exit",
    15000,
  );
}

export async function relaunch() {
  // Start the test instance directly (same profile, port and PID file as run.ps1).
  const profile = join(tmpdir(), "zenith-test-profile");
  const child = spawn(join("src-tauri", "target", "debug", "zenith.exe"), [], {
    detached: true,
    stdio: "ignore",
    env: {
      ...process.env,
      ZENITH_PROFILE_DIR: profile,
      ZENITH_REMOTE_DEBUG_PORT: String(PORT),
      ZENITH_PERF: "1",
    },
  });
  child.unref();
  mkdirSync(join(tmpdir(), "zenith-dev"), { recursive: true });
  writeFileSync(
    join(tmpdir(), "zenith-dev", "test-instance.pid"),
    String(child.pid),
  );
  sessions.clear();
  await waitFor(
    async () => {
      try {
        return !!(await ui(`!!document.querySelector('.sidebar .new-tab')`));
      } catch {
        return false;
      }
    },
    "relaunched UI",
    25000,
  );
}

/** Runs an item of the page-actions menu (the "⋯" in the address bar). */
export async function pageMenuItem(label) {
  await ui(`document.querySelector('.pill [data-action=page-menu]').click()`);
  await waitUi(`!!document.querySelector('.menu')`, "page menu");
  await ui(
    `[...document.querySelectorAll('.menu .item')].find((b) => b.textContent.trim().startsWith(${JSON.stringify(label)})).click()`,
  );
}

export async function openSettings() {
  await ui(`document.querySelector('[data-action=app-menu]').click()`);
  await waitUi(`!!document.querySelector('.menu')`, "app menu");
  await ui(
    `[...document.querySelectorAll('.menu .item')].find((b) => b.textContent.includes('Settings')).click()`,
  );
  await waitUi(
    `!!document.querySelector('#settings-tabs select')`,
    "settings page",
  );
}

export const downloadDir = join(tmpdir(), "zenith-e2e-downloads");

/** A fresh profile refreshes its filter lists at startup and then re-applies the UI's own
 *  settings (default download folder included). Tests that change runtime settings wait for
 *  that first, or their files would land in the user's real Downloads folder. */
export const settleStartupSettings = () =>
  waitFor(
    async () =>
      (JSON.parse((await invoke("settings_load")) ?? "{}").filterListsUpdated ??
        0) > 0,
    "filter list refresh",
    45000,
  ).catch(() => {});
export const extDir = join(tmpdir(), "zenith-e2e-ext");
