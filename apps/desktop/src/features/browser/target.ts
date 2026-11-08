// Which tab an action applies to, and opening Zenith's own pages.
import { browser } from "./browser.svelte";
import type { Tab } from "./types";
import type { InternalPage } from "../../shared/url";

/** The tab a shortcut came from, else the active tab. */
export function targetTab(source?: string | null): Tab | undefined {
  if (source && browser.tabs[source]) return browser.tabs[source];
  return browser.activeTab;
}

/** The target tab when it has a live webview. */
export function liveTab(source?: string | null): Tab | undefined {
  const tab = targetTab(source);
  return tab?.live ? tab : undefined;
}

export function openInternal(page: InternalPage, section?: string) {
  const url = `zenith://${page}${section ? "/" + section : ""}`;
  const existing = Object.values(browser.tabs).find(
    (t) =>
      t.url.startsWith(`zenith://${page}`) &&
      (t.essential || t.spaceId === browser.activeSpaceId),
  );
  if (existing) {
    if (section) existing.url = url;
    browser.activate(existing.id);
  } else browser.openTab(url);
}
