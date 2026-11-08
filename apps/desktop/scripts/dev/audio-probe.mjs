// Probe: how does WebView2 report a muted tab that plays audio once it is in the background?
// Opens a page, mutes it, starts an inaudible tone, switches away, then samples the tab's
// audible flag (as shown in the sidebar) and the AudioContext state for ~70 s.
const PORT = Number(process.env.ZENITH_REMOTE_DEBUG_PORT || 9444);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const list = async () =>
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
    new Promise(
      (r) => (
        pending.set(++id, r),
        ws.send(JSON.stringify({ id, method, params }))
      ),
    );
  const ev = async (expression) =>
    (
      await send("Runtime.evaluate", {
        expression,
        awaitPromise: true,
        returnByValue: true,
      })
    ).result?.result?.value;
  return { send, ev, close: () => ws.close() };
}

const chromeT = (await list()).find((t) => t.url.includes("tauri.localhost"));
const ui = await connect(chromeT);
const muted = process.argv.includes("--unmuted") ? false : true;

await ui.ev(
  `(async () => { document.querySelector('.new-tab').click(); await new Promise(r => setTimeout(r, 300)); const i = document.querySelector('.palette input'); i.value = 'https://example.org/?audio-probe'; i.dispatchEvent(new Event('input', { bubbles: true })); await new Promise(r => setTimeout(r, 450)); i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); })()`,
);
let pageT;
for (let i = 0; i < 40 && !pageT; i++) {
  await sleep(250);
  pageT = (await list()).find((t) => t.url.includes("audio-probe"));
}
await sleep(1500);
const tabId = await ui.ev(
  `document.querySelector('.list .tab.active')?.dataset.tabId`,
);
if (muted)
  await ui.ev(
    `window.__TAURI_INTERNALS__.invoke('tab_command', { id: ${JSON.stringify(tabId)}, cmd: 'mute', arg: null })`,
  );
const page = await connect(pageT);
await page.ev(
  `(() => { const b = document.createElement('button'); b.id = 'zaudio'; b.textContent = 'play'; b.style.cssText = 'position:fixed;left:10px;top:10px;font-size:30px;z-index:9'; b.onclick = () => { const ctx = new AudioContext(); const o = ctx.createOscillator(); const g = ctx.createGain(); g.gain.value = 0.001; o.connect(g).connect(ctx.destination); o.start(); window.__zctx = ctx; }; document.body.append(b); })()`,
);
const r = await page.ev(
  `(() => { const b = document.getElementById('zaudio').getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`,
);
for (const type of ["mouseMoved", "mousePressed", "mouseReleased"]) {
  await page.send("Input.dispatchMouseEvent", {
    type,
    x: r.x,
    y: r.y,
    button: "left",
    clickCount: 1,
  });
}
const sample = async (label) => {
  const audible = await ui.ev(
    `!!document.querySelector('.tab[data-tab-id="${tabId}"][data-audible]')`,
  );
  const state = await page
    .ev(`window.__zctx?.state ?? 'none'`)
    .catch(() => "gone");
  console.log(`${label.padEnd(14)} audible=${audible} ctx=${state}`);
};
for (let i = 0; i < 3; i++)
  (await sleep(1000), await sample(`foreground ${i + 1}s`));
// Background it: open a new-tab page.
await ui.ev(
  `(async () => { document.querySelector('.new-tab').click(); await new Promise(r => setTimeout(r, 300)); const i = document.querySelector('.palette input'); i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); })()`,
);
for (let s = 2; s <= 70; s += 4)
  (await sleep(4000), await sample(`background ${s}s`));
await page.ev(`window.__zctx?.close()`);
ui.close();
page.close();
