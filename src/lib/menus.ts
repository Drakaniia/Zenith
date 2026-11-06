// Context menus for chrome UI elements (the page itself uses WebView2's menu).
import { api } from "./ipc";
import { browser, needsWebview } from "./browser.svelte";
import {
  copyText,
  openIncognito,
  openInternal,
  runAction,
  toggleBookmark,
  toggleSiteBlocking,
} from "./actions";
import { bookmarks } from "./bookmarks.svelte";
import { settings } from "./settings.svelte";
import { ui, type MenuItem } from "./ui.svelte";
import type { Space, Tab } from "./types";
import { hostOf, isWebUrl, siteOf } from "./url";

export function tabMenu(
  tab: Tab,
  opts: { rename?: () => void } = {},
): MenuItem[] {
  const space = browser.activeSpace;
  const split = browser.activeSplit;
  const inSplit = !!split?.ids.includes(tab.id);
  const others = browser.spaces.filter((s) => s.id !== tab.spaceId);
  const active = browser.activeTab;
  const web = isWebUrl(tab.url);
  const blockingOff =
    web &&
    settings.value.blockerDisabledSites.includes(siteOf(hostOf(tab.url)));
  const items: MenuItem[] = [
    {
      label: "New tab below",
      icon: "plus",
      run: () => browser.openTab(undefined, { openerId: tab.id }),
    },
    { separator: true },
    {
      label: "Reload",
      icon: "reload",
      disabled: !tab.live,
      run: () => void api.tabCommand(tab.id, "reload"),
    },
    { label: "Duplicate", icon: "copy", run: () => browser.duplicate(tab.id) },
    ...(opts.rename
      ? [{ label: "Rename tab", icon: "edit", run: opts.rename } as MenuItem]
      : []),
    {
      label: "Copy link",
      icon: "link",
      disabled: !web,
      run: () => void copyText(tab.url),
    },
    {
      label: "Open in incognito window",
      icon: "incognito",
      disabled: !web,
      run: () => void openIncognito([tab.url]),
    },
    {
      label: bookmarks.has(tab.url) ? "Remove bookmark" : "Bookmark",
      icon: "star",
      disabled: !web,
      run: () => void toggleBookmark(tab),
    },
    { separator: true },
  ];
  if (!tab.essential) {
    items.push({
      label: tab.pinned ? "Unpin tab" : "Pin tab",
      icon: tab.pinned ? "unpin" : "pin",
      run: () => browser.togglePin(tab.id),
    });
  }
  if (tab.pinned && space) {
    const folders = space.folders ?? [];
    const inFolder = browser.folderOf(tab.id);
    items.push({
      label: "Move to folder",
      icon: "folder",
      children: [
        {
          label: "New folder",
          icon: "plus",
          run: () => browser.createFolder([tab.id]),
        },
        ...(folders.length ? [{ separator: true } as MenuItem] : []),
        ...folders.map((f) => ({
          label: f.name,
          icon: "folder" as const,
          checked: inFolder?.id === f.id,
          run: () => browser.moveToFolder(tab.id, f.id),
        })),
        ...(inFolder
          ? [
              { separator: true } as MenuItem,
              {
                label: "Remove from folder",
                run: () => browser.moveToFolder(tab.id, null),
              },
            ]
          : []),
      ],
    });
  }
  items.push({
    label: tab.essential ? "Remove from Essentials" : "Add to Essentials",
    icon: tab.essential ? "essentialRemove" : "essentialAdd",
    disabled: !tab.essential && browser.essentials.length >= 12,
    run: () => browser.toggleEssential(tab.id),
  });
  if (
    (tab.pinned || tab.essential) &&
    tab.pinnedUrl &&
    tab.pinnedUrl !== tab.url
  ) {
    items.push(
      {
        label: "Reset to pinned page",
        icon: "home",
        run: () => browser.resetPinned(tab.id),
      },
      {
        label: "Replace pinned page with current",
        icon: "pin",
        run: () => (tab.pinnedUrl = tab.url),
      },
    );
  }
  items.push(
    {
      label: tab.muted ? "Unmute tab" : "Mute tab",
      icon: tab.muted ? "volume" : "volumeOff",
      run: () => browser.setMuted(tab.id, !tab.muted),
    },
    inSplit
      ? {
          label: "Exit split view",
          icon: "split",
          run: () => browser.unsplit(),
        }
      : {
          label: "Split with current tab",
          icon: "split",
          disabled: !active || active.id === tab.id,
          run: () => active && browser.splitWith(active.id, tab.id),
        },
  );
  if (others.length && !tab.essential) {
    items.push({
      label: "Move to space",
      icon: "layers",
      children: others.map((s) => ({
        label: `${s.icon}  ${s.name}`,
        run: () => browser.moveToSpace(tab.id, s.id),
      })),
    });
  }
  if (web) {
    items.push({
      label: blockingOff
        ? "Turn on blocking for this site"
        : "Turn off blocking for this site",
      icon: blockingOff ? "shieldCheck" : "shieldOff",
      run: () => toggleSiteBlocking(tab),
    });
  }
  items.push(
    {
      label: "Put to sleep",
      icon: "moonSleep",
      disabled:
        !tab.live || !needsWebview(tab.url) || browser.isVisible(tab.id),
      run: () => void browser.sleep(tab.id),
    },
    { separator: true },
    {
      label: tab.pinned || tab.essential ? "Unload tab" : "Close tab",
      icon: "close",
      hint: "Ctrl+W",
      run: () => browser.closeTab(tab.id),
    },
  );
  if (!tab.pinned && !tab.essential && space) {
    items.push(
      { label: "Close other tabs", run: () => browser.closeOthers(tab.id) },
      { label: "Close tabs below", run: () => browser.closeBelow(tab.id) },
    );
  }
  return items;
}

