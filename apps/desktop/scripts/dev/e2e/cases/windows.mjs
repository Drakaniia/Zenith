// Scenarios: the window (compact mode, layout animation, narrow windows, incognito, spaces, Essentials).
import {
  sleep,
  targets,
  session,
  evalIn,
  chrome,
  incognitoChromes,
  resizeWindow,
  ui,
  invoke,
  waitFor,
  waitUi,
  page,
  openUrl,
  tabTitles,
  layout,
  activeTabId,
  tabSel,
  openSettings,
} from "../harness.mjs";

export const scenarios = [];
const list = scenarios;
const test = (name, fn, limitMs) => list.push({ name, fn, limitMs });

test("compact-mode", async () => {
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

test("layout-animation", async () => {
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

test("narrow-multiple-toolbars", async () => {
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

test("incognito-window", async () => {
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

test("spaces", async () => {
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

test("essentials", async () => {
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
