// One dispatcher for keyboard shortcuts, menus and palette commands.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./ipc";
import { browser } from "./browser.svelte";
import {
  backupBookmarksFile,
  bookmarkAllTabs,
  bookmarkPage,
  exportBookmarksFile,
  importBookmarksFile,
  restoreBookmarks,
  toggleBookmarksBar,
  toggleBookmarksPanel,
} from "./bookmark-actions";
import { settings } from "./settings.svelte";
import { ui } from "./ui.svelte";
import type { Tab } from "./types";
import {
  hostOf,
  internalPage,
  isWebUrl,
  siteOf,
  type InternalPage,
} from "./url";
import type { IconName } from "./icons";
import { ACTIONS } from "./shortcuts";
import { incognito } from "./app.svelte";
import { onboarding } from "./onboarding.svelte";
import { notes } from "./notes.svelte";

const ZOOM_LEVELS = [
  0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4,
  5,
];

/** Actions that should keep firing while the key is held down. */
const REPEATABLE = new Set([
  "nextTab",
  "prevTab",
  "zoomIn",
  "zoomOut",
  "findNext",
  "findPrev",
  "back",
  "forward",
]);

function target(source?: string | null): Tab | undefined {
  if (source && browser.tabs[source]) return browser.tabs[source];
  return browser.activeTab;
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

/** Opens the notes editor (or brings its tab forward) with the keyboard in the open note;
 *  `create` starts a new note. */
export function openNotes(opts: { create?: boolean } = {}) {
  openInternal("notes");
  ui.closeMenu();
  if (ui.palette.open) ui.palette.open = false;
  // The notes page is part of the chrome UI: take the keyboard from the page webviews.
  void api.focusView(null);
  void notes.load().then(() => {
    if (opts.create || !notes.items.length) notes.create();
    else notes.focusEditor++;
  });
}

const onNotesPage = () => internalPage(browser.activeTab?.url ?? "") === "notes";

export async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    await api.clipboardWrite(text);
  }
}

function zoomStep(tab: Tab | undefined, dir: number) {
  if (!tab?.live) return;
  let z: number;
  if (dir === 0) z = 1;
  else {
    const cur = tab.zoom || 1;
    const next =
      dir > 0
        ? ZOOM_LEVELS.find((l) => l > cur + 0.001)
        : [...ZOOM_LEVELS].reverse().find((l) => l < cur - 0.001);
    z = next ?? cur;
  }
  void api.tabCommand(tab.id, "zoom", z);
}

export function openFind(tab: Tab | undefined) {
  if (!tab?.live) return;
  ui.find = { ...ui.find, open: true, tabId: tab.id };
  void api.focusView(null);
  requestAnimationFrame(() => document.getElementById("find-input")?.focus());
}

/** The first key bound to an action, for menu hints and tooltips. */
export function shortcutHint(action: string): string | undefined {
  return settings.value.shortcuts[action]?.[0];
}

/** A tooltip such as "Compact mode (Ctrl+Alt+C)": the label with its shortcut, when it has one. */
export function withHint(label: string, action: string): string {
  const key = shortcutHint(action);
  return key ? `${label} (${key})` : label;
}

export function toggleSiteBlocking(tab: Tab | undefined) {
  if (!tab || !isWebUrl(tab.url)) return;
  const site = siteOf(hostOf(tab.url));
  const list = settings.value.blockerDisabledSites;
  const disabled = list.includes(site);
  settings.value.blockerDisabledSites = disabled
    ? list.filter((s) => s !== site)
    : [...list, site];
  settings.save(false);
  void api
    .blockerSetDisabledSites(settings.value.blockerDisabledSites)
    .then(() => {
      if (tab.live) void api.tabCommand(tab.id, "reload");
    });
  ui.toast(
    disabled
      ? `Blocking turned on for ${site}`
      : `Blocking turned off for ${site}`,
    {
      icon: disabled ? "shieldCheck" : "shieldOff",
    },
  );
}

/** Opens a new incognito window (a separate process), optionally with pages to load. */
export async function openIncognito(urls: string[] = []) {
  try {
    await api.incognitoOpen(urls.filter(isWebUrl));
  } catch (e) {
    ui.toast(String(e), { icon: "incognito" });
  }
}

/** Saves the session, then quits. */
export async function quitApp() {
  try {
    await notes.flush();
    await api.sessionSave(browser.snapshot());
  } finally {
    await api.quit();
  }
}

/** Zoom for the app menu's zoom row: -1, 0 (reset) or +1 step. */
export function zoomActive(dir: -1 | 0 | 1) {
  zoomStep(browser.activeTab, dir);
}

