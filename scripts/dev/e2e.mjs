// End-to-end checks against an isolated Zenith test instance (debug build, own profile and
// remote-debugging port; see run.ps1). Never drives any other Zenith window.
//   powershell -File scripts/dev/run.ps1 -Fresh
//   node scripts/dev/e2e.mjs [--only name,name] [--skip-restart]
// Drives the chrome UI through the DOM and pages through CDP Input events (trusted input).
import {
  mkdirSync,
  writeFileSync,
  existsSync,
  readdirSync,
  rmSync,
  readFileSync,
  statSync,
} from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawn, spawnSync } from "node:child_process";

const PORT = Number(process.env.ZENITH_REMOTE_DEBUG_PORT || 9444);
const args = process.argv.slice(2);
const only = args.includes("--only")
  ? args[args.indexOf("--only") + 1].split(",")
  : null;
const skipRestart = args.includes("--skip-restart");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function targets() {
  const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
  return (await res.json()).filter((t) => t.type === "page");
}
// The bundled UI (tauri.localhost), or the Vite dev server when the binary came from `tauri dev`.
const isChrome = (t) =>
  /^https?:\/\/(tauri\.localhost|127\.0\.0\.1:1420|localhost:1420)/.test(t.url);

const sessions = new Map();
async function session(target) {
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

async function evalIn(target, expression, timeout) {
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
async function chrome() {
  const all = (await targets()).filter(isChrome);
  if (!all.length) throw new Error("chrome UI target not found");
  if (all.length === 1) return all[0];
  for (const t of all)
    if (!(await evalIn(t, "!!window.__ZENITH_INCOGNITO__"))) return t;
  return all[0];
}
/** Chrome UI targets of incognito windows. */
async function incognitoChromes() {
  const out = [];
  for (const t of (await targets()).filter(isChrome))
    if (await evalIn(t, "!!window.__ZENITH_INCOGNITO__")) out.push(t);
  return out;
}
const resizeWindow = (w, h) =>
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
const ui = async (expr, timeout) => evalIn(await chrome(), expr, timeout);
const invoke = (cmd, a = {}, timeout) =>
  ui(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(a)})`,
    timeout,
  );

async function waitFor(fn, what, timeout = 10000) {
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
const waitUi = (expr, what, timeout) => waitFor(() => ui(expr), what, timeout);

async function page(match) {
  return waitFor(
    async () =>
      (await targets()).find((t) => !isChrome(t) && t.url.includes(match)),
    `page ${match}`,
  );
}

async function click(target, x, y, modifiers = 0, button = "left") {
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

async function clickSelector(target, selector, modifiers = 0) {
  const r = await evalIn(
    target,
    `(() => { const e = document.querySelector(${JSON.stringify(selector)}); if (!e) return null; const b = e.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`,
  );
  if (!r) throw new Error(`no element ${selector}`);
  await click(target, r.x, r.y, modifiers);
}

const typeInPalette = (text, key = "Enter", extra = "") =>
  ui(`(async () => {
    const i = document.querySelector('.palette input');
    i.value = ${JSON.stringify(text)};
    i.dispatchEvent(new Event('input', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 450));
    i.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(key)}, bubbles: true ${extra} }));
    return true;
  })()`);

async function openUrl(url, newTab = true) {
  await ui(
    `document.querySelector(${JSON.stringify(newTab ? ".new-tab" : ".pill .main")}).click()`,
  );
  await waitUi(`!!document.querySelector('.palette input')`, "palette");
  await typeInPalette(url);
}

const tabTitles = () =>
  ui(`[...document.querySelectorAll('.list .tab')].map((t) => t.title)`);
const layout = () => invoke("debug_layout");
const activeTabId = () =>
  ui(`document.querySelector('.list .tab.active')?.dataset.tabId ?? null`);
const tabSel = (id, extra = "") => `.tab[data-tab-id="${id}"]${extra}`;

/** The DevTools target of a specific tab (several tabs may show the same URL). */
async function pageOf(tabId) {
  return waitFor(async () => {
    const t = (await layout()).tabs.find((x) => x.id === tabId);
    return t?.targetId
      ? (await targets()).find((x) => x.id === t.targetId)
      : null;
  }, `target of tab ${tabId}`);
}

async function appExited() {
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

async function relaunch() {
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
async function pageMenuItem(label) {
  await ui(`document.querySelector('.pill [data-action=page-menu]').click()`);
  await waitUi(`!!document.querySelector('.menu')`, "page menu");
  await ui(
    `[...document.querySelectorAll('.menu .item')].find((b) => b.textContent.trim().startsWith(${JSON.stringify(label)})).click()`,
  );
}

async function openSettings() {
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

// --------------------------------------------------------------------------------------------
const results = [];
const resultsFile = join(tmpdir(), "zenith-e2e-results.json");
writeFileSync(resultsFile, "[]");
async function test(name, fn, limitMs = 60000) {
  if (only && !only.includes(name)) return;
  const start = Date.now();
  const deadline = new Promise((_, rej) =>
    setTimeout(
      () => rej(new Error(`test timed out after ${limitMs / 1000}s`)),
      limitMs,
    ),
  );
  try {
    const detail = await Promise.race([fn(), deadline]);
    results.push({ name, ok: true, detail, ms: Date.now() - start });
    console.log(`PASS ${name}${detail ? ` — ${detail}` : ""}`);
  } catch (e) {
    results.push({
      name,
      ok: false,
      detail: String(e?.message ?? e),
      ms: Date.now() - start,
    });
    console.log(`FAIL ${name} — ${e?.message ?? e}`);
  }
  writeFileSync(resultsFile, JSON.stringify(results, null, 2));
}

const downloadDir = join(tmpdir(), "zenith-e2e-downloads");

/** A fresh profile refreshes its filter lists at startup and then re-applies the UI's own
 *  settings (default download folder included). Tests that change runtime settings wait for
 *  that first, or their files would land in the user's real Downloads folder. */
const settleStartupSettings = () =>
  waitFor(
    async () =>
      (JSON.parse((await invoke("settings_load")) ?? "{}").filterListsUpdated ??
        0) > 0,
    "filter list refresh",
    45000,
  ).catch(() => {});
const extDir = join(tmpdir(), "zenith-e2e-ext");

await waitUi(`!!document.querySelector('.app')`, "chrome UI", 20000);
// Start from a known layout: leave compact mode if it was turned on.
if (await ui(`document.querySelector('.app').classList.contains('compact')`)) {
  await ui(
    `document.querySelector('.hot-left').dispatchEvent(new MouseEvent('mouseenter'))`,
  );
  await waitUi(
    `!!document.querySelector('.flyout [data-action=compact]')`,
    "floating sidebar",
  );
  await ui(`document.querySelector('.flyout [data-action=compact]').click()`);
  await waitUi(
    `!document.querySelector('.app').classList.contains('compact')`,
    "compact mode off",
  );
}
await waitUi(
  `!!document.querySelector('.sidebar .new-tab')`,
  "chrome UI with restored session",
  20000,
);

await test("palette-navigation", async () => {
  await openUrl("example.com");
  await waitFor(
    async () => (await tabTitles()).includes("Example Domain"),
    "Example Domain tab",
  );
  await page("example.com");
  return (await tabTitles()).join(", ");
});

await test("tab-has-no-ipc", async () => {
  const t = await page("example.com");
  const v = await evalIn(
    t,
    `[typeof window.__TAURI_INTERNALS__, typeof window.__TAURI__, typeof window.ipc].join(',')`,
  );
  if (v !== "undefined,undefined,undefined") throw new Error(`page sees ${v}`);
  return "no Tauri bridge in page";
});

await test("webview-matches-pane", async () => {
  const pane = await ui(
    `(() => { const b = document.querySelector('.web').getBoundingClientRect(); return [b.left, b.top, b.width, b.height].map(Math.round); })()`,
  );
  const l = await layout();
  const v = l.tabs.find((t) => t.visible);
  if (!v) throw new Error("no visible webview");
  const got = [v.x, v.y, v.w, v.h].map(Math.round);
  if (got.some((n, i) => Math.abs(n - pane[i]) > 2))
    throw new Error(`pane ${pane} vs webview ${got}`);
  return `webview at ${got.join(",")}`;
});

await test("palette-raises-overlay", async () => {
  await ui(`document.querySelector('.pill .main').click()`);
  await waitUi(`!!document.querySelector('.palette')`, "palette");
  await waitFor(async () => (await layout()).chromeRaised, "chrome raised");
  await ui(
    `document.querySelector('.palette input').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`,
  );
  await waitFor(async () => !(await layout()).chromeRaised, "chrome lowered");
  return "raised while open, lowered after";
});

await test("ad-blocking", async () => {
  const t = await page("example.com");
  await evalIn(
    t,
    `new Promise((res) => { const s = document.createElement('script'); s.src = 'https://www.googletagmanager.com/gtag/js?id=G-ZENITHTEST'; s.onerror = () => res('blocked'); s.onload = () => res('loaded'); document.head.append(s); })`,
  );
  const ok = await evalIn(
    t,
    `new Promise((res) => { const s = document.createElement('img'); s.src = 'https://example.com/favicon.ico?first-party'; s.onload = s.onerror = () => res(true); document.body.append(s); })`,
  );
  const count = await waitUi(
    `Number(document.querySelector('.shield .count')?.textContent || 0)`,
    "blocked count",
  );
  return `blocked ${count} request(s); first-party request allowed: ${ok}`;
});

await test("history", async () => {
  const rows = await invoke("history_search", {
    text: "example",
    limit: 5,
    offset: 0,
  });
  if (!rows.some((r) => r.url.includes("example.com")))
    throw new Error("example.com not in history");
  return `${rows.length} matching entr${rows.length === 1 ? "y" : "ies"}`;
});

await test("bookmark", async () => {
  await ui(`document.querySelector('.pill .star').click()`);
  await waitUi(`!!document.querySelector('.pill .star.on')`, "starred");
  const list = await invoke("bookmarks_list");
  if (!list.some((b) => b.url.includes("example.com")))
    throw new Error("bookmark not stored");
  return `${list.length} bookmark(s)`;
});

await test("glance", async () => {
  const t = await page("example.com");
  await clickSelector(t, "a", 1 /* Alt */);
  await waitUi(`!!document.querySelector('.frame')`, "glance frame");
  const l = await waitFor(async () => {
    const l = await layout();
    return l.chromeRaised && l.tabs.filter((x) => x.visible).length === 2
      ? l
      : null;
  }, "glance webview over raised chrome");
  await waitFor(
    async () => (await targets()).some((x) => x.url.includes("iana.org")),
    "glance page loaded",
  );
  await ui(`document.querySelector('.side .action').click()`);
  await waitFor(
    async () => !(await ui(`!!document.querySelector('.frame')`)),
    "glance closed",
  );
  return `glance webview visible with ${l.tabs.length} webviews`;
});

await test("second-tab-and-split", async () => {
  await openUrl("https://www.iana.org/help/example-domains");
  await waitFor(async () => (await tabTitles()).length >= 2, "second tab");
  await page("iana.org");
  await pageMenuItem("Split view");
  await waitUi(
    `document.querySelectorAll('.panes .card').length === 2`,
    "two panes",
  );
  const l = await waitFor(async () => {
    const l = await layout();
    const vis = l.tabs.filter((x) => x.visible);
    return vis.length === 2 ? vis : null;
  }, "two visible webviews");
  const [a, b] = l.sort((x, y) => x.x - y.x);
  if (a.x + a.w > b.x + 1) throw new Error("panes overlap");
  await pageMenuItem("Exit split view");
  await waitUi(
    `document.querySelectorAll('.panes .card').length === 1`,
    "unsplit",
  );
  return `panes ${Math.round(a.w)}px + ${Math.round(b.w)}px`;
});

await test("split-pane-close", async () => {
  // Zen draws a split pair as one group row in the sidebar; its unsplit button ends the split.
  await pageMenuItem("Split view");
  await waitUi(
    `document.querySelectorAll('.split-group .tab').length === 2`,
    "split group in the sidebar",
  );
  const [first, second] = await ui(
    `[...document.querySelectorAll('.split-group .tab')].map((t) => t.title.split('\\n')[0])`,
  );
  await ui(`document.querySelector('.split-group .unsplit').click()`);
  await waitUi(
    `!document.querySelector('.split-group') && document.querySelectorAll('.panes .card').length === 1`,
    "split closed",
  );
  await waitFor(
    async () => (await layout()).tabs.filter((x) => x.visible).length === 1,
    "one visible webview",
  );
  const active = await ui(
    `document.querySelector('.list .tab.active')?.title.split('\\n')[0]`,
  );
  if (active !== first)
    throw new Error(`expected "${first}" active, got "${active}"`);
  if (!(await tabTitles()).includes(second))
    throw new Error(
      `"${second}" was closed instead of returned to the tab list`,
    );
  return `closed the "${second}" pane; "${first}" stays active, both remain tabs`;
});

await test("drag-to-pin", async () => {
  const s = await session(await chrome());
  const from = await ui(
    `(() => { const t = document.querySelector('[data-zone=tabs] .tab'); const b = t.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2, id: t.dataset.tabId, title: t.title }; })()`,
  );
  const mouse = (type, x, y, buttons) =>
    s.send("Input.dispatchMouseEvent", {
      type,
      x,
      y,
      button: "left",
      buttons,
      clickCount: type === "mouseMoved" ? 0 : 1,
    });
  await mouse("mouseMoved", from.x, from.y, 0);
  await mouse("mousePressed", from.x, from.y, 1);
  for (let i = 1; i <= 6; i++)
    await mouse("mouseMoved", from.x + i * 2, from.y - i * 2, 1);
  const hint = await waitUi(
    `(() => { const h = document.querySelector('.pin-hint'); if (!h) return null; const b = h.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`,
    "pin drop zone",
  );
  await mouse("mouseMoved", hint.x, hint.y, 1);
  await sleep(120);
  await mouse("mouseReleased", hint.x, hint.y, 0);
  await waitUi(
    `!!document.querySelector(${JSON.stringify("[data-zone=pinned] " + tabSel(from.id))})`,
    "tab in the pinned list",
  );
  return `dragged "${from.title}" into Pinned with real mouse input`;
});

await test("sleep-and-wake", async () => {
  // Activate the iana tab, then put the example tab to sleep from its menu.
  await ui(
    `[...document.querySelectorAll('.list .tab')].find((t) => t.title.includes('IANA') || t.title.includes('Example Domains'))?.click()`,
  );
  await sleep(400);
  await ui(
    `(() => { const t = [...document.querySelectorAll('.list .tab')].find((t) => t.title === 'Example Domain'); t.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 60, clientY: 200 })); })()`,
  );
  await waitUi(`!!document.querySelector('.menu')`, "tab menu");
  await ui(
    `[...document.querySelectorAll('.menu .item')].find((b) => b.textContent.includes('Put to sleep')).click()`,
  );
  await waitUi(
    `[...document.querySelectorAll('.list .tab')].some((t) => t.title === 'Example Domain' && t.classList.contains('sleeping'))`,
    "sleeping class",
  );
  await waitFor(
    async () =>
      !(await targets()).some((x) => x.url === "https://example.com/"),
    "webview destroyed",
  );
  await ui(
    `[...document.querySelectorAll('.list .tab')].find((t) => t.title === 'Example Domain').click()`,
  );
  await page("example.com");
  await waitUi(
    `![...document.querySelectorAll('.list .tab')].some((t) => t.classList.contains('sleeping') && t.title === 'Example Domain')`,
    "awake",
  );
  return "webview freed and recreated";
});

await test("find-in-page", async () => {
  await pageMenuItem("Find in page");
  await waitUi(`!!document.getElementById('find-input')`, "find bar");
  await ui(
    `(() => { const i = document.getElementById('find-input'); i.value = 'domain'; i.dispatchEvent(new Event('input', { bubbles: true })); })()`,
  );
  const text = await waitUi(
    `(document.querySelector('.find .count')?.textContent || '').includes(' of ') && document.querySelector('.find .count').textContent.trim()`,
    "match count",
  );
  await ui(
    `document.getElementById('find-input').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`,
  );
  return text;
});

await test("permission-prompt", async () => {
  const t = await page("example.com");
  await evalIn(
    t,
    `navigator.geolocation.getCurrentPosition(() => {}, () => {}); true`,
  );
  await waitUi(`!!document.querySelector('.prompt')`, "permission prompt");
  const text = await ui(`document.querySelector('.prompt p').textContent`);
  await ui(
    `[...document.querySelectorAll('.prompt .btn')].find((b) => b.textContent === 'Block').click()`,
  );
  await waitFor(
    async () => !(await ui(`!!document.querySelector('.prompt')`)),
    "prompt dismissed",
  );
  return text.trim();
});

await test("popup-blocking-and-user-popups", async () => {
  const t = await page("example.com");
  const before = (await tabTitles()).length;
  await evalIn(t, `window.open('https://example.org/', '_blank'); true`);
  await waitUi(
    `[...document.querySelectorAll('.toast')].some((x) => x.textContent.includes('Pop-up blocked'))`,
    "popup blocked toast",
  );
  await evalIn(
    t,
    `(() => { const a = document.createElement('a'); a.id = 'zp'; a.href = 'https://example.org/'; a.target = '_blank'; a.textContent = 'popup'; a.style.cssText = 'position:fixed;left:10px;top:10px;font-size:30px'; document.body.append(a); })()`,
  );
  await clickSelector(t, "#zp");
  await waitFor(
    async () => (await tabTitles()).length === before + 1,
    "target=_blank opened as tab",
  );
  return `blocked scripted popup, opened link popup as tab (${before + 1} tabs)`;
});

await test("crash-recovery", async () => {
  await ui(
    `[...document.querySelectorAll('.list .tab')].find((t) => t.title === 'Example Domain').click()`,
  );
  const t = await pageOf(await waitFor(activeTabId, "active tab"));
  const s = await session(t);
  s.send("Page.crash").catch(() => {});
  await waitUi(
    `(document.querySelector('.state-card h2')?.textContent || '').includes('crashed')`,
    "crash card",
  );
  await ui(`document.querySelector('.state-card .btn.primary').click()`);
  await page("example.com");
  await waitFor(
    async () => !(await ui(`!!document.querySelector('.state-card h2')`)),
    "recovered",
  );
  return "crash card shown, tab reloaded";
});

await test("download", async () => {
  rmSync(downloadDir, { recursive: true, force: true });
  mkdirSync(downloadDir, { recursive: true });
  await settleStartupSettings();
  await invoke("apply_settings", {
    config: {
      downloadDir,
      recordHistory: true,
      blockPopups: true,
      blockerEnabled: true,
    },
    appearance: {
      theme: "system",
      material: "mica",
      pageBackground: [32, 32, 36],
      contentScheme: "system",
    },
  });
  const t = await pageOf(await waitFor(activeTabId, "active tab"));
  await evalIn(
    t,
    `(() => { const a = document.createElement('a'); a.id = 'zd'; a.href = 'data:text/plain,zenith-download-test'; a.download = 'zenith-test.txt'; a.textContent = 'download'; a.style.cssText = 'position:fixed;left:10px;top:60px;font-size:30px'; document.body.append(a); })()`,
  );
  await clickSelector(t, "#zd");
  await waitFor(
    () => existsSync(join(downloadDir, "zenith-test.txt")),
    "file on disk",
    15000,
  );
  const active = await waitFor(
    async () => {
      const list = await invoke("downloads_history", { limit: 5 });
      return list.find((d) => d.state === "completed") ?? null;
    },
    "download record",
    10000,
  );
  return `saved ${readdirSync(downloadDir).join(", ")} (${active.state})`;
});

await test("media-downloader", async () => {
  // A local site: a page with a <video> (a progressive file) whose player also loads an HLS stream.
  const segments = [0, 1, 2].map((i) =>
    Buffer.alloc(150_000 + i * 1000, 0x30 + i),
  );
  const routes = {
    "/watch": [
      "text/html",
      `<!doctype html><title>Zenith media test</title><body style="margin:40px"><video id="v" src="/clip.mp4" muted preload="auto" style="width:480px;height:270px;background:#000"></video><script>fetch('/hls/master.m3u8').then((r) => r.text()).then(() => fetch('/hls/v.m3u8'));</script>`,
    ],
    "/clip.mp4": ["video/mp4", Buffer.alloc(300_000, 7)],
    "/hls/master.m3u8": [
      "application/vnd.apple.mpegurl",
      "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=900000,RESOLUTION=640x360\nv.m3u8\n",
    ],
    "/hls/v.m3u8": [
      "application/vnd.apple.mpegurl",
      `#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-PLAYLIST-TYPE:VOD\n${segments.map((_, i) => `#EXTINF:4,\ns${i}.ts\n`).join("")}#EXT-X-ENDLIST\n`,
    ],
    ...Object.fromEntries(
      segments.map((b, i) => [`/hls/s${i}.ts`, ["video/mp2t", b]]),
    ),
  };
  const server = createServer((req, res) => {
    const route = routes[req.url.split("?")[0]];
    if (!route) return res.writeHead(404).end();
    const body = Buffer.isBuffer(route[1]) ? route[1] : Buffer.from(route[1]);
    const m = /bytes=(\d+)-(\d*)/.exec(req.headers.range ?? "");
    if (m) {
      const a = Number(m[1]);
      const b = m[2]
        ? Math.min(Number(m[2]), body.length - 1)
        : body.length - 1;
      res.writeHead(206, {
        "Content-Type": route[0],
        "Content-Range": `bytes ${a}-${b}/${body.length}`,
        "Content-Length": b - a + 1,
        "Accept-Ranges": "bytes",
      });
      return res.end(body.subarray(a, b + 1));
    }
    res.writeHead(200, {
      "Content-Type": route[0],
      "Content-Length": body.length,
      "Accept-Ranges": "bytes",
    });
    res.end(body);
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const base = `http://127.0.0.1:${server.address().port}`;
  let saved = "";
  try {
    rmSync(downloadDir, { recursive: true, force: true });
    mkdirSync(downloadDir, { recursive: true });
    await settleStartupSettings();
    await invoke("apply_settings", {
      config: {
        downloadDir,
        recordHistory: true,
        blockPopups: true,
        blockerEnabled: true,
        mediaButton: true,
        useFfmpeg: false,
      },
      appearance: {
        theme: "system",
        material: "mica",
        pageBackground: [32, 32, 36],
        contentScheme: "system",
      },
    });
    await openUrl(`${base}/watch`);
    const t = await page(`${base}/watch`);
    const id = await waitFor(activeTabId, "media tab id");
    // Found from network traffic alone: the <video> file and the stream (its variant folded in).
    const listed = await waitFor(
      async () => {
        const r = await invoke("media_list", { tabId: id, scan: true });
        return r.items
          .map((i) => i.kind)
          .sort()
          .join(",") === "file,hls"
          ? r
          : null;
      },
      "video file and HLS stream detected",
      15000,
    );
    const stream = listed.items.find((i) => i.kind === "hls");
    if (stream.duration !== 12)
      throw new Error(
        `stream duration ${stream.duration} (media playlist not folded into the master)`,
      );
    await waitUi(
      `!!document.querySelector('.pill .media')`,
      "download button in the address pill",
    );
    // The button over the video (trusted pointer input) opens the panel for that video.
    const v = await evalIn(
      t,
      `(() => { const b = document.getElementById('v').getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`,
    );
    await (
      await session(t)
    ).send("Input.dispatchMouseEvent", { type: "mouseMoved", x: v.x, y: v.y });
    const host = await waitFor(
      () =>
        evalIn(
          t,
          `(() => { const h = document.querySelector('zenith-media-button'); if (!h || getComputedStyle(h).display === 'none') return null; const b = h.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`,
        ),
      "download button over the video",
    );
    await click(t, host.x, host.y);
    await waitUi(
      `!!document.querySelector('.panel .item.hinted')`,
      "media panel with the clicked video highlighted",
      10000,
    );
    // Download the stream from the panel; segments must arrive complete and in order.
    await ui(
      `[...document.querySelectorAll('.panel .item')].find((i) => i.innerText.includes('HLS')).querySelector('.go').click()`,
    );
    const expected = Buffer.concat(segments);
    const record = await waitFor(
      async () =>
        (await invoke("downloads_history", { limit: 10 })).find(
          (d) => d.url === `${base}/hls/master.m3u8` && d.state !== "progress",
        ),
      "stream download finished",
      20000,
    );
    saved = record.path;
    if (record.state !== "completed")
      throw new Error(`stream download ${record.state}`);
    if (!saved.startsWith(downloadDir))
      throw new Error(`saved outside the configured folder: ${saved}`);
    if (!saved.endsWith(".ts"))
      throw new Error(`unexpected container: ${saved}`);
    if (!readFileSync(saved).equals(expected))
      throw new Error("the saved stream differs from its segments");
    await ui(
      `document.querySelector('.panel header .icon-btn:last-child').click()`,
    );
    return `listed ${listed.items.map((i) => i.kind).join(" + ")}; in-page button opened the panel; saved "${saved.split(/[\\/]/).pop()}" (${expected.length} bytes, exact)`;
  } finally {
    server.close();
    // Never leave test output behind (e.g. in the user's Downloads folder after a settings race).
    if (saved && existsSync(saved)) rmSync(saved, { force: true });
  }
});