export function spaceMenu(space: Space): MenuItem[] {
  return [
    {
      label: "Edit space…",
      icon: "edit",
      run: () => (ui.spaceEditor = { spaceId: space.id }),
    },
    {
      label: "New space…",
      icon: "plus",
      run: () => (ui.spaceEditor = { spaceId: null }),
    },
    {
      label: "New folder",
      icon: "folder",
      run: () => browser.createFolder([], space.id),
    },
    { separator: true },
    {
      label: "Unload all tabs",
      icon: "moonSleep",
      run: () => browser.unloadSpace(space.id),
    },
    {
      label: "Close unpinned tabs",
      icon: "eraser",
      run: () => browser.clearSpace(space.id),
    },
    {
      label: "Delete space",
      icon: "trash",
      danger: true,
      disabled: browser.spaces.length <= 1,
      run: () => browser.deleteSpace(space.id),
    },
  ];
}

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
      hint: "Ctrl+Alt+S",
      disabled: !t,
      run: () => browser.toggleSplit(),
    },
    { separator: true },
    {
      label: "Find in page",
      icon: "search",
      hint: "Ctrl+F",
      disabled: !live,
      run: () => void runAction("find"),
    },
    {
      label: "Zoom in",
      icon: "zoomIn",
      hint: "Ctrl+Plus",
      disabled: !live,
      run: () => void runAction("zoomIn"),
    },
    {
      label: "Zoom out",
      icon: "zoomOut",
      hint: "Ctrl+Minus",
      disabled: !live,
      run: () => void runAction("zoomOut"),
    },
    {
      label: "Reset zoom",
      hint: "Ctrl+0",
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
      hint: "Ctrl+P",
      disabled: !live,
      run: () => void runAction("print"),
    },
    {
      label: "Save page as…",
      icon: "save",
      hint: "Ctrl+S",
      disabled: !live,
      run: () => void runAction("savePage"),
    },
    {
      label: "View source",
      icon: "code",
      hint: "Ctrl+U",
      disabled: !live,
      run: () => void runAction("viewSource"),
    },
    {
      label: "Developer tools",
      icon: "code",
      hint: "F12",
      disabled: !live,
      run: () => void runAction("devtools"),
    },
    { separator: true },
    {
      label: "Reload without cache",
      icon: "reload",
      hint: "Ctrl+F5",
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

/** The sidebar's "+" button (Zen: zenCreateNewPopup). */
export function createNewMenu(): MenuItem[] {
  const space = browser.activeSpace;
  return [
    {
      label: "New tab",
      icon: "plus",
      hint: "Ctrl+T",
      run: () => ui.openPalette("newTab"),
    },
    {
      label: "New split view",
      icon: "split",
      hint: "Ctrl+Alt+S",
      disabled: !browser.activeTab,
      run: () => browser.toggleSplit(),
    },
    {
      label: "New folder",
      icon: "folder",
      disabled: !space,
      run: () => space && browser.createFolder([], space.id),
    },
    {
      label: "New space",
      icon: "layers",
      run: () => (ui.spaceEditor = { spaceId: null }),
    },
    {
      label: "New incognito window",
      icon: "incognito",
      hint: "Ctrl+Shift+N",
      run: () => void openIncognito(),
    },
    { separator: true },
    {
      label: "Reopen closed tab",
      icon: "history",
      hint: "Ctrl+Shift+T",
      disabled: !browser.closed.length,
      run: () => browser.reopenClosed(),
    },
  ];
}

export function appMenu(): MenuItem[] {
  const live = browser.activeTab?.live ? browser.activeTab : undefined;
  return [
    {
      label: "New tab",
      icon: "plus",
      hint: "Ctrl+T",
      run: () => ui.openPalette("newTab"),
    },
    {
      label: "New space",
      icon: "layers",
      run: () => (ui.spaceEditor = { spaceId: null }),
    },
    {
      label: "New incognito window",
      icon: "incognito",
      hint: "Ctrl+Shift+N",
      run: () => void openIncognito(),
    },
    {
      label: "Reopen closed tab",
      icon: "history",
      hint: "Ctrl+Shift+T",
      disabled: !browser.closed.length,
      run: () => browser.reopenClosed(),
    },
    { separator: true },
    { zoom: true, disabled: !live },
    { separator: true },
    {
      label: "Find in page…",
      icon: "search",
      hint: "Ctrl+F",
      disabled: !live,
      run: () => void runAction("find"),
    },
    {
      label: "Print…",
      icon: "printer",
      hint: "Ctrl+P",
      disabled: !live,
      run: () => void runAction("print"),
    },
    {
      label: "Save page as…",
      icon: "save",
      hint: "Ctrl+S",
      disabled: !live,
      run: () => void runAction("savePage"),
    },
    { separator: true },
    {
      label: "History",
      icon: "history",
      hint: "Ctrl+H",
      run: () => openInternal("history"),
    },
    {
      label: "Bookmarks",
      icon: "bookmark",
      hint: "Ctrl+Shift+O",
      run: () => openInternal("bookmarks"),
    },
    {
      label: "Downloads",
      icon: "download",
      hint: "Ctrl+J",
      run: () => openInternal("downloads"),
    },
    {
      label: "Extensions",
      icon: "puzzle",
      run: () => openInternal("extensions"),
    },
    { separator: true },
    {
      label: "Compact mode",
      icon: "sidebar",
      hint: "Ctrl+B",
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
      hint: "Ctrl+,",
      run: () => openInternal("settings"),
    },
    { separator: true },
    { label: "Quit", hint: "Ctrl+Shift+Q", run: () => void runAction("quit") },
  ];
}
