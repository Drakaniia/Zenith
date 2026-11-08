// Probe: does WebView2 report user zoom (Ctrl+wheel on the page) to the chrome UI?
import { connect } from "./cdp-session.mjs";

const PORT = Number(process.env.ZENITH_REMOTE_DEBUG_PORT || 9444);
const list = async () =>
  (await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json()).filter(
    (t) => t.type === "page",
  );

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