await test("extension-loading", async () => {
  rmSync(extDir, { recursive: true, force: true });
  mkdirSync(extDir, { recursive: true });
  writeFileSync(
    join(extDir, "manifest.json"),
    JSON.stringify({
      manifest_version: 3,
      name: "Zenith E2E Marker",
      version: "1.0",
      description: "Marks pages so the test can see content scripts run.",
      content_scripts: [
        { matches: ["<all_urls>"], js: ["mark.js"], run_at: "document_start" },
      ],
    }),
  );
  writeFileSync(
    join(extDir, "mark.js"),
    `document.documentElement.dataset.zenithExt = '1';`,
  );
  const ext = await invoke("ext_install_folder", { path: extDir });
  if (!ext?.id) throw new Error("install returned nothing");
  await openUrl("https://example.net/");
  const t = await page("example.net");
  const marked = await waitFor(
    () => evalIn(t, `document.documentElement.dataset.zenithExt === '1'`),
    "content script ran",
    15000,
  );
  const list = await invoke("ext_list");
  await invoke("ext_remove", { id: ext.id });
  return `installed ${ext.name} (${ext.id.slice(0, 8)}…), content script ran: ${marked}, listed: ${list.length}`;
});

await test("compact-mode", async () => {
  await ui(`document.querySelector('.sidebar [data-action=compact]').click()`);
  await waitUi(
    `document.querySelector('.app').classList.contains('compact')`,
    "compact class",
  );
  const l = await waitFor(async () => {
    const l = await layout();
    const v = l.tabs.find((x) => x.visible);
    return v && v.x < 20 ? v : null;
  }, "webview fills window");
  await ui(
    `document.querySelector('.hot-left').dispatchEvent(new MouseEvent('mouseenter'))`,
  );
  await waitUi(
    `!!document.querySelector('.flyout .sidebar')`,
    "floating sidebar",
  );
  await waitFor(
    async () => (await layout()).chromeRaised,
    "flyout floats over page",
  );
  await ui(
    `document.querySelector('.flyout').dispatchEvent(new MouseEvent('mouseleave'))`,
  );
  await waitFor(
    async () => !(await ui(`!!document.querySelector('.flyout')`)),
    "flyout hidden",
  );
  await ui(`(() => { const e = new KeyboardEvent('keydown'); })()`);
  // Leave compact mode through the settings store via the toggle in the floating sidebar.
  await ui(
    `document.querySelector('.hot-left').dispatchEvent(new MouseEvent('mouseenter'))`,
  );
  await waitUi(
    `!!document.querySelector('.flyout [data-action=compact]')`,
    "floating sidebar again",
  );
  await ui(`document.querySelector('.flyout [data-action=compact]').click()`);
  await waitUi(
    `!document.querySelector('.app').classList.contains('compact')`,
    "compact off",
  );
  return `compact webview at x=${Math.round(l.x)}, w=${Math.round(l.w)}`;
});

