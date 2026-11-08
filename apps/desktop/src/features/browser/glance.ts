// BrowserStore Glance: the floating preview card and page-opened popups.
import type { BrowserStore } from "./browser.svelte";
import type { PopupAnchor, TabId } from "./types";
import { tabsApi } from "./api";
import { focusAfterLayout, scheduleLayout } from "../../platform/layout.svelte";
import { makeTab } from "./model";

export function openGlance(store: BrowserStore, url: string, openerId?: TabId, anchor?: PopupAnchor) {
  if (store.glance) store.closeGlance();
  const draft = makeTab(url, "", { openerId, live: true, loading: true });
  store.tabs[draft.id] = draft;
  store.glance = { tabId: draft.id, openerId, popup: false, anchor };
  void tabsApi
    .tabCreate({
      id: draft.id,
      url,
      glance: true,
      openerUrl: openerId ? store.tabs[openerId]?.url : null,
    })
    .catch(() => store.closeGlance());
  focusAfterLayout(draft.id);
  scheduleLayout();
}

/** Shows a page-opened popup (window.open with a size) as a floating card over its opener. */
export function adoptPopup(store: BrowserStore, id: TabId, url: string, openerId: TabId) {
  store.tabs[id] = makeTab(url, "", {
    id,
    openerId,
    live: true,
    loading: true,
  });
  if (store.glance) store.closeGlance();
  store.glance = { tabId: id, openerId, popup: true };
  void tabsApi.tabSetGlance(id, true);
  focusAfterLayout(id);
  scheduleLayout();
}

export function closeGlance(store: BrowserStore) {
  const g = store.glance;
  if (!g) return;
  store.glance = null;
  if (store.tabs[g.tabId]) {
    void tabsApi.tabClose(g.tabId);
    delete store.tabs[g.tabId];
  }
  focusAfterLayout(store.activeTab?.live ? store.activeTab.id : null);
  scheduleLayout();
}

/** Turns the Glance card into a regular tab without reloading the page. */
export function expandGlance(store: BrowserStore, split = false) {
  const g = store.glance;
  const tab = g && store.tabs[g.tabId];
  if (!g || !tab) return;
  store.glance = null;
  void tabsApi.tabSetGlance(tab.id, false);
  const space = store.activeSpace!;
  tab.spaceId = space.id;
  tab.openerId = g.openerId;
  const at = g.openerId ? space.tabIds.indexOf(g.openerId) : -1;
  space.tabIds.splice(at >= 0 ? at + 1 : 0, 0, tab.id);
  if (split && g.openerId && store.tabs[g.openerId])
    store.splitWith(g.openerId, tab.id);
  else store.activate(tab.id);
}
