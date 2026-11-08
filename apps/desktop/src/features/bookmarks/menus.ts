// Menus of bookmarks: entries, folder dropdowns, the app menu's Bookmarks, context menus.
import { browser } from "../browser/browser.svelte";
import { copyText } from "../../shared/clipboard";
import { openIncognito } from "../window/actions";
import { openInternal } from "../browser/target";
import { shortcutHint } from "../shortcuts/hints";
import {
  barPinned,
  bookmarkAllTabs,
  bookmarkPage,
  copyNodes,
  deleteNodes,
  editNode,
  menuHow,
  newFolderDialog,
  openAll,
  openBookmark,
  pasteNodes,
  toggleBookmarksBar,
  toggleBookmarksPanel,
} from "./bookmark-actions";
import { bookmarks } from "./bookmarks.svelte";
import { ROOTS, ROOT_TITLES, ROOT_TOOLBAR, bookmarksIn, nodeLabel } from "./bookmarks";
import { faviconFor } from "../../shared/favicons";
import { settings, type BookmarksBarMode } from "../settings/settings.svelte";
import type { MenuItem } from "../../shared/ui/menu";
import type { BookmarkNode } from "./types";
import { isWebUrl } from "../../shared/url";

/** The first shortcut bound to an action, for menu hints. */
const hintFor = shortcutHint;

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