await test("layout-animation", async () => {
  // Compact mode (and every layout switch) moves the page smoothly instead of jumping.
  const visible = async () => (await layout()).tabs.find((x) => x.visible);
  const settled = () =>
    waitFor(async () => {
      const a = await visible();
      await sleep(150);
      const b = await visible();
      return a && b && a.x === b.x && a.w === b.w ? b : null;
    }, "page at rest");
  const start = await settled();
  await ui(`document.querySelector('.sidebar [data-action=compact]').click()`);
  const xs = [];
  const until = Date.now() + 600;
  while (Date.now() < until) {
    const v = (await layout()).tabs.find((x) => x.visible);
    if (v) xs.push(Math.round(v.x));
    await sleep(15);
  }
  const end = xs[xs.length - 1];
  const between = xs.filter((x) => x < start.x - 4 && x > end + 4);
  // Back to the normal layout.
  await ui(
    `document.querySelector('.hot-left').dispatchEvent(new MouseEvent('mouseenter'))`,
  );
  await waitUi(
    `!!document.querySelector('.flyout [data-action=compact]')`,
    "floating sidebar",
  );
  await ui(`document.querySelector('.flyout [data-action=compact]').click()`);
  await waitUi(
    `!document.querySelector('.app').classList.contains('compact')`,
    "compact off",
  );
  const back = await settled();
  if (Math.abs(back.x - start.x) > 2)
    throw new Error(`page came back at x=${back.x}, started at ${start.x}`);
  if (end > 20)
    throw new Error(`page did not reach the window edge (x=${end})`);
  if (!between.length)
    throw new Error(`no intermediate frames: ${xs.join(",")}`);
  return `page moved ${Math.round(start.x)} → ${end} through ${between.length} intermediate frames`;
});

