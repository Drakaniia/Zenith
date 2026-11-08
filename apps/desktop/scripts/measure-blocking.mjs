// Ad/tracker blocking reference measurement (plan 4.10 / section 11).
//
// Runs against the isolated test instance (scripts/dev/run.ps1). Requests are counted where
// Zenith blocks them, in WebView2's WebResourceRequested event, so every request of the page
// is seen: all frames (including cross-site ad iframes), workers and the document itself.
// For each reference site:
//   1. open it and let it settle (warms the cache),
//   2. reload with blocking on and count requests made / blocked,
//   3. turn blocking off for the site with the URL-pill shield (reloads) and count again,
//   4. turn blocking back on.
// Writes measurements/blocking.md.
//   node scripts/measure-blocking.mjs [--settle 15]
import { writeFileSync, mkdirSync } from "node:fs";

const PORT = Number(process.env.ZENITH_REMOTE_DEBUG_PORT || 9444);
const args = process.argv.slice(2);
const SETTLE =
  Number(args.includes("--settle") ? args[args.indexOf("--settle") + 1] : 15) *
  1000;
const SITES = [
  "https://edition.cnn.com/",
  "https://www.theguardian.com/international",
  "https://www.bbc.com/news",
  "https://www.imdb.com/",
  "https://www.nytimes.com/",
  "https://www.reddit.com/",
];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = async () =>
  (await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json()).filter(
    (t) => t.type === "page",
  );

async function connect(t) {
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((res, rej) => ((ws.onopen = res), (ws.onerror = rej)));
  let id = 0;
  const pending = new Map();
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) (pending.get(m.id)(m), pending.delete(m.id));
  };
  const send = (method, params = {}) =>
    new Promise((r) => {
      pending.set(++id, r);
      ws.send(JSON.stringify({ id, method, params }));
      setTimeout(() => r({}), 20000);
    });
  const ev = async (expression) =>
    (
      await send("Runtime.evaluate", {
        expression,
        awaitPromise: true,
        returnByValue: true,
      })
    ).result?.result?.value;
  return { ev, close: () => ws.close() };
}

const ui = await connect(
  (await targets()).find((t) => t.url.includes("tauri.localhost")),
);
const invoke = (cmd, a = {}) =>
  ui.ev(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(a)})`,
  );
const counts = async (id) => {
  const l = await invoke("debug_layout");
  const t = l?.tabs.find((x) => x.id === id);
  return t ? { requests: t.requests, blocked: t.blocked } : null;
};
const stats = await invoke("blocker_stats");

const rows = [];
for (const site of SITES) {
  const host = new URL(site).hostname.replace(/^(www|edition)\./, "");
  try {
    const previous = await ui.ev(
      `document.querySelector('.list .tab.active')?.dataset.tabId ?? ''`,
    );
    await ui.ev(
      `(async () => { document.querySelector('.new-tab').click(); await new Promise(r => setTimeout(r, 300)); const i = document.querySelector('.palette input'); i.value = ${JSON.stringify(site)}; i.dispatchEvent(new Event('input', { bubbles: true })); await new Promise(r => setTimeout(r, 450)); i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); })()`,
    );
    let id = "";
    for (let i = 0; i < 40 && (!id || id === previous); i++) {
      await sleep(250);
      id = await ui.ev(
        `document.querySelector('.list .tab.active')?.dataset.tabId ?? ''`,
      );
    }
    if (!id || id === previous) throw new Error("the new tab did not open");
    await sleep(SETTLE);
    await invoke("tab_command", { id, cmd: "reload", arg: null });
    await sleep(SETTLE);
    const on = await counts(id);
    await ui.ev(`document.querySelector('.pill .shield').click()`); // blocking off for this site, reloads
    await sleep(SETTLE);
    const off = await counts(id);
    await ui.ev(`document.querySelector('.pill .shield').click()`); // back on
    await sleep(1500);
    await ui.ev(`document.querySelector('.list .tab.active .close')?.click()`);
    if (!on || !off) throw new Error("tab disappeared");
    rows.push({ site: host, on, off });
    console.log(
      `${host}: blocking on ${on.requests} requests (${on.blocked} blocked) | blocking off ${off.requests} requests (${off.blocked} blocked)`,
    );
  } catch (e) {
    console.log(`${host}: ${e.message}`);
    rows.push({ site: host, error: e.message });
  }
}

const pct = (part, whole) => (whole ? Math.round((part / whole) * 100) : 0);
const ok = rows.filter((r) => !r.error);
const reached = (r) => r.on.requests - r.on.blocked;
const lines = [
  "# Ad and tracker blocking: reference measurement",
  "",
  `Measured ${new Date().toISOString().slice(0, 10)} on Windows 11 with the built-in list plus EasyList and EasyPrivacy (${stats.rules.toLocaleString("en-US")} rules), ${SETTLE / 1000} s after each load, no consent banners accepted.`,
  "",
  'Requests are counted in WebView2\'s `WebResourceRequested` event, where Zenith blocks them, so every request of the page is included (all frames, workers, the document). "Off" is the same page reloaded with blocking turned off for that site via the shield in the address pill.',
  "",
  "| Site | Requests, blocking off | Requests, blocking on | Blocked | Requests that reached the network |",
  "|---|---|---|---|---|",
  ...rows.map((r) =>
    r.error
      ? `| ${r.site} | error: ${r.error} | | | |`
      : `| ${r.site} | ${r.off.requests} | ${r.on.requests} | ${r.on.blocked} | ${reached(r)} (−${100 - pct(reached(r), r.off.requests)}% vs off) |`,
  ),
  "",
];
if (ok.length) {
  const offTotal = ok.reduce((a, r) => a + r.off.requests, 0);
  const reachedTotal = ok.reduce((a, r) => a + reached(r), 0);
  const blockedTotal = ok.reduce((a, r) => a + r.on.blocked, 0);
  lines.push(
    `Across ${ok.length} sites: ${offTotal} requests without blocking; with blocking ${blockedTotal} were blocked and ${reachedTotal} reached the network (−${100 - pct(reachedTotal, offTotal)}%).`,
    'Blocked scripts also never start the requests they would have made, which is why "blocking on" pages usually make fewer requests in total.',
    "",
    "Numbers vary between runs because these sites rotate ads and content.",
    "",
  );
}
mkdirSync("measurements", { recursive: true });
writeFileSync("measurements/blocking.md", lines.join("\n"));
console.log("wrote measurements/blocking.md");
ui.close();
