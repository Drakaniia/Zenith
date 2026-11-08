// Scenarios: restarting the app (a clean close, then a force kill) and restoring the session.
// They run last and only without --skip-restart.
import { spawnSync } from "node:child_process";
import {
  sleep,
  ui,
  invoke,
  waitFor,
  tabTitles,
  appExited,
  relaunch,
} from "../harness.mjs";

export const scenarios = [];
const list = scenarios;
const test = (name, fn, limitMs) => list.push({ name, fn, limitMs });

test("session-restore", async () => {
  const before = (await tabTitles()).sort();
  await sleep(800);
  await ui(
    `window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' })`,
  ).catch(() => {});
  await appExited();
  await relaunch();
  const after = await waitFor(
    async () => {
      const t = (await tabTitles()).sort();
      return t.length === before.length ? t : null;
    },
    "restored tabs",
    10000,
  );
  return `restored ${after.length} tabs: ${after.join(", ")}`;
}, 90000);

test("abrupt-restore", async () => {
  const before = (await tabTitles()).sort();
  const counts = async () => ({
    bookmarks: (await invoke("bookmarks_list")).length,
    history: (
      await invoke("history_search", { text: "", limit: 500, offset: 0 })
    ).length,
    downloads: (await invoke("downloads_history", { limit: 100 })).length,
  });
  const c1 = await counts();
  await sleep(1200); // the session snapshot lands ~400 ms after the last change
  spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "scripts/dev/run.ps1",
      "-Stop",
    ],
    { stdio: "ignore" },
  );
  await appExited();
  await relaunch();
  const after = await waitFor(
    async () => {
      const t = (await tabTitles()).sort();
      return t.length === before.length ? t : null;
    },
    "restored tabs",
    15000,
  );
  const c2 = await counts();
  for (const k of Object.keys(c1))
    if (c2[k] < c1[k])
      throw new Error(`${k}: ${c1[k]} before the kill, ${c2[k]} after`);
  return `force-killed; restored ${after.length} tabs, ${c2.bookmarks} bookmarks, ${c2.history} history entries, ${c2.downloads} downloads`;
}, 90000);