await test("narrow-multiple-toolbars", async () => {
  await openSettings();
  await ui(
    `[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Multiple toolbars').click()`,
  );
  await waitUi(
    `!!document.querySelector('.topbar [data-action=app-menu]')`,
    "toolbar above the page",
  );
  // A web page (the settings page has no webview).
  await openUrl("https://example.com/?narrow-e2e");
  await page("narrow-e2e");
  try {
    resizeWindow(560, 640);
    await sleep(800);
    const r = await ui(`(() => {
      const w = innerWidth;
      const inView = (sel) => { const e = document.querySelector(sel); if (!e) return null; const b = e.getBoundingClientRect(); return b.right <= w + 0.5 && b.left >= 0 && b.width > 0; };
      const bar = document.querySelector('.topbar');
      const stage = document.querySelector('.stage').getBoundingClientRect();
      const items = [...bar.children].map((c) => { const b = c.getBoundingClientRect(); return (c.getAttribute('class') || c.tagName).split(' ')[0] + '@' + Math.round(b.left) + '+' + Math.round(b.width); });
      return { w, menu: inView('.topbar [data-action=app-menu]'), close: inView('.topbar .ctl.close'), overflow: !!document.querySelector('.topbar [data-action=overflow]'), fits: bar.scrollWidth <= bar.clientWidth + 1, scroll: bar.scrollWidth, client: bar.clientWidth, stageRight: Math.round(w - stage.right), stageW: Math.round(stage.width), items };
    })()`);
    if (!r.menu || !r.close || !r.fits)
      throw new Error(`toolbar cut off at ${r.w}px: ${JSON.stringify(r)}`);
    const v = await waitFor(
      async () => (await layout()).tabs.find((x) => x.visible),
      "visible page",
    );
    if (Math.abs(v.x + v.w - (r.w - r.stageRight)) > 2)
      throw new Error(
        `page not aligned with its pane: ${JSON.stringify({ v, r })}`,
      );
    return `${r.w}px wide: menu and window controls visible, overflow menu ${r.overflow ? "shown" : "not needed"}, page ${r.stageW}px`;
  } finally {
    resizeWindow(1280, 820);
    await sleep(500);
    await ui(`document.querySelector('.list .tab.active .close')?.click()`);
    await ui(`document.querySelector('[data-action=app-menu]').click()`);
    await waitUi(`!!document.querySelector('.menu')`, "app menu");
    await ui(
      `[...document.querySelectorAll('.menu .item')].find((b) => b.textContent.includes('Settings')).click()`,
    );
    await waitUi(
      `!!document.querySelector('#settings-tabs select')`,
      "settings page",
    );
    await ui(
      `[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Single toolbar').click()`,
    );
    await waitUi(`!document.querySelector('.topbar')`, "single toolbar again");
  }
});

