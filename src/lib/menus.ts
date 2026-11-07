// Context menus for chrome UI elements (the page itself uses WebView2's menu).
import { api } from "./ipc";
import { browser, needsWebview } from "./browser.svelte";
import {
  copyText,
  openIncognito,
  openInternal,
  openNotes,
  runAction,
  shortcutHint,
  toggleSiteBlocking,
} from "./actions";
import {
  barPinned,
  bookmarkAllTabs,
  bookmarkPage,
  bookmarkTabDialog,
  bookmarkTabsDialog,
  copyNodes,
  deleteNodes,
  editNode,
  menuHow,
  newFolderDialog,
  openAll,
  openBookmark,
  pasteNodes,
  pickedTabs,
  toggleBookmarksBar,
  toggleBookmarksPanel,
} from "./bookmark-actions";
import { bookmarks } from "./bookmarks.svelte";
import { ROOTS, ROOT_TITLES, ROOT_TOOLBAR, bookmarksIn, nodeLabel } from "./bookmarks";
import { faviconFor } from "./favicons";
import { settings, type BookmarksBarMode } from "./settings.svelte";
import { ui, type MenuItem } from "./ui.svelte";
import type { BookmarkNode, Space, Tab } from "./types";
import { hostOf, isWebUrl, siteOf } from "./url";

/** The first shortcut bound to an action, for menu hints. */
const hintFor = shortcutHint;

// ---------------------------------------------------------------------------------------------
// Bookmarks

/** The icon of a built-in folder (Zen's: a boxed star, a boxed list, a plain folder). */
export function rootIcon(id: number): "bookmarkToolbar" | "bookmarkMenu" | "folder" {
  return id === ROOT_TOOLBAR ? "bookmarkToolbar" : id === ROOTS[1] ? "bookmarkMenu" : "folder";
}

/** A bookmark, folder or separator as a menu entry (folders open as submenus). */
export function bookmarkItem(n: BookmarkNode): MenuItem {
  if (n.kind === "separator") return { separator: true };
  if (n.kind === "folder") return { label: nodeLabel(n), icon: "folder", children: () => bookmarkFolderItems(n.id) };
  return {
    label: nodeLabel(n),
    site: { url: n.url, favicon: faviconFor(n.url) },
    run: (mods) => openBookmark(n.url, menuHow(mods)),
  };
}

/** What a folder's dropdown lists (in the toolbar and in the app menu): the bookmarks that show in
 *  this workspace, subfolders as submenus, and "Open all in tabs" at the end. */
export function bookmarkFolderItems(folderId: number): MenuItem[] {
  const folder = bookmarks.node(folderId);
  const list = bookmarks.visibleKids(folderId);
  if (!list.length) return [{ label: "(Empty)", heading: true }];
  const items = list.map(bookmarkItem);
  if (folder && list.filter((n) => n.kind === "bookmark").length > 1) {
    items.push({ separator: true }, { label: "Open all in tabs", icon: "tab", run: () => openAll(folder) });
  }
  return items;
}

/** The app menu's "Bookmarks" (Zen: … > Bookmarks): the page and tab actions, the toolbar and
 *  sidebar switches, the Library, the most recent bookmarks and the three built-in folders. */
export function bookmarksMenu(): MenuItem[] {
  const tab = browser.activeTab;
  const web = !!tab && isWebUrl(tab.url);
  const marked = web && bookmarks.has(tab!.url);
  const recent = bookmarks.recent(8);
  const panelOpen = settings.value.bookmarksPanel && settings.value.bookmarksPanelView === "bookmarks";
  const items: MenuItem[] = [
    {
      label: marked ? "Edit this bookmark" : "Bookmark this page",
      icon: marked ? "bookmarkStar" : "bookmark",
      hint: hintFor("bookmark"),
      disabled: !web,
      run: () => void bookmarkPage(tab),
    },
    { label: "Bookmark all tabs…", icon: "tab", hint: hintFor("bookmarkAllTabs"), run: () => bookmarkAllTabs() },
    { label: "Search bookmarks", icon: "search", run: () => settings.update({ bookmarksPanel: true, bookmarksPanelView: "bookmarks" }) },
    { separator: true },
    { label: "Bookmarks toolbar", icon: "bookmarkToolbar", hint: hintFor("toggleBookmarksBar"), checked: barPinned(), run: () => toggleBookmarksBar() },
    { label: "Bookmarks sidebar", icon: "sidebar", hint: hintFor("toggleBookmarksSidebar"), checked: panelOpen, run: () => toggleBookmarksPanel() },
    { label: "Manage bookmarks", icon: "library", hint: hintFor("bookmarks"), run: () => openInternal("bookmarks") },
    { separator: true },
  ];
  if (recent.length) items.push({ label: "Recent bookmarks", heading: true }, ...recent.map(bookmarkItem), { separator: true });
  for (const root of ROOTS) {
    items.push({ label: bookmarks.node(root)?.title || ROOT_TITLES[root], icon: rootIcon(root), children: () => bookmarkFolderItems(root) });
  }
  return items;
}

