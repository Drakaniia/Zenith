// BrowserStore tab placement: pin, essentials, move to a space or within the sidebar.
import type { BrowserStore } from "./browser.svelte";
import type { SpaceId, TabId } from "./types";
import { scheduleLayout } from "../../platform/layout.svelte";
import { neighbor } from "./tab-close";

export function togglePin(store: BrowserStore, id: TabId) {
  const tab = store.tabs[id];
  if (!tab || tab.essential) return;
  const space = store.space(tab.spaceId);
  if (!space) return;
  if (tab.pinned) {
    space.pinnedIds = space.pinnedIds.filter((x) => x !== id);
    for (const f of space.folders ?? [])
      f.tabIds = f.tabIds.filter((x) => x !== id);
    space.tabIds.unshift(id);
    tab.pinned = false;
    tab.pinnedUrl = undefined;
  } else {
    space.tabIds = space.tabIds.filter((x) => x !== id);
    space.pinnedIds.push(id);
    tab.pinned = true;
    tab.pinnedUrl = tab.url;
  }
}

export function toggleEssential(store: BrowserStore, id: TabId) {
  const tab = store.tabs[id];
  if (!tab) return;
  if (tab.essential) {
    store.essentials = store.essentials.filter((x) => x !== id);
    const space = store.activeSpace!;
    tab.essential = false;
    tab.spaceId = space.id;
    tab.pinned = true;
    space.pinnedIds.push(id);
  } else {
    if (store.essentials.length >= 12) return;
    for (const space of store.spaces) {
      space.pinnedIds = space.pinnedIds.filter((x) => x !== id);
      space.tabIds = space.tabIds.filter((x) => x !== id);
      if (space.split?.ids.includes(id)) space.split = undefined;
    }
    tab.essential = true;
    tab.pinned = false;
    tab.spaceId = "";
    tab.pinnedUrl = tab.url;
    store.essentials.push(id);
  }
}

export function moveToSpace(store: BrowserStore, id: TabId, spaceId: SpaceId) {
  const tab = store.tabs[id];
  const target = store.space(spaceId);
  if (!tab || !target || tab.spaceId === spaceId) return;
  if (tab.essential) store.toggleEssential(id);
  const from = store.space(tab.spaceId);
  if (from) {
    if (from.activeTabId === id) {
      from.activeTabId = neighbor(store, id, from);
      if (from.id === store.activeSpaceId && from.activeTabId)
        store.activate(from.activeTabId);
    }
    if (from.split?.ids.includes(id)) from.split = undefined;
    from.pinnedIds = from.pinnedIds.filter((x) => x !== id);
    from.tabIds = from.tabIds.filter((x) => x !== id);
  }
  tab.spaceId = spaceId;
  (tab.pinned ? target.pinnedIds : target.tabIds).unshift(id);
  scheduleLayout();
}

/** Moves a tab within the sidebar: into essentials, pinned or regular lists. */
export function moveTab(store: BrowserStore, id: TabId, zone: "essentials" | "pinned" | "tabs", index: number) {
  const tab = store.tabs[id];
  const space = store.activeSpace;
  if (!tab || !space) return;
  if (zone === "essentials") {
    if (!tab.essential) store.toggleEssential(id);
    const list = store.essentials.filter((x) => x !== id);
    list.splice(Math.min(index, list.length), 0, id);
    store.essentials = list;
    return;
  }
  if (tab.essential) {
    store.essentials = store.essentials.filter((x) => x !== id);
    tab.essential = false;
  }
  if (tab.spaceId !== space.id) {
    const from = store.space(tab.spaceId);
    if (from) {
      from.pinnedIds = from.pinnedIds.filter((x) => x !== id);
      from.tabIds = from.tabIds.filter((x) => x !== id);
    }
    tab.spaceId = space.id;
  }
  space.pinnedIds = space.pinnedIds.filter((x) => x !== id);
  space.tabIds = space.tabIds.filter((x) => x !== id);
  const list = zone === "pinned" ? space.pinnedIds : space.tabIds;
  list.splice(Math.min(index, list.length), 0, id);
  const nowPinned = zone === "pinned";
  if (nowPinned && !tab.pinned) tab.pinnedUrl = tab.url;
  if (!nowPinned) tab.pinnedUrl = undefined;
  tab.pinned = nowPinned;
}