await test("incognito-window", async () => {
  const t0 = Date.now();
  await invoke("incognito_open", {
    urls: ["https://example.com/?incognito-e2e"],
  });
  const inc = await waitFor(
    async () => (await incognitoChromes())[0],
    "incognito chrome UI",
    15000,
  );
  await waitFor(
    () => evalIn(inc, `!!document.querySelector('.app.incognito .sidebar')`),
    "incognito UI ready",
    15000,
  );
  const readyMs = Date.now() - t0;
  const p = await page("incognito-e2e");
  await waitFor(
    () =>
      evalIn(
        p,
        `location.href.startsWith('https://example.com/') && document.readyState === 'complete'`,
      ),
    "incognito page loaded",
    20000,
  );
  // Separate cookie jar from the normal window: the cookie lands in the InPrivate jar only (the
  // normal window's chrome webview uses the user's profile).
  await evalIn(p, `document.cookie = 'zinc=1; path=/'`);
  const jar = async (t) =>
    (
      (await (await session(t)).send("Network.getCookies", { urls: ["https://example.com/"] })).result?.cookies ?? []
    ).some((c) => c.name === "zinc");
  if (!(await jar(p))) throw new Error("the incognito page could not set a cookie");
  const leaked = await jar(await chrome());
  // The incognito window's page is not recorded in history.
  const hist = await invoke("history_search", {
    text: "incognito-e2e",
    limit: 10,
    offset: 0,
  });
  // Close the incognito window alone; the normal window stays.
  await evalIn(
    inc,
    `window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: window.__TAURI_INTERNALS__.metadata.currentWindow.label })`,
  ).catch(() => {});
  await waitFor(
    async () => (await incognitoChromes()).length === 0,
    "incognito window closed",
    10000,
  );
  await waitFor(
    async () => !(await targets()).some((x) => x.url.includes("incognito-e2e")),
    "incognito page freed",
    10000,
  );
  const alive = await ui(`!!document.querySelector('.sidebar .new-tab')`);
  await ui(`document.querySelector('.list .tab.active .close')?.click()`);
  if (leaked) throw new Error("incognito cookie visible in the normal window");
  if (hist.length) throw new Error("incognito page recorded in history");
  if (!alive) throw new Error("normal window went away");
  return `incognito window ready in ${readyMs} ms (same process), cookies isolated, no history; closed alone`;
});

