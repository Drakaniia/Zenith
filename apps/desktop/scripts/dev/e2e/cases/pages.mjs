// Scenarios: what pages do (find in page, permission prompts, popups, crashes, zoom).
import {
  session,
  evalIn,
  ui,
  invoke,
  waitFor,
  waitUi,
  page,
  clickSelector,
  openUrl,
  tabTitles,
  activeTabId,
  pageOf,
  pageMenuItem,
} from "../harness.mjs";

export const scenarios = [];
export const zoom = [];
let list = scenarios;
const test = (name, fn, limitMs) => list.push({ name, fn, limitMs });

test("find-in-page", async () => {
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

test("permission-prompt", async () => {
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

test("popup-blocking-and-user-popups", async () => {
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

test("crash-recovery", async () => {
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

// Run at a different point of the suite (see index.mjs).
list = zoom;

test("zoom", async () => {
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