export async function runAction(
  action: string,
  source?: string | null,
  repeat = false,
) {
  if (repeat && !REPEATABLE.has(action)) return;
  if (ui.shortcutRecording) return;
  const tab = target(source);
  const live = tab?.live ? tab : undefined;
  switch (action) {
    case "newTab":
      return ui.openPalette("newTab");
    case "closeTab":
      if (browser.glance && (source === browser.glance.tabId || !source))
        return browser.closeGlance();
      if (tab) browser.closeTab(tab.id);
      // Closing the last tab keeps the window open; Ctrl+W on that empty window closes it.
      else if (!source) void getCurrentWindow().close();
      return;
    case "closeGlance":
      return browser.closeGlance();
    case "reopenClosed":
      return browser.reopenClosed();
    case "nextTab":
      return browser.cycleTab(1);
    case "prevTab":
      return browser.cycleTab(-1);
    case "lastTab":
      return browser.activateIndex(-1);
    case "duplicateTab":
      return tab && browser.duplicate(tab.id);
    case "pinTab":
      return tab && browser.togglePin(tab.id);
    case "muteTab":
      return tab && browser.setMuted(tab.id, !tab.muted);
    case "sleepTab": {
      if (!tab) return;
      const id = tab.id;
      browser.cycleTab(1);
      if (browser.activeTab?.id !== id) await browser.sleep(id);
      return;
    }
    case "openPalette":
      return ui.openPalette(
        "navigate",
        tab && isWebUrl(tab.url) ? tab.url : "",
      );
    case "commandPalette":
      return ui.openPalette("command");
    case "back":
      return live && api.tabCommand(live.id, "back");
    case "forward":
      return live && api.tabCommand(live.id, "forward");
    case "reload":
      if (tab?.crashed) return browser.reviveCrashed(tab.id);
      return live && api.tabCommand(live.id, "reload");
    case "hardReload":
      return live && api.tabCommand(live.id, "hardReload");
    case "stop":
      return live && api.tabCommand(live.id, "stop");
    case "copyUrl":
      if (tab && !tab.url.startsWith("zenith://")) {
        await copyText(tab.url);
        ui.toast("Link copied", { icon: "link" });
      }
      return;
    case "find":
      // On the notes page Ctrl+F searches the notes.
      if (!live && onNotesPage()) {
        notes.focusSearch++;
        return;
      }
      return openFind(live);
    case "findNext":
    case "findPrev":
      if (!ui.find.open) return openFind(live);
      if (ui.find.query)
        void api.find(ui.find.tabId, action === "findNext" ? "next" : "prev");
      return;
    case "zoomIn":
      return zoomStep(live, 1);
    case "zoomOut":
      return zoomStep(live, -1);
    case "zoomReset":
      return zoomStep(live, 0);
    case "print":
      return live && api.tabCommand(live.id, "print");
    case "savePage":
      return live && api.tabCommand(live.id, "saveAs");
    case "downloadMedia":
      if (live && isWebUrl(live.url)) ui.openMedia(live.id);
      else ui.toast("Open a page with a video first", { icon: "film" });
      return;
    case "devtools":
      return live && api.tabCommand(live.id, "devtools");
    case "viewSource":
      if (live) {
        const src = await api.tabCommand<string>(live.id, "viewSource");
        if (src) browser.openTab(src, { openerId: live.id });
      }
      return;
    case "toggleCompact":
      ui.flyout = false;
      return settings.update({ compact: !settings.value.compact });
    case "toggleSidebar": {
      // Zen: expand or collapse the sidebar (single/multiple toolbars <-> collapsed toolbar).
      const collapsed = settings.value.toolbarMode === "collapsed";
      const mode = settings.value.toolbarMode;
      // Compact mode hides the sidebar entirely, so changing its width would show nothing: leave
      // compact mode in the same step so the click always has a visible effect.
      const wasCompact = settings.value.compact;
      if (wasCompact) ui.flyout = false;
      if (collapsed)
        return settings.update({
          toolbarMode: settings.value.lastExpandedMode ?? "single",
          ...(wasCompact ? { compact: false } : {}),
        });
      return settings.update({
        toolbarMode: "collapsed",
        lastExpandedMode: mode === "collapsed" ? "single" : mode,
        ...(wasCompact ? { compact: false } : {}),
      });
    }
    case "quit":
      return quitApp();
    case "newIncognitoWindow":
      return openIncognito();
    case "toggleSplit":
      return browser.toggleSplit();
    case "fullscreen": {
      const w = getCurrentWindow();
      return w.setFullscreen(!(await w.isFullscreen()));
    }
    case "nextSpace":
      return browser.cycleSpace(1);
    case "prevSpace":
      return browser.cycleSpace(-1);
    case "newSpace":
      ui.spaceEditor = { spaceId: null };
      return;
    case "bookmark":
      return bookmarkPage(tab);
    case "bookmarkAllTabs":
      return bookmarkAllTabs();
    case "toggleBookmarksSidebar":
      return toggleBookmarksPanel();
    case "toggleBookmarksBar":
      return toggleBookmarksBar();
    case "runSetup":
      // An incognito window saves nothing, so setup there would be lost.
      if (incognito)
        return ui.toast("Run setup from a normal window", { icon: "incognito" });
      return onboarding.restart();
    case "history":
    case "bookmarks":
    case "downloads":
    case "settings":
    case "extensions":
    case "passwords":
      return openInternal(action);
    case "openNotes":
      return openNotes();
    case "newNote":
      return openNotes({ create: true });
    default: {
      const m = /^selectTab(\d)$/.exec(action);
      if (m) browser.activateIndex(Number(m[1]) - 1);
    }
  }
}