await test("spaces", async () => {
  await ui(`document.querySelector('.space-dot.add').click()`);
  await waitUi(`!!document.querySelector('.dialog input')`, "space editor");
  await ui(
    `(() => { const i = document.querySelector('.dialog input'); i.value = 'Work'; i.dispatchEvent(new Event('input', { bubbles: true })); document.querySelector('.dialog').requestSubmit(); })()`,
  );
  await waitUi(
    `document.querySelector('.space-name')?.textContent.includes('Work')`,
    "Work space active",
  );
  const tabs = await tabTitles();
  await ui(`document.querySelector('.space-dot').click()`);
  await waitUi(
    `document.querySelector('.space-name')?.textContent.includes('Home')`,
    "back to Home",
  );
  return `Work space had ${tabs.length} tabs; ${await ui(`document.querySelectorAll('.space-dot:not(.add)').length`)} spaces`;
});

await test("essentials", async () => {
  await openUrl("https://example.com/?essential-check");
  await page("essential-check");
  const id = await waitFor(activeTabId, "tab id");
  await ui(
    `document.querySelector(${JSON.stringify(tabSel(id))}).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 60, clientY: 200 }))`,
  );
  await waitUi(`!!document.querySelector('.menu')`, "tab menu");
  await ui(
    `[...document.querySelectorAll('.menu .item')].find((b) => b.textContent.includes('Add to Essentials')).click()`,
  );
  const sel = JSON.stringify(`.essentials [data-tab-id="${id}"]`);
  await waitUi(
    `!!document.querySelector(${sel})`,
    "tab in the Essentials grid",
  );
  const spaces = await ui(
    `document.querySelectorAll('.space-dot:not(.add)').length`,
  );
  if (spaces > 1) {
    await ui(`document.querySelectorAll('.space-dot:not(.add)')[1].click()`);
    await sleep(400);
    await waitUi(
      `!!document.querySelector(${sel})`,
      "Essentials shown in the other space",
    );
    await ui(`document.querySelectorAll('.space-dot:not(.add)')[0].click()`);
    await sleep(400);
  }
  return `tab added to Essentials${spaces > 1 ? " and visible in every space" : ""}`;
});

await test("zoom", async () => {
  const marker = `zoom-check-${Date.now()}`;
  await openUrl(`https://example.com/?${marker}`);
  const t = await page(marker);
  const id = await waitFor(activeTabId, "tab id");
  const before = await evalIn(t, "window.devicePixelRatio");
  const set = await invoke("tab_command", { id, cmd: "zoom", arg: 1.5 });
  await waitUi(
    `document.querySelector('.topbar .zoom')?.textContent === '150%'`,
    "zoom badge shows 150%",
  ).catch(async (e) => {
    const dump = await ui(
      `JSON.stringify({ active: document.querySelector('.list .tab.active')?.title, badge: document.querySelector('.topbar .zoom')?.textContent ?? null, buttons: [...document.querySelectorAll('.topbar .actions > *')].map((b) => b.className) })`,
    );
    throw new Error(`${e.message}; set=${set}; id=${id}; ${dump}`);
  });
  const after = await waitFor(
    async () => {
      const r = await evalIn(t, "window.devicePixelRatio");
      return r > before * 1.4 ? r : null;
    },
    "page re-rendered at 150%",
    5000,
  );
  await ui(`document.querySelector('.topbar .zoom').click()`);
  await waitUi(
    `!document.querySelector('.topbar .zoom')`,
    "zoom reset from the badge",
  );
  return `150% zoom applied (devicePixelRatio ${before} -> ${after}) and reset from the badge`;
});

