// Actions on tabs, split view, spaces and Zenith's own pages.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { registerActionPattern, registerActions } from "../../platform/actions";
import { shell } from "../../platform/shell";
import { ui } from "../../shared/ui/ui.svelte";
import { NEW_TAB_URL } from "../../shared/url";
import { settings } from "../settings/settings.svelte";
import { browser } from "./browser.svelte";
import { openInternal, targetTab } from "./target";
import type { SplitLayout } from "./types";

/** Zen's split shortcuts (Ctrl+Alt+V, H, G): the tabs picked in the sidebar (two to four), else
 *  the current tab and the next one. In a split view they change its layout instead. */
function splitShortcut(layout: SplitLayout) {
  if (browser.activeSplit) return browser.setSplitLayout(layout);
  const picked = ui.selectedTabs.filter((id) => browser.tabs[id]);
  if (picked.length >= 2) {
    ui.clearTabSelection();
    return browser.splitTabs(picked, layout);
  }
  const tab = browser.activeTab;
  if (!tab) return;
  const order = browser.order();
  const i = order.indexOf(tab.id);
  // The next tab, wrapping around to the first (Essentials, then pinned, then regular tabs).
  const next = [...order.slice(i + 1), ...order.slice(0, Math.max(i, 0))].find((id) => id !== tab.id);
  const other = next ?? browser.openTab(NEW_TAB_URL, { activate: false }).id;
  browser.splitTabs([tab.id, other], layout);
}

/** Ctrl+O: a local file in a new tab. */
async function openLocalFile() {
  const path = await shell.pickFile().catch(() => null);
  if (!path) return;
  try {
    browser.openTab(new URL("file:///" + path.replace(/\\/g, "/").replace(/^\/+/, "")).href);
  } catch {
    ui.toast("That file can't be opened", { icon: "alert" });
  }
}

const internal = (_: unknown, action: string) =>
  openInternal(action as "history" | "bookmarks" | "downloads" | "settings" | "extensions" | "passwords");

registerActions(
  {
    closeTab: (source) => {
      if (browser.glance && (source === browser.glance.tabId || !source))
        return browser.closeGlance();
      const tab = targetTab(source);
      if (tab) browser.closeTab(tab.id);
      // Closing the last tab keeps the window open; Ctrl+W on that empty window closes it.
      else if (!source) void getCurrentWindow().close();
    },
    closeGlance: () => browser.closeGlance(),
    reopenClosed: () => browser.reopenClosed(),
    nextTab: () => browser.cycleTab(1),
    prevTab: () => browser.cycleTab(-1),
    lastTab: () => browser.activateIndex(-1),
    duplicateTab: (source) => {
      const tab = targetTab(source);
      return tab && browser.duplicate(tab.id);
    },
    pinTab: (source) => {
      const tab = targetTab(source);
      return tab && browser.togglePin(tab.id);
    },
    muteTab: (source) => {
      const tab = targetTab(source);
      return tab && browser.setMuted(tab.id, !tab.muted);
    },
    sleepTab: async (source) => {
      const tab = targetTab(source);
      if (!tab) return;
      const id = tab.id;
      browser.cycleTab(1);
      if (browser.activeTab?.id !== id) await browser.sleep(id);
    },
    goHome: (source) => {
      const tab = targetTab(source);
      const home = settings.value.homepage || NEW_TAB_URL;
      if (tab && !tab.essential) return browser.navigate(tab, home);
      return browser.openTab(home);
    },
    resetPinned: (source) => {
      const tab = targetTab(source);
      if (tab?.pinnedUrl) return browser.resetPinned(tab.id);
      return ui.toast("Only pinned tabs and Essentials have a pinned address", { icon: "pin" });
    },
    openFile: () => openLocalFile(),
    toggleSplit: () => browser.toggleSplit(),
    splitVertical: () => splitShortcut("vsep"),
    splitHorizontal: () => splitShortcut("hsep"),
    splitGrid: () => splitShortcut("grid"),
    unsplit: () => browser.unsplit(),
    nextSpace: () => browser.cycleSpace(1),
    prevSpace: () => browser.cycleSpace(-1),
    newSpace: () => {
      ui.spaceEditor = { spaceId: null };
    },
    clearData: () => openInternal("settings", "privacy"),
    history: internal,
    bookmarks: internal,
    downloads: internal,
    settings: internal,
    extensions: internal,
    passwords: internal,
  },
  { repeatable: ["nextTab", "prevTab"] },
);

registerActionPattern(/^selectTab(\d)$/, (m) => browser.activateIndex(Number(m[1]) - 1));
registerActionPattern(/^switchSpace(\d+)$/, (m) => {
  const space = browser.spaces[Number(m[1]) - 1];
  if (space) browser.switchSpace(space.id);
});
