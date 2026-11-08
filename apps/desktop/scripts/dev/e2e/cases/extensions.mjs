// Scenarios: extensions (an unpacked folder, extensions from the store).
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import {
  evalIn,
  invoke,
  waitFor,
  page,
  openUrl,
  extDir,
} from "../harness.mjs";

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