await test("download-exempt", async () => {
  // A control tab that should go to sleep, and a tab with a (paused) download that must not.
  await openUrl("https://example.org/?sleep-control");
  await page("sleep-control");
  const controlId = await waitFor(activeTabId, "control tab id");
  await openUrl("https://example.com/?download-exempt");
  const t = await page("download-exempt");
  const id = await waitFor(activeTabId, "download tab id");
  await evalIn(
    t,
    `(() => { const a = document.createElement('a'); a.id = 'zbig'; a.href = 'https://proof.ovh.net/files/100Mb.dat'; a.textContent = 'big file'; a.style.cssText = 'position:fixed;left:10px;top:110px;font-size:30px'; document.body.append(a); })()`,
  );
  await clickSelector(t, "#zbig");
  const dl = await waitFor(
    async () =>
      (await invoke("downloads_active")).find(
        (d) => d.state === "progress" && d.tabId === id,
      ),
    "download started",
    20000,
  );
  await invoke("download_action", { id: dl.id, action: "pause" });
  await ui(`document.querySelector('.new-tab').click()`);
  await waitUi(`!!document.querySelector('.palette input')`, "palette");
  await typeInPalette("");
  const start = Date.now();
  await waitUi(
    `!!document.querySelector(${JSON.stringify(tabSel(controlId, ".sleeping"))})`,
    "control tab asleep",
    150000,
  );
  const slept = Math.round((Date.now() - start) / 1000);
  const downloadTabAsleep = await ui(
    `!!document.querySelector(${JSON.stringify(tabSel(id, ".sleeping"))})`,
  );
  await invoke("download_action", { id: dl.id, action: "cancel" });
  if (downloadTabAsleep)
    throw new Error("the tab with a download in progress was put to sleep");
  return `control tab slept after ~${slept}s; the tab with a download in progress stayed awake (download then canceled)`;
}, 200000);

await test("shortcut-editor", async () => {
  await openSettings();
  const row = `[...document.querySelectorAll('#settings-shortcuts .row')].find((r) => r.querySelector('.label')?.textContent === 'Duplicate tab')`;
  const key = (code, ctrl = false, shift = false) =>
    ui(
      `window.dispatchEvent(new KeyboardEvent('keydown', { code: '${code}', key: '${code === "Escape" ? "Escape" : "x"}', ctrlKey: ${ctrl}, shiftKey: ${shift}, bubbles: true, cancelable: true }))`,
    );
  const message = () =>
    ui(
      `document.querySelector('#settings-shortcuts .message')?.textContent ?? ''`,
    );
  // Record a new chord.
  await ui(`${row}.querySelector('.add').click()`);
  await waitUi(`!!${row}.querySelector('.listening')`, "recording");
  await key("KeyY", true, true);
  await waitUi(
    `!${row}.querySelector('.listening') && [...${row}.querySelectorAll('.kbd')].some((k) => k.textContent === 'Y')`,
    "Ctrl+Shift+Y listed",
  );
  // A chord owned by another action needs confirmation.
  await ui(`${row}.querySelector('.add').click()`);
  await key("KeyT", true);
  const conflict = await waitFor(
    async () => ((await message()).includes("New tab") ? message() : null),
    "conflict warning",
  );
  // Editing keys are refused.
  await key("KeyC", true);
  const reserved = await waitFor(
    async () => ((await message()).includes("Reserved") ? message() : null),
    "reserved-key warning",
  );
  await key("Escape");
  await waitUi(`!${row}.querySelector('.listening')`, "recording cancelled");
  // Settings are saved with a short debounce.
  await waitFor(
    async () =>
      (
        JSON.parse(await invoke("settings_load")).shortcuts?.duplicateTab ?? []
      ).includes("Ctrl+Shift+Y"),
    "binding saved",
    5000,
  );
  return `recorded Ctrl+Shift+Y and saved it; conflict: "${conflict.trim()}"; reserved: "${reserved.trim()}"`;
});

await test("search-suggestions", async () => {
  const g = await invoke(
    "search_suggest",
    { engine: "google", query: "rust lang" },
    20000,
  );
  const y = await invoke(
    "search_suggest",
    { engine: "yandex", query: "rust lang" },
    20000,
  );
  if (!g.length) throw new Error("no Google suggestions");
  return `Google: ${g.slice(0, 3).join(" / ")}; Yandex: ${y.length} suggestions`;
});

await test("memory-stats", async () => {
  const m = await invoke("memory_stats");
  return `${m.processes.length} processes, ${Math.round(m.workingSet / 1048576)} MB working set, ${Math.round(m.privateBytes / 1048576)} MB private`;
});

await test("store-extensions", async () => {
  const notes = [];
  const installed = [];
  for (const [name, id] of [
    ["Dark Reader", "eimadpbcbfnmbkopoojfekhnkhdbieeh"],
    ["uBlock Origin Lite", "ddkjiahejlhfcafbddmgiahcphecmpfh"],
  ]) {
    try {
      const ext = await invoke(
        "ext_install_store",
        { input: `https://chromewebstore.google.com/detail/${id}` },
        90000,
      );
      installed.push(ext);
      notes.push(
        `${ext.name} ${ext.version} installed (enabled: ${ext.enabled})`,
      );
    } catch (e) {
      notes.push(`${name}: ${e?.message ?? e}`);
    }
  }
  if (!installed.length) throw new Error(notes.join("; "));
  await openUrl("https://example.com/?extension-check");
  const t = await page("extension-check");
  const dark = await waitFor(
    () =>
      evalIn(t, `!!document.querySelector('.darkreader, style.darkreader')`),
    "Dark Reader styles",
    20000,
  ).catch(() => false);
  notes.push(`Dark Reader restyled the page: ${dark}`);
  const all = await invoke("ext_list");
  const builtin = all.filter((e) => e.builtin);
  const refused = builtin.length
    ? await invoke("ext_set_enabled", {
        id: builtin[0].id,
        enabled: false,
      }).then(
        () => false,
        () => true,
      )
    : true;
  if (!refused)
    throw new Error(
      `a built-in extension (${builtin[0].name}) could be disabled`,
    );
  for (const ext of installed) await invoke("ext_remove", { id: ext.id });
  notes.push(
    `${all.length - builtin.length} user + ${builtin.length} built-in listed; built-ins protected; removed again`,
  );
  return notes.join("; ");
}, 240000);

