// Scenarios: navigation and the basics (palette, IPC isolation, webview placement, ad blocking, history, bookmarks).
import {
  evalIn,
  ui,
  invoke,
  waitFor,
  waitUi,
  page,
  openUrl,
  tabTitles,
  layout,
} from "../harness.mjs";

export const scenarios = [];
const list = scenarios;
const test = (name, fn, limitMs) => list.push({ name, fn, limitMs });

test("palette-navigation", async () => {
  await openUrl("example.com");
  await waitFor(
    async () => (await tabTitles()).includes("Example Domain"),
    "Example Domain tab",
  );
  await page("example.com");
  return (await tabTitles()).join(", ");
});

test("tab-has-no-ipc", async () => {
  const t = await page("example.com");
  const v = await evalIn(
    t,
    `[typeof window.__TAURI_INTERNALS__, typeof window.__TAURI__, typeof window.ipc].join(',')`,
  );
  if (v !== "undefined,undefined,undefined") throw new Error(`page sees ${v}`);
  return "no Tauri bridge in page";
});

test("webview-matches-pane", async () => {
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

test("palette-raises-overlay", async () => {
  await ui(`document.querySelector('.pill .main').click()`);
  await waitUi(`!!document.querySelector('.palette')`, "palette");
  await waitFor(async () => (await layout()).chromeRaised, "chrome raised");
  await ui(
    `document.querySelector('.palette input').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`,
  );
  await waitFor(async () => !(await layout()).chromeRaised, "chrome lowered");
  return "raised while open, lowered after";
});

test("ad-blocking", async () => {
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

test("history", async () => {
  const rows = await invoke("history_search", {
    text: "example",
    limit: 5,
    offset: 0,
  });
  if (!rows.some((r) => r.url.includes("example.com")))
    throw new Error("example.com not in history");
  return `${rows.length} matching entr${rows.length === 1 ? "y" : "ies"}`;
});

test("bookmark", async () => {
  await ui(`document.querySelector('.pill .star').click()`);
  await waitUi(`!!document.querySelector('.pill .star.on')`, "starred");
  const list = await invoke("bookmarks_list");
  if (!list.some((b) => b.url.includes("example.com")))
    throw new Error("bookmark not stored");
  return `${list.length} bookmark(s)`;
});
