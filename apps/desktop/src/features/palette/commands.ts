// The palette's command mode (">"): every action with a label, plus a few extra commands.
import { runAction } from "../../platform/actions";
import type { IconName } from "../../shared/ui/icons";
import { ui } from "../../shared/ui/ui.svelte";
import { toggleSiteBlocking } from "../blocker/site-blocking";
import {
  backupBookmarksFile,
  exportBookmarksFile,
  importBookmarksFile,
  restoreBookmarks,
} from "../bookmarks/bookmark-actions";
import { browser } from "../browser/browser.svelte";
import { openInternal } from "../browser/target";
import { chooseChatbot } from "../chatbot/chatbot";
import { makeDefaultBrowser } from "../default-browser/default-browser";
import { openNotes } from "../notes/actions";
import { settings } from "../settings/settings.svelte";
import { ACTIONS } from "../shortcuts/shortcuts";

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
  newWindow: "window",
  closeWindow: "close",
  undoCloseWindow: "window",
  goHome: "globe",
  stop: "stop",
  copyUrlMarkdown: "link",
  focusSearch: "search",
  showAllTabs: "layers",
  resetPinned: "pin",
  readerMode: "note",
  pictureInPicture: "film",
  pageInfo: "info",
  screenshot: "save",
  openFile: "file",
  toggleChatbot: "sparkle",
  toggleSidebarPanel: "sidebar",
  historySidebar: "history",
  toggleFloatingSidebar: "sidebar",
  toggleFloatingToolbar: "sidebar",
  splitVertical: "split",
  splitHorizontal: "split",
  splitGrid: "split",
  unsplit: "unsplit",
  clearData: "eraser",
  taskManager: "code",
  responsiveDesign: "code",
  browserToolbox: "code",
  browserConsole: "code",
  inspector: "code",
  webConsole: "code",
  jsDebugger: "code",
  networkMonitor: "code",
  styleEditor: "code",
  performance: "code",
  storage: "code",
  domPanel: "code",
  accessibility: "code",
};

/** Commands offered by the palette's command mode. */
export function commands(): Command[] {
  const list: Command[] = ACTIONS.filter(
    (a) =>
      !/^selectTab|^switchSpace\d|^lastTab|^nextTab|^prevTab|^openPalette|^commandPalette|^findNext|^findPrev|^back$|^forward$/.test(
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
      run: () => void makeDefaultBrowser().catch(() => {}),
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
      id: "chooseChatbot",
      label: "Choose the AI chatbot…",
      icon: "sparkle",
      keywords: "ai chatgpt claude gemini copilot mistral",
      run: () => chooseChatbot(false),
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
