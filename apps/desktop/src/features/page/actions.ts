// Actions on the current page: navigation, reload, find, zoom, page tools and developer tools.
import { registerActions, runAction } from "../../platform/actions";
import { shell } from "../../platform/shell";
import { webviewApi } from "../../platform/webview-api";
import { copyText } from "../../shared/clipboard";
import type { IconName } from "../../shared/ui/icons";
import { ui } from "../../shared/ui/ui.svelte";
import { displayHost, hostOf, internalPage, isSecure, isWebUrl, siteOf } from "../../shared/url";
import { toggleSiteBlocking } from "../blocker/site-blocking";
import { tabsApi } from "../browser/api";
import { browser, tabTitle } from "../browser/browser.svelte";
import { liveTab, openInternal, targetTab } from "../browser/target";
import type { Tab } from "../browser/types";
import { notes } from "../notes/notes.svelte";
import { settings } from "../settings/settings.svelte";
import { pageApi } from "./api";

const ZOOM_LEVELS = [
  0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4,
  5,
];

const onNotesPage = () => internalPage(browser.activeTab?.url ?? "") === "notes";
const onSettingsPage = () => internalPage(browser.activeTab?.url ?? "") === "settings";

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
  void tabsApi.tabCommand(tab.id, "zoom", z);
}

export function openFind(tab: Tab | undefined) {
  if (!tab?.live) return;
  ui.find = { ...ui.find, open: true, tabId: tab.id };
  void webviewApi.focusView(null);
  requestAnimationFrame(() => document.getElementById("find-input")?.focus());
}

/** Zoom for the app menu's zoom row: -1, 0 (reset) or +1 step. */
export function zoomActive(dir: -1 | 0 | 1) {
  zoomStep(browser.activeTab, dir);
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

/** Runs one of the backend's page tools (tab.rs `command`) and reports failures. */
async function pageTool<T>(tab: Tab | undefined, cmd: string, arg?: unknown): Promise<T | undefined> {
  if (!tab?.live) {
    ui.toast("Open a web page first", { icon: "info" });
    return undefined;
  }
  try {
    return await tabsApi.tabCommand<T>(tab.id, cmd, arg);
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

const devtoolsPanel = (source: string | null | undefined, action: string) =>
  void pageTool(liveTab(source), "devtools", DEVTOOLS_PANELS[action]);

const findStep = (source: string | null | undefined, action: string) => {
  if (!ui.find.open) return openFind(liveTab(source));
  if (ui.find.query)
    void pageApi.find(ui.find.tabId, action === "findNext" ? "next" : "prev");
};

registerActions(
  {
    back: (source) => {
      const live = liveTab(source);
      return live && tabsApi.tabCommand(live.id, "back");
    },
    forward: (source) => {
      const live = liveTab(source);
      return live && tabsApi.tabCommand(live.id, "forward");
    },
    // Esc in a page that is still loading stops it; the page gets the key either way.
    escape: (source) => {
      const live = liveTab(source);
      return live?.loading && tabsApi.tabCommand(live.id, "stop");
    },
    reload: (source) => {
      const tab = targetTab(source);
      if (tab?.crashed) return browser.reviveCrashed(tab.id);
      const live = liveTab(source);
      return live && tabsApi.tabCommand(live.id, "reload");
    },
    hardReload: (source) => {
      const live = liveTab(source);
      return live && tabsApi.tabCommand(live.id, "hardReload");
    },
    stop: (source) => {
      const live = liveTab(source);
      return live && tabsApi.tabCommand(live.id, "stop");
    },
    copyUrl: async (source) => {
      const tab = targetTab(source);
      if (tab && !tab.url.startsWith("zenith://")) {
        await copyText(tab.url);
        ui.toast("Link copied", { icon: "link" });
      }
    },
    copyUrlMarkdown: (source) => copyMarkdownLink(targetTab(source)),
    pageInfo: (source) => pageInfo(targetTab(source)),
    readerMode: async (source) => {
      const r = await pageTool<string>(liveTab(source), "reader");
      if (r === "none") ui.toast("Reader view isn't available for this page", { icon: "note" });
    },
    pictureInPicture: async (source) => {
      const r = await pageTool<string>(liveTab(source), "pip");
      if (r === "none") ui.toast("No video on this page", { icon: "film" });
      else if (r === "blocked") ui.toast("This video can't play in picture-in-picture", { icon: "film" });
    },
    switchTextDirection: (source) => void pageTool(liveTab(source), "textDirection"),
    screenshot: async (source) => {
      const path = await pageTool<string>(liveTab(source), "screenshot");
      if (path)
        ui.toast("Screenshot saved", {
          icon: "check",
          timeout: 6000,
          action: { label: "Open", run: () => void shell.openPath(path) },
        });
    },
    responsiveDesign: async (source) => {
      const on = await pageTool<boolean>(liveTab(source), "responsive");
      if (on !== undefined)
        ui.toast(on ? "Responsive design mode: 390 px wide, touch" : "Responsive design mode off", { icon: "code" });
    },
    taskManager: (source) =>
      void pageTool(liveTab(source) ?? Object.values(browser.tabs).find((t) => t.live), "taskManager"),
    inspector: devtoolsPanel,
    webConsole: devtoolsPanel,
    jsDebugger: devtoolsPanel,
    networkMonitor: devtoolsPanel,
    styleEditor: devtoolsPanel,
    performance: devtoolsPanel,
    storage: devtoolsPanel,
    domPanel: devtoolsPanel,
    accessibility: devtoolsPanel,
    find: (source) => {
      const live = liveTab(source);
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
    },
    findNext: findStep,
    findPrev: findStep,
    zoomIn: (source) => zoomStep(liveTab(source), 1),
    zoomOut: (source) => zoomStep(liveTab(source), -1),
    zoomReset: (source) => zoomStep(liveTab(source), 0),
    print: (source) => {
      const live = liveTab(source);
      return live && tabsApi.tabCommand(live.id, "print");
    },
    savePage: (source) => {
      const live = liveTab(source);
      return live && tabsApi.tabCommand(live.id, "saveAs");
    },
    devtools: (source) => {
      const live = liveTab(source);
      return live && tabsApi.tabCommand(live.id, "devtools");
    },
    viewSource: async (source) => {
      const live = liveTab(source);
      if (live) {
        const src = await tabsApi.tabCommand<string>(live.id, "viewSource");
        if (src) browser.openTab(src, { openerId: live.id });
      }
    },
  },
  { repeatable: ["zoomIn", "zoomOut", "findNext", "findPrev", "back", "forward"] },
);
