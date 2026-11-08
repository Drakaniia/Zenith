// Scenarios: tabs (Glance, split view, pinning by drag, sleeping and waking, the sleep timeout).
import {
  sleep,
  targets,
  session,
  evalIn,
  chrome,
  ui,
  invoke,
  waitFor,
  waitUi,
  page,
  clickSelector,
  typeInPalette,
  openUrl,
  tabTitles,
  layout,
  activeTabId,
  tabSel,
  pageMenuItem,
  openSettings,
} from "../harness.mjs";

export const scenarios = [];
export const sleepTimeout = [];
let list = scenarios;
const test = (name, fn, limitMs) => list.push({ name, fn, limitMs });

test("glance", async () => {
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

test("second-tab-and-split", async () => {
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

test("split-pane-close", async () => {
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

test("drag-to-pin", async () => {
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

test("sleep-and-wake", async () => {
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

// Run at a different point of the suite (see index.mjs).
list = sleepTimeout;

test("sleep-timeout-scroll-audio", async () => {
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
