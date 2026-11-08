// One dispatcher for keyboard shortcuts, menus and palette commands.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./ipc";
import { browser, tabTitle } from "./browser.svelte";
import { chooseChatbot, toggleChatbot } from "./chatbot";
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
import type { SplitLayout, Tab } from "./types";
import {
  NEW_TAB_URL,
  displayHost,
  hostOf,
  internalPage,
  isSecure,
  isWebUrl,
  siteOf,
  type InternalPage,
} from "./url";
import type { IconName } from "./icons";
import { ACTIONS, chordLabel } from "./shortcuts";
import { incognito } from "./app.svelte";
import { onboarding } from "./onboarding.svelte";
import { notes } from "./notes.svelte";
import { makeDefaultBrowser } from "./default-browser";

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
const onSettingsPage = () => internalPage(browser.activeTab?.url ?? "") === "settings";

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
  const key = settings.value.shortcuts[action]?.[0];
  return key && chordLabel(key);
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

/** Firefox's Page Info, as a panel under the address bar: the site, its connection and what
 *  Zenith did on it, with the related actions. */
function pageInfo(tab: Tab | undefined) {
  if (!tab) return;
  const web = isWebUrl(tab.url);
  const anchor = [...document.querySelectorAll<HTMLElement>(".pill .main")].find((el) => el.getBoundingClientRect().width > 0);
  const r = anchor?.getBoundingClientRect();
  const site = web ? siteOf(hostOf(tab.url)) : "";
  const blockingOff = web && settings.value.blockerDisabledSites.includes(site);
  const zoom = Math.round((tab.zoom || 1) * 100);
  ui.openMenu(r ? r.left : Math.max(8, window.innerWidth / 2 - 140), r ? r.bottom + 6 : 56, [
    { label: tabTitle(tab), heading: true },
    { label: web ? displayHost(tab.url) : tab.url, icon: "globe", heading: true },
    web
      ? isSecure(tab.url)
        ? { label: "Connection is secure (HTTPS)", icon: "lock", heading: true }
        : { label: "Connection is not secure", icon: "unlock", heading: true }
      : { label: "Zenith page or local file", icon: "info", heading: true },
    ...(web
      ? [
          {
            label: blockingOff ? "Ad and tracker blocking is off here" : `${tab.blocked} ads and trackers blocked`,
            icon: (blockingOff ? "shieldOff" : "shieldCheck") as IconName,
            heading: true,
          },
        ]
      : []),
    ...(zoom !== 100 ? [{ label: `Zoom ${zoom}%`, icon: "zoomIn" as IconName, heading: true }] : []),
    { separator: true },
    { label: "Copy address", icon: "link", run: () => void copyText(tab.url) },
    ...(web
      ? [
          {
            label: blockingOff ? "Turn on blocking for this site" : "Turn off blocking for this site",
            icon: (blockingOff ? "shieldCheck" : "shieldOff") as IconName,
            run: () => toggleSiteBlocking(tab),
          },
          { label: "View page source", icon: "code" as IconName, run: () => void runAction("viewSource", tab.id) },
          { label: "Clear site data and permissions…", icon: "eraser" as IconName, run: () => openInternal("settings", "privacy") },
        ]
      : []),
  ]);
}

/** Ctrl+O: a local file in a new tab. */
async function openLocalFile() {
  const path = await api.pickFile().catch(() => null);
  if (!path) return;
  try {
    browser.openTab(new URL("file:///" + path.replace(/\\/g, "/").replace(/^\/+/, "")).href);
  } catch {
    ui.toast("That file can't be opened", { icon: "alert" });
  }
}

/** Runs one of the backend's page tools (tab.rs `command`) and reports failures. */
async function pageTool<T>(tab: Tab | undefined, cmd: string, arg?: unknown): Promise<T | undefined> {
  if (!tab?.live) {
    ui.toast("Open a web page first", { icon: "info" });
    return undefined;
  }
  try {
    return await api.tabCommand<T>(tab.id, cmd, arg);
  } catch (e) {
    ui.toast(String(e), { icon: "alert" });
    return undefined;
  }
}

/** DevTools panels for Firefox's tool shortcuts (Chromium's names for the nearest panel). */
const DEVTOOLS_PANELS: Record<string, string> = {
  inspector: "elements",
  webConsole: "console",
  jsDebugger: "sources",
  networkMonitor: "network",
  styleEditor: "sources",
  performance: "timeline",
  storage: "resources",
  domPanel: "elements",
  accessibility: "elements",
};