await test("sleep-timeout-scroll-audio", async () => {
  // Sleep background tabs after 1 minute (Settings > Tabs & memory).
  await openSettings();
  await ui(
    `(() => { const s = document.querySelector('#settings-tabs select'); s.value = '1'; s.dispatchEvent(new Event('change', { bubbles: true })); })()`,
  );

  // A long page, scrolled down.
  await openUrl("https://www.rfc-editor.org/rfc/rfc2606.html");
  const longPage = await page("rfc2606");
  const longId = await waitFor(activeTabId, "long page tab id");
  await waitFor(
    () =>
      evalIn(
        longPage,
        `document.readyState === 'complete' && document.documentElement.scrollHeight > 2500`,
      ),
    "long page loaded",
    20000,
  );
  await evalIn(longPage, `window.scrollTo(0, 1500)`);
  const scrolled = await evalIn(longPage, `Math.round(window.scrollY)`);

  // A muted tab that plays (inaudible) audio: exempt from sleeping.
  await openUrl("https://example.org/?audio-check");
  const audioPage = await page("audio-check");
  const audioId = await waitFor(activeTabId, "audio tab id");
  await ui(
    `document.querySelector(${JSON.stringify(tabSel(audioId))}).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 60, clientY: 200 }))`,
  );
  await waitUi(`!!document.querySelector('.menu')`, "tab menu");
  await ui(
    `[...document.querySelectorAll('.menu .item')].find((b) => b.textContent.includes('Mute tab')).click()`,
  );
  await evalIn(
    audioPage,
    `(() => { const b = document.createElement('button'); b.id = 'zaudio'; b.textContent = 'play'; b.style.cssText = 'position:fixed;left:10px;top:10px;font-size:30px;z-index:9'; b.onclick = () => { const ctx = new AudioContext(); const o = ctx.createOscillator(); const g = ctx.createGain(); g.gain.value = 0.001; o.connect(g).connect(ctx.destination); o.start(); window.__zctx = ctx; }; document.body.append(b); })()`,
  );
  await clickSelector(audioPage, "#zaudio");
  await waitUi(
    `!!document.querySelector(${JSON.stringify(tabSel(audioId, "[data-audible]"))})`,
    "tab reported as playing audio",
    15000,
  );

  // Move both to the background with a new-tab page.
  await ui(`document.querySelector('.new-tab').click()`);
  await waitUi(`!!document.querySelector('.palette input')`, "palette");
  await typeInPalette("");
  const hiddenAt = Date.now();
  const timeline = [];
  const state = (id) =>
    ui(
      `(() => { const t = document.querySelector(${JSON.stringify(tabSel(id))}); return t ? (t.classList.contains('sleeping') ? 'asleep' : 'awake') + (t.dataset.audible ? '+audio' : '') : 'gone'; })()`,
    );
  let longState = "";
  while (Date.now() - hiddenAt < 150000) {
    longState = await state(longId);
    const audioState = await state(audioId);
    const entry = `${Math.round((Date.now() - hiddenAt) / 1000)}s long=${longState} audio=${audioState}`;
    if (
      timeline[timeline.length - 1]?.split(" ").slice(1).join(" ") !==
      entry.split(" ").slice(1).join(" ")
    )
      timeline.push(entry);
    if (longState.startsWith("asleep")) break;
    await sleep(2000);
  }
  if (!longState.startsWith("asleep"))
    throw new Error(`long page never slept: ${timeline.join("; ")}`);
  const sleptAfter = Math.round((Date.now() - hiddenAt) / 1000);
  if (!(await state(audioId)).startsWith("awake"))
    throw new Error(`the audio tab was put to sleep: ${timeline.join("; ")}`);
  if ((await targets()).some((x) => x.url.includes("rfc2606")))
    throw new Error("sleeping tab still has a webview");
  if (!(await targets()).some((x) => x.url.includes("audio-check")))
    throw new Error("audio tab lost its webview");

  // Wake: the page reloads at its old scroll position.
  await ui(`document.querySelector(${JSON.stringify(tabSel(longId))}).click()`);
  const woke = await page("rfc2606");
  const restored = await waitFor(
    async () => {
      const y = await evalIn(
        woke,
        `document.readyState === 'complete' ? Math.round(window.scrollY) : -1`,
      );
      return y > 0 ? y : null;
    },
    "scroll position restored",
    20000,
  );
  if (Math.abs(restored - scrolled) > 60)
    throw new Error(`scrolled to ${scrolled}, restored ${restored}`);
  await ui(
    `document.querySelector(${JSON.stringify(tabSel(audioId, " .close"))}).click()`,
  );
  const load = await invoke("system_memory_load");
  const why =
    load >= 88
      ? `early, by the memory-pressure rule (system memory ${load}% in use)`
      : "by the 1-minute timeout";
  return `background tab slept after ~${sleptAfter}s ${why} and woke at scrollY ${restored} (was ${scrolled}); audio tab stayed awake`;
}, 240000);

if (!skipRestart) {
  await test("session-restore", async () => {
    const before = (await tabTitles()).sort();
    await sleep(800);
    await ui(
      `window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' })`,
    ).catch(() => {});
    await appExited();
    await relaunch();
    const after = await waitFor(
      async () => {
        const t = (await tabTitles()).sort();
        return t.length === before.length ? t : null;
      },
      "restored tabs",
      10000,
    );
    return `restored ${after.length} tabs: ${after.join(", ")}`;
  }, 90000);

  await test("abrupt-restore", async () => {
    const before = (await tabTitles()).sort();
    const counts = async () => ({
      bookmarks: (await invoke("bookmarks_list")).length,
      history: (
        await invoke("history_search", { text: "", limit: 500, offset: 0 })
      ).length,
      downloads: (await invoke("downloads_history", { limit: 100 })).length,
    });
    const c1 = await counts();
    await sleep(1200); // the session snapshot lands ~400 ms after the last change
    spawnSync(
      "powershell",
      [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        "scripts/dev/run.ps1",
        "-Stop",
      ],
      { stdio: "ignore" },
    );
    await appExited();
    await relaunch();
    const after = await waitFor(
      async () => {
        const t = (await tabTitles()).sort();
        return t.length === before.length ? t : null;
      },
      "restored tabs",
      15000,
    );
    const c2 = await counts();
    for (const k of Object.keys(c1))
      if (c2[k] < c1[k])
        throw new Error(`${k}: ${c1[k]} before the kill, ${c2[k]} after`);
    return `force-killed; restored ${after.length} tabs, ${c2.bookmarks} bookmarks, ${c2.history} history entries, ${c2.downloads} downloads`;
  }, 90000);
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
writeFileSync(resultsFile, JSON.stringify(results, null, 2));
for (const s of sessions.values()) s.close();
process.exit(failed.length ? 1 : 0);