/** The context menu of a bookmark, folder or separator, wherever it is listed. `selection` is every
 *  node picked in a list (the Library), which Open and Delete then apply to. */
export function bookmarkNodeMenu(node: BookmarkNode, selection: BookmarkNode[] = []): MenuItem[] {
  const picked = selection.length > 1 && selection.some((n) => n.id === node.id) ? selection : [node];
  const marks = picked.filter((n) => n.kind === "bookmark");
  const removable = picked.filter((n) => n.parent !== null);
  const items: MenuItem[] = [];
  if (node.kind === "bookmark") {
    items.push(
      { label: marks.length > 1 ? `Open ${marks.length} bookmarks in new tabs` : "Open", icon: "external", run: () => (marks.length > 1 ? marks.forEach((m) => openBookmark(m.url, "background")) : openBookmark(node.url, "here")) },
      { label: "Open in new tab", icon: "plus", disabled: marks.length > 1, run: () => openBookmark(node.url, "tab") },
      { label: "Open in Glance", icon: "expand", disabled: marks.length > 1, run: () => openBookmark(node.url, "glance") },
      { label: "Open in incognito window", icon: "incognito", disabled: marks.length > 1, run: () => void openIncognito([node.url]) },
      { separator: true },
      { label: "Edit bookmark…", icon: "edit", disabled: picked.length > 1, run: () => editNode(node) },
      { label: "Copy link", icon: "link", disabled: picked.length > 1, run: () => void copyText(node.url) },
    );
  } else if (node.kind === "folder") {
    const here = bookmarksIn(bookmarks.index, node.id);
    items.push(
      { label: "Open all in tabs", icon: "tab", disabled: !here.length, run: () => openAll(node) },
      { separator: true },
      { label: "Rename…", icon: "edit", disabled: node.parent === null, run: () => editNode(node) },
      { label: "Sort by name", icon: "sort", disabled: bookmarks.kids(node.id).length < 2, run: () => void bookmarks.sortByName(node.id) },
      { label: "New folder…", icon: "plus", run: () => newFolderDialog(node.id) },
      { label: "New separator", icon: "minimize", run: () => void bookmarks.addSeparator(node.id) },
    );
  }
  if (node.kind !== "separator" || removable.length) {
    const target = node.kind === "folder" ? node.id : node.parent;
    items.push(
      { separator: true },
      { label: "Cut", icon: "scissors", disabled: !removable.length, run: () => void copyNodes(picked.map((n) => n.id), true) },
      { label: "Copy", icon: "copy", disabled: !removable.length, run: () => void copyNodes(picked.map((n) => n.id), false) },
      { label: "Paste", icon: "clipboard", disabled: !bookmarks.clip || target === null, run: () => target !== null && void pasteNodes(target, node.kind === "folder" ? undefined : node.position + 1) },
    );
  }
  if (removable.length) {
    if (items.length) items.push({ separator: true });
    const label = removable.length > 1 ? `Delete ${removable.length} items` : node.kind === "folder" ? "Delete folder" : node.kind === "separator" ? "Delete separator" : "Delete bookmark";
    items.push({ label, icon: "trash", danger: true, run: () => void deleteNodes(removable.map((n) => n.id)) });
  }
  return items;
}

/** The context menu of the bookmarks toolbar itself: when it shows, Other Bookmarks, new items. */
export function bookmarksBarMenu(): MenuItem[] {
  const mode = settings.value.bookmarksBar;
  const pick = (label: string, value: BookmarksBarMode): MenuItem => ({
    label,
    checked: mode === value,
    run: () => settings.update({ bookmarksBar: value }),
  });
  return [
    pick("Always show the toolbar", "always"),
    pick("Only show on a new tab", "newtab"),
    pick("Never show the toolbar", "never"),
    { separator: true },
    { label: "Show Other Bookmarks", checked: settings.value.bookmarksOther, run: () => settings.update({ bookmarksOther: !settings.value.bookmarksOther }) },
    { separator: true },
    { label: "Paste", icon: "clipboard", disabled: !bookmarks.clip, run: () => void pasteNodes(ROOT_TOOLBAR) },
    { separator: true },
    { label: "New folder…", icon: "plus", run: () => newFolderDialog(ROOT_TOOLBAR) },
    { label: "New separator", icon: "minimize", run: () => void bookmarks.addSeparator(ROOT_TOOLBAR) },
    { label: "Sort by name", icon: "sort", disabled: bookmarks.kids(ROOT_TOOLBAR).length < 2, run: () => void bookmarks.sortByName(ROOT_TOOLBAR) },
    { separator: true },
    { label: "Manage bookmarks", icon: "library", hint: hintFor("bookmarks"), run: () => openInternal("bookmarks") },
  ];
}

/** The menu of several tabs picked in the sidebar. */
function pickedTabsMenu(picked: Tab[]): MenuItem[] {
  return [
    { label: `Bookmark ${picked.length} tabs…`, icon: "bookmark", run: () => bookmarkTabsDialog(picked) },
    {
      label: "Reload tabs",
      icon: "reload",
      run: () => {
        for (const t of picked) if (t.live) void api.tabCommand(t.id, "reload");
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