function copyMarkdownLink(tab: Tab | undefined) {
  if (!tab || tab.url.startsWith("zenith://")) return;
  const title = tabTitle(tab).replace(/[[\]\\]/g, "\\$&");
  const url = tab.url.replace(/[()]/g, (c) => encodeURIComponent(c));
  void copyText(`[${title}](${url})`).then(() => ui.toast("Markdown link copied", { icon: "link" }));
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
    case "goHome": {
      const home = settings.value.homepage || NEW_TAB_URL;
      if (tab && !tab.essential) return browser.navigate(tab, home);
      return browser.openTab(home);
    }
    case "focusSearch":
      return ui.openPalette("search");
    case "showAllTabs":
      // The address bar limited to open tabs (Firefox's % restriction).
      ui.openPalette("navigate", "% ");
      ui.palette.selectAll = false;
      return;
    case "escape":
      // Esc in a page that is still loading stops it; the page gets the key either way.
      return live?.loading && api.tabCommand(live.id, "stop");
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
    case "copyUrlMarkdown":
      return copyMarkdownLink(tab);
    case "resetPinned":
      if (tab?.pinnedUrl) return browser.resetPinned(tab.id);
      return ui.toast("Only pinned tabs and Essentials have a pinned address", { icon: "pin" });
    case "pageInfo":
      return pageInfo(tab);
    case "openFile":
      return openLocalFile();
    case "readerMode": {
      const r = await pageTool<string>(live, "reader");
      if (r === "none") ui.toast("Reader view isn't available for this page", { icon: "note" });
      return;
    }
    case "pictureInPicture": {
      const r = await pageTool<string>(live, "pip");
      if (r === "none") ui.toast("No video on this page", { icon: "film" });
      else if (r === "blocked") ui.toast("This video can't play in picture-in-picture", { icon: "film" });
      return;
    }
    case "switchTextDirection":
      return void pageTool(live, "textDirection");
    case "screenshot": {
      const path = await pageTool<string>(live, "screenshot");
      if (path)
        ui.toast("Screenshot saved", {
          icon: "check",
          timeout: 6000,
          action: { label: "Open", run: () => void api.openPath(path) },
        });
      return;
    }
    case "responsiveDesign": {
      const on = await pageTool<boolean>(live, "responsive");
      if (on !== undefined)
        ui.toast(on ? "Responsive design mode: 390 px wide, touch" : "Responsive design mode off", { icon: "code" });
      return;
    }
    case "taskManager":
      return void pageTool(live ?? Object.values(browser.tabs).find((t) => t.live), "taskManager");
    case "browserToolbox":
    case "browserConsole":
      return api.chromeDevtools().catch((e) => ui.toast(String(e), { icon: "alert" }));
    case "inspector":
    case "webConsole":
    case "jsDebugger":
    case "networkMonitor":
    case "styleEditor":
    case "performance":
    case "storage":
    case "domPanel":
    case "accessibility":
      return void pageTool(live, "devtools", DEVTOOLS_PANELS[action]);
    case "find":
      // On the notes page Ctrl+F searches the notes.
      if (!live && onNotesPage()) {
        notes.focusSearch++;
        return;
      }
      // On the settings page it searches the settings.
      if (!live && onSettingsPage()) {
        ui.settingsSearch++;
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
      ui.flyoutPinned = false;
      ui.topReveal = false;
      return settings.update({ compact: !settings.value.compact });
    case "toggleFloatingSidebar":
      // Zen: shows the floating sidebar of compact mode and keeps it until pressed again.
      if (!settings.value.compact)
        return ui.toast(`The floating sidebar is part of compact mode${shortcutHint("toggleCompact") ? ` (${shortcutHint("toggleCompact")})` : ""}`, { icon: "sidebar" });
      ui.flyoutPinned = !(ui.flyout && ui.flyoutPinned);
      ui.flyout = ui.flyoutPinned;
      return;
    case "toggleFloatingToolbar":
      // Zen: compact mode hides the toolbar too; it then floats in from the top edge.
      if (settings.layoutMode === "single")
        return ui.toast("With the single toolbar the address bar is in the sidebar", { icon: "sidebar" });
      ui.topReveal = false;
      return settings.update({
        compact: true,
        compactHideToolbar: !(settings.value.compact && settings.value.compactHideToolbar),
      });
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
    case "splitVertical":
      return splitShortcut("vsep");
    case "splitHorizontal":
      return splitShortcut("hsep");
    case "splitGrid":
      return splitShortcut("grid");
    case "unsplit":
      return browser.unsplit();
    case "toggleChatbot":
      return toggleChatbot();
    case "chooseChatbot":
      return chooseChatbot(false);
    case "newWindow":
      // From an incognito window a normal window opens, without that window's spaces.
      return api.windowOpen(incognito ? null : browser.snapshot()).catch((e) => ui.toast(String(e), { icon: "alert" }));
    case "closeWindow":
      return void getCurrentWindow().close();
    case "undoCloseWindow":
      if (!(await api.windowUndoClose().catch(() => false)))
        ui.toast("No recently closed windows", { icon: "window" });
      return;
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
    case "historySidebar": {
      // Ctrl+H: the sidebar panel on its history view, or closed again.
      const s = settings.value;
      if (s.bookmarksPanel && s.bookmarksPanelView === "history") return settings.update({ bookmarksPanel: false });
      return settings.update({ bookmarksPanel: true, bookmarksPanelView: "history" });
    }
    case "toggleSidebarPanel":
      // Firefox's sidebar toggle: the panel on whichever view it showed last.
      return settings.update({ bookmarksPanel: !settings.value.bookmarksPanel });
    case "clearData":
      return openInternal("settings", "privacy");
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
      if (m) return browser.activateIndex(Number(m[1]) - 1);
      const s = /^switchSpace(\d+)$/.exec(action);
      const space = s && browser.spaces[Number(s[1]) - 1];
      if (space) browser.switchSpace(space.id);
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