export interface Command {
  id: string;
  label: string;
  icon: IconName;
  keywords?: string;
  run: () => void;
}

const ICONS: Record<string, IconName> = {
  newTab: "plus",
  closeTab: "close",
  reopenClosed: "history",
  duplicateTab: "copy",
  pinTab: "pin",
  muteTab: "volumeOff",
  sleepTab: "moonSleep",
  reload: "reload",
  hardReload: "reload",
  copyUrl: "link",
  find: "search",
  zoomIn: "zoomIn",
  zoomOut: "zoomOut",
  zoomReset: "search",
  print: "printer",
  savePage: "save",
  downloadMedia: "film",
  viewSource: "code",
  devtools: "code",
  toggleCompact: "sidebar",
  toggleSidebar: "sidebarCollapse",
  quit: "close",
  toggleSplit: "split",
  fullscreen: "expand",
  nextSpace: "layers",
  prevSpace: "layers",
  newSpace: "layers",
  newIncognitoWindow: "incognito",
  bookmark: "bookmark",
  bookmarkAllTabs: "bookmark",
  toggleBookmarksSidebar: "sidebar",
  toggleBookmarksBar: "bookmark",
  history: "history",
  bookmarks: "library",
  downloads: "download",
  extensions: "puzzle",
  passwords: "key",
  settings: "settings",
  runSetup: "sparkle",
  openNotes: "note",
};

/** Commands offered by the palette's command mode. */
export function commands(): Command[] {
  const list: Command[] = ACTIONS.filter(
    (a) =>
      !/^selectTab|^lastTab|^nextTab|^prevTab|^openPalette|^commandPalette|^findNext|^findPrev|^back$|^forward$/.test(
        a.id,
      ),
  ).map((a) => ({
    id: a.id,
    label: a.label,
    icon: ICONS[a.id] ?? "command",
    keywords: a.group,
    run: () => void runAction(a.id),
  }));
  list.push(
    {
      id: "newNote",
      label: "New note",
      icon: "note",
      keywords: "notes editor text write",
      run: () => openNotes({ create: true }),
    },
    {
      id: "clearSpace",
      label: "Close all unpinned tabs in this space",
      icon: "eraser",
      run: () => browser.clearSpace(),
    },
    {
      id: "closeOthers",
      label: "Close other tabs",
      icon: "close",
      run: () => browser.activeTab && browser.closeOthers(browser.activeTab.id),
    },
    {
      id: "toggleBlocking",
      label: "Turn ad blocking on/off for this site",
      icon: "shield",
      run: () => toggleSiteBlocking(browser.activeTab),
    },
    {
      id: "themeDark",
      label: "Theme: dark",
      icon: "moon",
      keywords: "appearance",
      run: () => settings.update({ theme: "dark" }),
    },
    {
      id: "themeLight",
      label: "Theme: light",
      icon: "sun",
      keywords: "appearance",
      run: () => settings.update({ theme: "light" }),
    },
    {
      id: "themeSystem",
      label: "Theme: follow Windows",
      icon: "sun",
      keywords: "appearance",
      run: () => settings.update({ theme: "system" }),
    },
    {
      id: "clearData",
      label: "Clear browsing data…",
      icon: "eraser",
      keywords: "privacy cookies cache",
      run: () => openInternal("settings", "privacy"),
    },
    {
      id: "defaultBrowser",
      label: "Make Zenith the default browser",
      icon: "globe",
      run: () => void api.defaultBrowserRegister(),
    },
    {
      id: "importBookmarks",
      label: "Import bookmarks (HTML)",
      icon: "bookmark",
      keywords: "bookmarks firefox chrome file",
      run: () => void importBookmarksFile(),
    },
    {
      id: "importBookmarksBrowser",
      label: "Import bookmarks from another browser",
      icon: "bookmark",
      keywords: "bookmarks chrome edge firefox",
      run: () => (ui.bookmarkImport = true),
    },
    {
      id: "exportBookmarks",
      label: "Export bookmarks (HTML)",
      icon: "bookmark",
      keywords: "bookmarks",
      run: () => void exportBookmarksFile(),
    },
    {
      id: "backupBookmarks",
      label: "Back up bookmarks",
      icon: "bookmark",
      keywords: "bookmarks json save",
      run: () => void backupBookmarksFile(),
    },
    {
      id: "restoreBookmarks",
      label: "Restore bookmarks from a backup",
      icon: "bookmark",
      keywords: "bookmarks json",
      run: () => void restoreBookmarks(),
    },
    {
      id: "shortcuts",
      label: "Customize keyboard shortcuts",
      icon: "keyboard",
      run: () => openInternal("settings", "shortcuts"),
    },
  );
  for (const space of browser.spaces) {
    list.push({
      id: `space:${space.id}`,
      label: `Switch to space: ${space.name}`,
      icon: "layers",
      run: () => browser.switchSpace(space.id),
    });
  }
  return list;
}
