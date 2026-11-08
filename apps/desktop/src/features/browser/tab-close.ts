// BrowserStore tab closing: close, remove, close others/below, reopen and duplicate.
import type { BrowserStore } from "./browser.svelte";
import type { Space, TabId } from "./types";
import { tabsApi } from "./api";
import { isNewTab } from "../../shared/url";
import { focusAfterLayout, scheduleLayout } from "../../platform/layout.svelte";

/** Tab to show after `id` goes away, following the order of the tab's own list. Never an
 *  Essential, and a regular tab never hands over to a pinned one: closing the last regular tab
 *  leaves the space on its start page. */
export function neighbor(store: BrowserStore, id: TabId, space: Space): TabId | undefined {
  const tab = store.tabs[id];
  const regular = !!tab && !tab.pinned && !tab.essential;
  const ids = (regular ? space.tabIds : [...store.pinnedOrder(space), ...space.tabIds]).filter(
    (x) => store.tabs[x] && !store.tabs[x].essential,
  );
  if (
    tab?.openerId &&
    ids.includes(tab.openerId) &&
    tab.lastActive > (store.tabs[tab.openerId]?.lastActive ?? 0)
  )
    return tab.openerId;
  const i = ids.indexOf(id);
  const rest = ids.filter((x) => x !== id);
  if (!rest.length) return undefined;
  return rest[Math.min(Math.max(i, 0), rest.length - 1)];
}

/** Ctrl+W. Pinned and essential tabs are unloaded (kept) instead of removed, as in Zen. */
export function closeTab(store: BrowserStore, id: TabId) {
  const tab = store.tabs[id];
  if (!tab) return;
  if (store.glance?.tabId === id) return store.closeGlance();
  if (tab.pinned || tab.essential) {
    deactivateEverywhere(store, id);
    void store.unload(id, true);
    return;
  }
  store.removeTab(id, true);
}

export function deactivateEverywhere(store: BrowserStore, id: TabId) {
  for (const space of store.spaces) {
    if (space.split?.ids.includes(id)) space.split = undefined;
    if (space.activeTabId === id) {
      const next = neighbor(store, id, space);
      space.activeTabId = next;
      if (space.id === store.activeSpaceId && next) store.activate(next);
    }
  }
  if (store.activeSpace && !store.activeSpace.activeTabId)
    focusAfterLayout(null);
}

export function removeTab(store: BrowserStore, id: TabId, remember: boolean) {
  const tab = store.tabs[id];
  if (!tab) return;
  if (remember && !isNewTab(tab.url)) {
    const space = store.space(tab.spaceId);
    store.closed.unshift({
      url: tab.url,
      title: tab.title,
      favicon: tab.favicon,
      spaceId: tab.spaceId,
      index: space ? space.tabIds.indexOf(id) : 0,
      pinned: tab.pinned,
      closedAt: Date.now(),
    });
    if (store.closed.length > 25) store.closed.length = 25;
  }
  deactivateEverywhere(store, id);
  store.essentials = store.essentials.filter((x) => x !== id);
  for (const space of store.spaces) {
    space.pinnedIds = space.pinnedIds.filter((x) => x !== id);
    space.tabIds = space.tabIds.filter((x) => x !== id);
  }
  if (tab.live) void tabsApi.tabClose(id);
  store.hiddenAt.delete(id);
  store.audibleAt.delete(id);
  delete store.tabs[id];
  scheduleLayout();
}

export function closeOthers(store: BrowserStore, id: TabId) {
  const tab = store.tabs[id];
  const space = store.space(tab?.spaceId) ?? store.activeSpace;
  if (!space) return;
  for (const other of [...space.tabIds])
    if (other !== id) store.removeTab(other, true);
  store.activate(id);
}

export function closeBelow(store: BrowserStore, id: TabId) {
  const space = store.space(store.tabs[id]?.spaceId) ?? store.activeSpace;
  if (!space) return;
  const i = space.tabIds.indexOf(id);
  for (const other of space.tabIds.slice(i + 1)) store.removeTab(other, true);
}

/** Zen's "Clear" button: close every regular (unpinned) tab of the space. */
export function clearSpace(store: BrowserStore, spaceId = store.activeSpaceId) {
  const space = store.space(spaceId);
  if (!space) return;
  for (const id of [...space.tabIds]) store.removeTab(id, true);
}

export function reopenClosed(store: BrowserStore) {
  const c = store.closed.shift();
  if (!c) return;
  const space = store.space(c.spaceId) ?? store.activeSpace;
  store.openTab(c.url, {
    spaceId: space?.id,
    index: c.index,
    pinned: c.pinned,
  });
}

export function duplicate(store: BrowserStore, id: TabId) {
  const tab = store.tabs[id];
  if (tab)
    store.openTab(tab.url, {
      openerId: tab.id,
      spaceId: tab.essential ? undefined : tab.spaceId,
    });
}
