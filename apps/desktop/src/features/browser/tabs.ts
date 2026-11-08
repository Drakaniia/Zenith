// BrowserStore tab lifecycle: open, adopt, wake, activate, navigate, sleep and unload.
import type { BrowserStore } from "./browser.svelte";
import type { SpaceId, Tab, TabId } from "./types";
import { tabsApi } from "./api";
import { settings } from "../settings/settings.svelte";
import { NEW_TAB_URL, hostOf, siteOf } from "../../shared/url";
import { focusAfterLayout, scheduleLayout } from "../../platform/layout.svelte";
import { makeTab, needsWebview } from "./model";
import { isSplitAnywhere } from "./split";

/** Opens a tab. Background tabs still start loading so they are ready when visited. */
export function openTab(
  store: BrowserStore,
  url: string = NEW_TAB_URL,
  opts: {
    activate?: boolean;
    spaceId?: SpaceId;
    openerId?: TabId;
    index?: number;
    pinned?: boolean;
  } = {},
): Tab {
  const space =
    store.space(opts.spaceId) ?? store.activeSpace ?? store.createSpace();
  const draft = makeTab(url, space.id, {
    openerId: opts.openerId,
    pinned: !!opts.pinned,
  });
  store.tabs[draft.id] = draft;
  const tab = store.tabs[draft.id];
  const list = opts.pinned ? space.pinnedIds : space.tabIds;
  let index = opts.index ?? (settings.value.newTabsAtTop ? 0 : list.length);
  if (opts.index === undefined && opts.openerId) {
    const at = list.indexOf(opts.openerId);
    if (at >= 0) {
      index = at + 1;
      // Keep tabs opened from the same page together, in opening order.
      while (
        index < list.length &&
        store.tabs[list[index]]?.openerId === opts.openerId
      )
        index++;
    }
  }
  list.splice(Math.max(0, Math.min(index, list.length)), 0, tab.id);
  if (opts.pinned) tab.pinnedUrl = url;
  if (opts.activate !== false) store.activate(tab.id);
  else if (needsWebview(url)) void store.wake(tab);
  return tab;
}

/** Adopts a webview the backend already created (popup opened with an opener). */
export function adoptTab(
  store: BrowserStore,
  id: TabId,
  url: string,
  openerId: TabId | undefined,
  activate: boolean,
) {
  const opener = openerId ? store.tabs[openerId] : undefined;
  const spaceId =
    opener && !opener.essential ? opener.spaceId : store.activeSpaceId;
  const space = store.space(spaceId) ?? store.activeSpace!;
  store.tabs[id] = makeTab(url, space.id, {
    id,
    openerId,
    live: true,
    loading: true,
  });
  const at = openerId ? space.tabIds.indexOf(openerId) : -1;
  space.tabIds.splice(at >= 0 ? at + 1 : 0, 0, id);
  if (activate) store.activate(id);
}

export async function wake(_store: BrowserStore, tab: Tab) {
  if (tab.live || !needsWebview(tab.url)) return;
  tab.live = true;
  tab.crashed = "";
  tab.error = "";
  tab.loading = true;
  tab.media = 0;
  const site = siteOf(hostOf(tab.url));
  const zoom =
    settings.value.siteZoom[site] ?? (tab.zoom !== 1 ? tab.zoom : undefined);
  const scroll = tab.scroll;
  tab.scroll = undefined;
  try {
    await tabsApi.tabCreate({
      id: tab.id,
      url: tab.url,
      muted: tab.muted,
      zoom,
      scrollX: scroll?.x,
      scrollY: scroll?.y,
    });
  } catch (e) {
    tab.live = false;
    tab.loading = false;
    tab.error = String(e);
  }
  scheduleLayout();
}

