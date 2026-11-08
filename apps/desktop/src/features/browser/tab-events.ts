// BrowserStore backend events: `tab-update` patches, media counts and crashed pages.
import type { BrowserStore } from "./browser.svelte";
import type { TabId, TabPatch } from "./types";
import { tabsApi } from "./api";
import { settings } from "../settings/settings.svelte";
import { rememberFavicon } from "../../shared/favicons";
import { hostOf, siteOf } from "../../shared/url";
import { focusAfterLayout, scheduleLayout } from "../../platform/layout.svelte";

export function applyPatch(store: BrowserStore, p: TabPatch) {
  const tab = store.tabs[p.id];
  if (!tab) return;
  if (p.url !== undefined && p.url !== tab.url) {
    const oldSite = siteOf(hostOf(tab.url));
    tab.url = p.url;
    const site = siteOf(hostOf(p.url));
    if (site && site !== oldSite) {
      const want = settings.value.siteZoom[site] ?? 1;
      if (Math.abs(want - tab.zoom) > 0.001)
        void tabsApi.tabCommand(tab.id, "zoom", want);
    }
  }
  if (p.title !== undefined) tab.title = p.title;
  if (p.loading !== undefined) tab.loading = p.loading;
  if (p.favicon !== undefined) {
    tab.favicon = p.favicon || undefined;
    rememberFavicon(tab.url, tab.favicon);
  }
  if (p.canGoBack !== undefined) tab.canGoBack = p.canGoBack;
  if (p.canGoForward !== undefined) tab.canGoForward = p.canGoForward;
  if (p.audible !== undefined) {
    if (p.audible || tab.audible) store.audibleAt.set(tab.id, Date.now());
    tab.audible = p.audible;
  }
  if (p.muted !== undefined) tab.muted = p.muted;
  if (p.blocked !== undefined) tab.blocked = p.blocked;
  if (p.error !== undefined) tab.error = p.error;
  if (p.crashed !== undefined && p.crashed !== tab.crashed) {
    tab.crashed = p.crashed;
    scheduleLayout();
  }
  if (p.zoom !== undefined) {
    tab.zoom = p.zoom;
    const site = siteOf(hostOf(tab.url));
    if (site) {
      const zooms = settings.value.siteZoom;
      if (Math.abs(p.zoom - 1) < 0.001) delete zooms[site];
      else zooms[site] = Math.round(p.zoom * 100) / 100;
      settings.save(false);
    }
  }
  if (p.fullscreen !== undefined) {
    tab.fullscreen = p.fullscreen;
    store.fullscreenTabId = p.fullscreen
      ? tab.id
      : store.fullscreenTabId === tab.id
        ? null
        : store.fullscreenTabId;
  }
}

/** The media downloader found (or forgot) videos in a tab. */
export function setMediaCount(store: BrowserStore, id: TabId, count: number) {
  const tab = store.tabs[id];
  if (tab && tab.media !== count) tab.media = count;
}

/** The backend reports a crashed page; reload by recreating its webview. */
export async function reviveCrashed(store: BrowserStore, id: TabId) {
  const tab = store.tabs[id];
  if (!tab) return;
  if (tab.live) {
    // The renderer is gone, so there is no scroll position to save: just free the view.
    tab.live = false;
    await tabsApi.tabClose(id).catch(() => {});
  }
  tab.crashed = "";
  await store.wake(tab);
  focusAfterLayout(id);
}
