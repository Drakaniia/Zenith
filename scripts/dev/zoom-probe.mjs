// Probe: does WebView2 report user zoom (Ctrl+wheel on the page) to the chrome UI?
const PORT = Number(process.env.ZENITH_REMOTE_DEBUG_PORT || 9444);
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
const all = await list();
const ui = await connect(all.find((t) => t.url.includes("tauri.localhost")));
const page = await connect(
  all.find((t) => t.url.includes(process.argv[2] ?? "zoom-check")),
);
await ui.ev(
  `(async () => { const T = window.__TAURI_INTERNALS__; window.__zlog = []; await T.invoke('plugin:event|listen', { event: 'tab-update', target: { kind: 'Any' }, handler: T.transformCallback((e) => { if (e.payload && e.payload.zoom !== undefined) window.__zlog.push(e.payload.zoom); }) }); })()`,
);
const before = await page.ev("innerWidth");
for (let i = 0; i < 2; i++) {
  await page.send("Input.dispatchMouseEvent", {
    type: "mouseWheel",
    x: 300,
    y: 300,
    deltaX: 0,
    deltaY: -120,
    modifiers: 2,
  });
  await new Promise((r) => setTimeout(r, 400));
}
await new Promise((r) => setTimeout(r, 1000));
const after = await page.ev("innerWidth");
console.log(
  JSON.stringify({
    innerWidthBefore: before,
    innerWidthAfter: after,
    zoomEvents: await ui.ev("window.__zlog"),
    badge: await ui.ev(
      `document.querySelector('.topbar .zoom')?.textContent ?? null`,
    ),
  }),
);
ui.close();
page.close();