export function activate(store: BrowserStore, id: TabId, opts: { focus?: boolean } = {}) {
  const tab = store.tabs[id];
  if (!tab) return;
  if (
    !tab.essential &&
    tab.spaceId &&
    tab.spaceId !== store.activeSpaceId &&
    store.space(tab.spaceId)
  ) {
    for (const v of store.visibleIds) store.hiddenAt.set(v, Date.now());
    store.activeSpaceId = tab.spaceId;
  }
  const space = store.activeSpace;
  if (!space) return;
  const before = store.visibleIds;
  space.activeTabId = id;
  tab.lastActive = Date.now();
  tab.suspended = false;
  for (const v of before)
    if (!store.visibleIds.includes(v)) store.hiddenAt.set(v, Date.now());
  for (const v of store.visibleIds) store.hiddenAt.delete(v);
  if (needsWebview(tab.url)) void store.wake(tab);
  if (opts.focus !== false)
    focusAfterLayout(tab.live || needsWebview(tab.url) ? id : null);
  scheduleLayout();
}

export function activateIndex(store: BrowserStore, n: number) {
  const ids = store.order();
  const id = n < 0 ? ids[ids.length - 1] : ids[n];
  if (id) store.activate(id);
}

export function cycleTab(store: BrowserStore, delta: number) {
  const ids = store.order();
  if (!ids.length) return;
  const cur = store.activeTab ? ids.indexOf(store.activeTab.id) : -1;
  store.activate(ids[(cur + delta + ids.length) % ids.length]);
}

export function navigate(store: BrowserStore, tab: Tab, url: string) {
  if (!needsWebview(url)) {
    if (tab.live) {
      store.openTab(url, { openerId: tab.id });
      return;
    }
    tab.url = url;
    tab.title = "";
    tab.favicon = undefined;
    scheduleLayout();
    return;
  }
  tab.error = "";
  if (tab.live) {
    void tabsApi.tabNavigate(tab.id, url).catch((e) => (tab.error = String(e)));
  } else {
    tab.url = url;
    tab.title = "";
    tab.favicon = undefined;
    void store.wake(tab);
  }
  focusAfterLayout(tab.id);
}

/** Frees the webview but keeps the tab (tab sleeping). Returns false if exempt. */
export async function sleep(store: BrowserStore, id: TabId, force = false): Promise<boolean> {
  const tab = store.tabs[id];
  if (!tab?.live) return false;
  if (
    !force &&
    (store.isVisible(id) ||
      store.recentlyAudible(id) ||
      isSplitAnywhere(store, id))
  )
    return false;
  tab.live = false;
  tab.loading = false;
  tab.audible = false;
  tab.suspended = false;
  tab.media = 0;
  try {
    const scroll = await tabsApi.tabSleep(id);
    if (scroll && (scroll.x > 0 || scroll.y > 0)) tab.scroll = scroll;
  } catch {
    /* the webview may already be gone */
  }
  store.hiddenAt.delete(id);
  scheduleLayout();
  return true;
}

/** Unloads a pinned/essential tab and optionally resets it to its pinned URL. */
export async function unload(store: BrowserStore, id: TabId, reset: boolean) {
  const tab = store.tabs[id];
  if (!tab) return;
  if (tab.live) await store.sleep(id, true);
  if (reset && tab.pinnedUrl) {
    tab.url = tab.pinnedUrl;
    tab.scroll = undefined;
  }
}

export function resetPinned(store: BrowserStore, id: TabId) {
  const tab = store.tabs[id];
  if (tab?.pinnedUrl) store.navigate(tab, tab.pinnedUrl);
}

export function resetZoom(store: BrowserStore, id: TabId) {
  if (store.tabs[id]?.live) return tabsApi.tabCommand(id, "zoom", 1);
}

export function setMuted(store: BrowserStore, id: TabId, muted: boolean) {
  const tab = store.tabs[id];
  if (!tab) return;
  tab.muted = muted;
  if (tab.live) void tabsApi.tabCommand(id, muted ? "mute" : "unmute");
}
