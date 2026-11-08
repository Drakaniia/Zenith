// BrowserStore spaces: create, switch, cycle, delete, unload and reorder.
import type { BrowserStore } from "./browser.svelte";
import type { Space, SpaceId } from "./types";
import { settings } from "../settings/settings.svelte";
import { focusAfterLayout, scheduleLayout } from "../../platform/layout.svelte";
import { SPACE_COLORS, SPACE_ICONS, uid } from "./model";

export function createSpace(store: BrowserStore, name?: string, icon?: string, color?: string): Space {
  const n = store.spaces.length;
  const space: Space = {
    id: uid("s"),
    name: name || (n === 0 ? "Home" : `Space ${n + 1}`),
    icon: icon || SPACE_ICONS[n % SPACE_ICONS.length],
    color: color || SPACE_COLORS[n % SPACE_COLORS.length],
    tabIds: [],
    pinnedIds: [],
  };
  store.spaces.push(space);
  return store.spaces[store.spaces.length - 1];
}

export function switchSpace(store: BrowserStore, id: SpaceId) {
  if (id === store.activeSpaceId || !store.space(id)) return;
  const previous = store.activeSpace;
  for (const v of store.visibleIds) store.hiddenAt.set(v, Date.now());
  store.activeSpaceId = id;
  const next = store.activeSpace;
  if (next?.activeTabId && store.tabs[next.activeTabId])
    store.activate(next.activeTabId);
  else focusAfterLayout(null);
  if (previous && settings.value.sleepOnSpaceSwitch) {
    for (const tid of [...previous.pinnedIds, ...previous.tabIds])
      void store.sleep(tid);
  }
  scheduleLayout();
}

export function cycleSpace(store: BrowserStore, delta: number) {
  if (store.spaces.length < 2) return;
  const i = store.spaces.findIndex((s) => s.id === store.activeSpaceId);
  const next =
    store.spaces[(i + delta + store.spaces.length) % store.spaces.length];
  store.switchSpace(next.id);
}

export function deleteSpace(store: BrowserStore, id: SpaceId) {
  if (store.spaces.length <= 1) return;
  const space = store.space(id);
  if (!space) return;
  for (const tid of [...space.pinnedIds, ...space.tabIds])
    store.removeTab(tid, false);
  const i = store.spaces.findIndex((s) => s.id === id);
  store.spaces.splice(i, 1);
  if (store.activeSpaceId === id)
    store.switchSpace(store.spaces[Math.max(0, i - 1)].id);
}

/** Unloads every tab of a space (Zen: unload all tabs in space). */
export function unloadSpace(store: BrowserStore, spaceId: SpaceId) {
  const space = store.space(spaceId);
  if (!space) return;
  for (const id of [...space.pinnedIds, ...space.tabIds]) void store.sleep(id);
}

export function moveSpace(store: BrowserStore, id: SpaceId, toIndex: number) {
  const from = store.spaces.findIndex((s) => s.id === id);
  if (from < 0) return;
  const [s] = store.spaces.splice(from, 1);
  store.spaces.splice(
    Math.max(0, Math.min(toIndex, store.spaces.length)),
    0,
    s,
  );
}
