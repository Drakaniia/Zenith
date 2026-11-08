// Menus of the toolbar: the page menu and the app menu.
import { browser } from "../browser/browser.svelte";
import { copyText } from "../../shared/clipboard";
import { openIncognito } from "../window/actions";
import { openInternal } from "../browser/target";
import { openNotes } from "../notes/actions";
import { runAction } from "../../platform/actions";
import { shortcutHint } from "../shortcuts/hints";
import { bookmarksMenu } from "../bookmarks/menus";
import { settings } from "../settings/settings.svelte";
import { ui } from "../../shared/ui/ui.svelte";
import type { MenuItem } from "../../shared/ui/menu";
import { isWebUrl } from "../../shared/url";

/** The first shortcut bound to an action, for menu hints. */
const hintFor = shortcutHint;

/** Page actions for the active tab (address bar "⋯" and the toolbar's page menu). */
export function pageMenu(): MenuItem[] {
  const t = browser.activeTab;
  const live = !!t?.live;
  const web = !!t && isWebUrl(t.url);
  const zoom = t?.live ? Math.round((t.zoom || 1) * 100) : 100;
  return [
    {
      label: browser.activeSplit ? "Exit split view" : "Split view",
      icon: browser.activeSplit ? "unsplit" : "split",
      hint: hintFor(browser.activeSplit ? "unsplit" : "splitVertical"),
      disabled: !t,
      run: () => void runAction(browser.activeSplit ? "unsplit" : "splitVertical"),
    },
    { separator: true },
    {
      label: "Find in page",
      icon: "search",
      hint: hintFor("find"),
      disabled: !live,
      run: () => void runAction("find"),
    },
    {
      label: "Zoom in",
      icon: "zoomIn",
      hint: hintFor("zoomIn"),
      disabled: !live,
      run: () => void runAction("zoomIn"),
    },
    {
      label: "Zoom out",
      icon: "zoomOut",
      hint: hintFor("zoomOut"),
      disabled: !live,
      run: () => void runAction("zoomOut"),
    },
    {
      label: "Reset zoom",
      hint: hintFor("zoomReset"),
      disabled: !live || zoom === 100,
      run: () => void runAction("zoomReset"),
    },
    { separator: true },
    {
      label: "Copy link",
      icon: "link",
      disabled: !web,
      run: () => t && void copyText(t.url),
    },
    {
      label:
        (t?.media ?? 0) > 0
          ? `Download video (${t!.media} found)…`
          : "Download video…",
      icon: "film",
      disabled: !live || !web,
      run: () => void runAction("downloadMedia"),
    },
    {
      label: "Print…",
      icon: "printer",
      hint: hintFor("print"),
      disabled: !live,
      run: () => void runAction("print"),
    },
    {
      label: "Save page as…",
      icon: "save",
      hint: hintFor("savePage"),
      disabled: !live,
      run: () => void runAction("savePage"),
    },
    {
      label: "View source",
      icon: "code",
      hint: hintFor("viewSource"),
      disabled: !live,
      run: () => void runAction("viewSource"),
    },
    {
      label: "Developer tools",
      icon: "code",
      hint: hintFor("devtools"),
      disabled: !live,
      run: () => void runAction("devtools"),
    },
    { separator: true },
    {
      label: "Reader view",
      icon: "note",
      hint: hintFor("readerMode"),
      disabled: !live || !web,
      run: () => void runAction("readerMode"),
    },
    {
      label: "Picture-in-picture",
      icon: "film",
      hint: hintFor("pictureInPicture"),
      disabled: !live || !web,
      run: () => void runAction("pictureInPicture"),
    },
    {
      label: "Take a screenshot",
      icon: "save",
      hint: hintFor("screenshot"),
      disabled: !live,
      run: () => void runAction("screenshot"),
    },
    {
      label: "Page info",
      icon: "info",
      hint: hintFor("pageInfo"),
      disabled: !t,
      run: () => void runAction("pageInfo"),
    },
    { separator: true },
    {
      label: "Reload without cache",
      icon: "reload",
      hint: hintFor("hardReload"),
      disabled: !live,
      run: () => void runAction("hardReload"),
    },
    {
      label: "Put tab to sleep",
      icon: "moonSleep",
      disabled: !live,
      run: () => void runAction("sleepTab"),
    },
  ];
}

export function appMenu(): MenuItem[] {
  const live = browser.activeTab?.live ? browser.activeTab : undefined;
  return [
    {
      label: "New tab",
      icon: "plus",
      hint: hintFor("newTab"),
      run: () => ui.openPalette("newTab"),
    },
    {
      label: "New window",
      icon: "window",
      hint: hintFor("newWindow"),
      run: () => void runAction("newWindow"),
    },
    {
      label: "New space",
      icon: "layers",
      run: () => (ui.spaceEditor = { spaceId: null }),
    },
    {
      label: "New incognito window",
      icon: "incognito",
      hint: hintFor("newIncognitoWindow"),
      run: () => void openIncognito(),
    },
    {
      label: "Reopen closed tab",
      icon: "history",
      hint: hintFor("reopenClosed"),
      disabled: !browser.closed.length,
      run: () => browser.reopenClosed(),
    },
    { separator: true },
    { zoom: true, disabled: !live },
    { separator: true },
    {
      label: "Find in page…",
      icon: "search",
      hint: hintFor("find"),
      disabled: !live,
      run: () => void runAction("find"),
    },
    {
      label: "Print…",
      icon: "printer",
      hint: hintFor("print"),
      disabled: !live,
      run: () => void runAction("print"),
    },
    {
      label: "Save page as…",
      icon: "save",
      hint: hintFor("savePage"),
      disabled: !live,
      run: () => void runAction("savePage"),
    },
    { separator: true },
    {
      label: "History",
      icon: "history",
      hint: hintFor("history"),
      run: () => openInternal("history"),
    },
    {
      label: "Bookmarks",
      icon: "bookmark",
      children: () => bookmarksMenu(),
    },
    {
      label: "Downloads",
      icon: "download",
      hint: hintFor("downloads"),
      run: () => openInternal("downloads"),
    },
    {
      label: "Extensions",
      icon: "puzzle",
      hint: hintFor("extensions"),
      run: () => openInternal("extensions"),
    },
    {
      label: "Passwords",
      icon: "key",
      run: () => openInternal("passwords"),
    },
    {
      label: "Notes",
      icon: "note",
      hint: hintFor("openNotes"),
      run: () => openNotes(),
    },
    { separator: true },
    {
      label: "Compact mode",
      icon: "sidebar",
      hint: hintFor("toggleCompact"),
      checked: settings.value.compact,
      run: () => settings.update({ compact: !settings.value.compact }),
    },
    {
      label: "Layout",
      icon: "window",
      children: (
        [
          ["single", "Single toolbar"],
          ["multiple", "Multiple toolbars"],
          ["collapsed", "Collapsed toolbar"],
        ] as const
      ).map(([mode, label]) => ({
        label,
        checked: settings.value.toolbarMode === mode,
        run: () => settings.update({ toolbarMode: mode }),
      })),
    },
    {
      label: "Settings",
      icon: "settings",
      hint: hintFor("settings"),
      run: () => openInternal("settings"),
    },
    { separator: true },
    { label: "Quit", hint: hintFor("quit"), run: () => void runAction("quit") },
  ];
}
