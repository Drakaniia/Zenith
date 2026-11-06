// Tab sleeping (plan section 5): background tabs are first suspended (cheap, state kept in the
// renderer), then fully unloaded after the configured timeout. Tabs playing audio, downloading,
// or shown in split view / Glance are exempt. Under system memory pressure the least recently
// used background tabs are unloaded early.
import { api } from "./ipc";
import { browser } from "./browser.svelte";
import { settings } from "./settings.svelte";
import { downloads } from "./ui.svelte";

const SUSPEND_AFTER_MS = 60_000;
const TICK_MS = 30_000;
const PRESSURE_LOAD = 88;

let timer: ReturnType<typeof setInterval> | undefined;

export function startSleepScheduler() {
  clearInterval(timer);
  timer = setInterval(() => void tick(), TICK_MS);
}

function exempt(id: string): boolean {
  const tab = browser.tabs[id];
  if (!tab) return true;
  return (
    browser.isVisible(id) ||
    browser.recentlyAudible(id) ||
    downloads.busyTab(id) ||
    browser.spaces.some((s) => s.split?.ids.includes(id))
  );
}

export async function tick(now = Date.now()) {
  const s = settings.value;
  const sleepAfter = s.sleepMinutes > 0 ? s.sleepMinutes * 60_000 : Infinity;
  const idle: { id: string; since: number }[] = [];
  for (const tab of Object.values(browser.tabs)) {
    if (!tab.live || exempt(tab.id)) continue;
    let since = browser.hiddenAt.get(tab.id);
    if (since === undefined) {
      since = now;
      browser.hiddenAt.set(tab.id, now);
    }
    const hiddenFor = now - since;
    if (hiddenFor >= sleepAfter) {
      await browser.sleep(tab.id);
      continue;
    }
    if (s.suspendHidden && !tab.suspended && hiddenFor >= SUSPEND_AFTER_MS) {
      tab.suspended = true;
      void api
        .tabCommand(tab.id, "suspend")
        .catch(() => (tab.suspended = false));
    }
    idle.push({ id: tab.id, since });
  }
  if (s.memoryPressure && idle.length) {
    const load = await api.systemMemoryLoad().catch(() => 0);
    if (load >= PRESSURE_LOAD) {
      idle.sort((a, b) => a.since - b.since);
      for (const { id } of idle.slice(0, 3)) await browser.sleep(id);
    }
  }
}
