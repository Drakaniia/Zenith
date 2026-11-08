// Menus of the sidebar: tabs, picked tabs, spaces, the sidebar itself and its "+" button.
import { tabsApi } from "../browser/api";
import { browser, needsWebview } from "../browser/browser.svelte";
import { copyText } from "../../shared/clipboard";
import { openIncognito } from "../window/actions";
import { openNotes } from "../notes/actions";
import { runAction } from "../../platform/actions";
import { shortcutHint } from "../shortcuts/hints";
import { toggleSiteBlocking } from "../blocker/site-blocking";
import { bookmarkTabDialog, bookmarkTabsDialog, pickedTabs } from "../bookmarks/bookmark-actions";
import { bookmarks } from "../bookmarks/bookmarks.svelte";
import { settings } from "../settings/settings.svelte";
import { ui } from "../../shared/ui/ui.svelte";
import type { MenuItem } from "../../shared/ui/menu";
import type { Space, Tab } from "../browser/types";
import { hostOf, isWebUrl, siteOf } from "../../shared/url";
import { SIDEBAR_POSITIONS } from "./sidebar-position";

/** The first shortcut bound to an action, for menu hints. */
const hintFor = shortcutHint;

/** The menu of several tabs picked in the sidebar. */
function pickedTabsMenu(picked: Tab[]): MenuItem[] {
  return [
    { label: `Bookmark ${picked.length} tabs…`, icon: "bookmark", run: () => bookmarkTabsDialog(picked) },
    {
      label: "Reload tabs",
      icon: "reload",
      run: () => {
        for (const t of picked) if (t.live) void tabsApi.tabCommand(t.id, "reload");
      },
    },
    { separator: true },
    {
      label: `Close ${picked.length} tabs`,
      icon: "close",
      run: () => {
        ui.clearTabSelection();
        for (const t of picked) browser.closeTab(t.id);
      },
    },
  ];
}

export function tabMenu(
  tab: Tab,
  opts: { rename?: () => void } = {},
): MenuItem[] {
  // Right-clicking one of several picked tabs (Ctrl+click, Shift+click) acts on all of them.
  const picked = ui.selectedTabs.includes(tab.id) ? pickedTabs() : [];
  if (picked.length > 1) return pickedTabsMenu(picked);
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
      run: () => void tabsApi.tabCommand(tab.id, "reload"),
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
      label: bookmarks.has(tab.url) ? "Edit bookmark…" : "Bookmark tab…",
      icon: bookmarks.has(tab.url) ? "bookmarkStar" : "bookmark",
      disabled: !web,
      run: () => bookmarkTabDialog(tab),
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
      hint: hintFor("closeTab"),
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

/** The sidebar's (or tab strip's) own menu, from a right-click on an empty part of it. */
export function sidebarMenu(): MenuItem[] {
  return [
    {
      label: "New Tab",
      icon: "plus",
      hint: shortcutHint("newTab"),
      run: () => ui.openPalette("newTab"),
    },
    { separator: true },
    {
      label: "Move sidebar to",
      icon: "sidebar",
      children: SIDEBAR_POSITIONS.map((p) => ({
        label: p.label,
        checked: settings.value.sidebarPosition === p.value,
        run: () => {
          ui.flyout = false;
          ui.flyoutPinned = false;
          settings.update({ sidebarPosition: p.value });
        },
      })),
    },
    {
      label: "Compact mode",
      icon: "sidebar",
      hint: shortcutHint("toggleCompact"),
      checked: settings.value.compact,
      run: () => void runAction("toggleCompact"),
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
      hint: hintFor("newTab"),
      run: () => ui.openPalette("newTab"),
    },
    {
      label: "New split view",
      icon: "split",
      hint: hintFor("splitVertical"),
      disabled: !browser.activeTab,
      run: () => void runAction("splitVertical"),
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
      label: "New note",
      icon: "note",
      run: () => openNotes({ create: true }),
    },
    {
      label: "New incognito window",
      icon: "incognito",
      hint: hintFor("newIncognitoWindow"),
      run: () => void openIncognito(),
    },
    { separator: true },
    {
      label: "Reopen closed tab",
      icon: "history",
      hint: hintFor("reopenClosed"),
      disabled: !browser.closed.length,
      run: () => browser.reopenClosed(),
    },
  ];
}
