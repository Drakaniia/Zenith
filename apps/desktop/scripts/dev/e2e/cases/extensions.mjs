// Scenarios: extensions (unpacked MV3 fixture: content script, popup, tabs.query, sendMessage,
// scripting, action.onClicked, setPopup, live badges, Zen panel, and real Chrome Web Store
// extensions Dark Reader + uBlock Origin Lite).
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import {
  evalIn,
  invoke,
  waitFor,
  waitUi,
  page,
  openUrl,
  extDir,
  ui,
  PORT,
  sleep,
} from "../harness.mjs";

const allTargets = async () => (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();

export const loading = [];
export const store = [];
let list = loading;
const test = (name, fn, limitMs) => list.push({ name, fn, limitMs });

test("extension-loading", async () => {
  rmSync(extDir, { recursive: true, force: true });
  mkdirSync(extDir, { recursive: true });
  writeFileSync(
    join(extDir, "manifest.json"),
    JSON.stringify({
      manifest_version: 3,
      name: "Zenith E2E Marker",
      version: "1.0",
      description: "Verifies content scripts, popup tabs.query/sendMessage/scripting, and badges.",
      permissions: ["tabs", "activeTab", "scripting"],
      host_permissions: ["<all_urls>"],
      background: { service_worker: "bg.js" },
      action: { default_popup: "popup.html" },
      content_scripts: [
        { matches: ["<all_urls>"], js: ["mark.js"], run_at: "document_start" },
      ],
    }),
  );
  writeFileSync(
    join(extDir, "mark.js"),
    `document.documentElement.dataset.zenithExt = '1';
     chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
       if (msg?.ping) reply({ pong: location.hostname, title: document.title });
     });`,
  );
  writeFileSync(
    join(extDir, "bg.js"),
    `chrome.action.setBadgeText({ text: "7" });
     chrome.action.setBadgeBackgroundColor({ color: "#ff5500" });`,
  );
  writeFileSync(
    join(extDir, "popup.html"),
    `<!doctype html><html><body style="width:260px;height:140px"><div id="out">loading</div><script src="popup.js"></script></body></html>`,
  );
  writeFileSync(
    join(extDir, "popup.js"),
    `(async () => {
      const [active] = await chrome.tabs.query({ active: true, currentWindow: true, windowType: "normal" });
      const cur = await chrome.tabs.getCurrent();
      let pong = null, execTitle = null;
      if (active?.id) {
        pong = await chrome.tabs.sendMessage(active.id, { ping: true }).catch((e) => ({ err: String(e) }));
        const r = await chrome.scripting.executeScript({ target: { tabId: active.id }, func: () => document.title }).catch(() => null);
        execTitle = r?.[0]?.result ?? null;
      }
      window.__popupResult = { activeUrl: active?.url ?? null, cur, pong, execTitle };
      document.getElementById("out").textContent = JSON.stringify(window.__popupResult);
    })();`,
  );

  const ext = await invoke("ext_install_folder", { path: extDir });
  if (!ext?.id) throw new Error("install returned nothing");
  if (!ext.hasAction || !ext.hasActiveTab || !ext.hostPermissions?.includes("<all_urls>")) {
    throw new Error(`missing manifest metadata: ${JSON.stringify(ext)}`);
  }

  await openUrl("https://example.net/");
  const t = await page("example.net");
  const marked = await waitFor(
    () => evalIn(t, `document.documentElement.dataset.zenithExt === '1'`),
    "content script ran",
    15000,
  );

  // Verify live badge streamed from MV3 service worker into Zenith.
  await waitFor(async () => {
    const badges = await invoke("ext_badges");
    return badges.some((b) => b.extensionId === ext.id && b.badge?.text === "7");
  }, "live MV3 badge synced", 10000);

  // Open the Zen-style unified extensions popover and verify site access status, then open popup.
  await ui(`document.querySelector('[data-action="extensions"]').click()`);
  await waitUi(
    `document.querySelector('[data-ext-row="${ext.id}"]')?.textContent?.includes('Can access this site')`,
    "unified panel site-access status",
    6000,
  );
  await ui(`document.querySelector('[data-ext-row="${ext.id}"] .open').click()`);
  await waitUi(`!!document.querySelector('[data-ext-popup="${ext.id}"]')`, "dedicated ExtensionPopup", 6000);

  const popTarget = await waitFor(
    async () => (await allTargets()).find((x) => x.url.includes(ext.id) && x.url.includes("popup.html")),
    "extension popup target",
    10000,
  );
  const popRes = await waitFor(
    () => evalIn(popTarget, `window.__popupResult`),
    "popup query/sendMessage/scripting result",
    10000,
  );
  if (!popRes.activeUrl?.includes("example.net")) {
    throw new Error(`popup saw wrong active tab: ${JSON.stringify(popRes)}`);
  }
  if (popRes.cur !== undefined && popRes.cur !== null) {
    throw new Error(`popup tabs.getCurrent() should be undefined: ${JSON.stringify(popRes.cur)}`);
  }
  if (popRes.pong?.pong !== "example.net") {
    throw new Error(`popup sendMessage failed: ${JSON.stringify(popRes)}`);
  }
  if (!popRes.execTitle) {
    throw new Error(`popup scripting.executeScript failed: ${JSON.stringify(popRes)}`);
  }

  // Close popup via backdrop click and test action-only extension (chrome.action.onClicked).
  await ui(`document.querySelector('[data-ext-popup]')?.previousElementSibling?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))`);
  await waitUi(`!document.querySelector('[data-ext-popup]')`, "popup closed", 4000);

  const actionDir = `${extDir}-action`;
  rmSync(actionDir, { recursive: true, force: true });
  mkdirSync(actionDir, { recursive: true });
  writeFileSync(
    join(actionDir, "manifest.json"),
    JSON.stringify({
      manifest_version: 3,
      name: "Zenith ActionOnly",
      version: "1.0",
      permissions: ["tabs", "scripting"],
      host_permissions: ["<all_urls>"],
      background: { service_worker: "bg.js" },
      action: {},
    }),
  );
  writeFileSync(
    join(actionDir, "bg.js"),
    `chrome.action.onClicked.addListener(async (tab) => {
      await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: () => { document.documentElement.dataset.zenithActionClicked = 'ok'; },
      });
    });`,
  );
  const actionExt = await invoke("ext_install_folder", { path: actionDir });
  await sleep(700);
  await ui(`document.querySelector('[data-action="extensions"]').click()`);
  await waitUi(`!!document.querySelector('[data-ext-row="${actionExt.id}"]')`, "action-only row", 6000);
  await ui(`document.querySelector('[data-ext-row="${actionExt.id}"] .open').click()`);
  await waitFor(
    () => evalIn(t, `document.documentElement.dataset.zenithActionClicked === 'ok'`),
    "chrome.action.onClicked executed on active tab",
    10000,
  );

  const extList = await invoke("ext_list");
  await invoke("ext_remove", { id: actionExt.id });
  await invoke("ext_remove", { id: ext.id });
  rmSync(actionDir, { recursive: true, force: true });
  return `installed ${ext.name}, contentScript=${marked}, badge=7, popupActive=${popRes.activeUrl}, pong=${popRes.pong.pong}, actionOnClicked=ok, listed=${extList.length}`;
}, 90000);

// Run at a different point of the suite (see index.mjs).
list = store;

test("store-extensions", async () => {
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

  // Open each store extension's real toolbar popup over example.com and verify it recognizes example.com!
  for (const ext of installed) {
    if (!ext.popupUrl) continue;
    await ui(`document.querySelector('[data-action="extensions"]').click()`);
    await waitUi(`!!document.querySelector('[data-ext-row="${ext.id}"]')`, `row ${ext.name}`, 6000);
    await ui(`document.querySelector('[data-ext-row="${ext.id}"] .open').click()`);
    await waitUi(`!!document.querySelector('[data-ext-popup="${ext.id}"]')`, `popup ${ext.name}`, 6000);
    const popTarget = await waitFor(
      async () => (await allTargets()).find((x) => x.url.includes(ext.id) && x.url.includes("popup")),
      `popup target ${ext.name}`,
      10000,
    );
    const sawExample = await waitFor(
      () => evalIn(popTarget, `document.body.innerText.toLowerCase().includes('example.com')`),
      `${ext.name} popup recognized example.com`,
      12000,
    );
    notes.push(`${ext.name} popup recognized example.com: ${sawExample}`);
    await ui(`document.querySelector('[data-ext-popup]')?.previousElementSibling?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))`);
    await waitUi(`!document.querySelector('[data-ext-popup]')`, `closed ${ext.name} popup`, 4000);
  }

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
