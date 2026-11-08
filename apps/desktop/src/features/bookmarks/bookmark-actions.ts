// What the user does with bookmarks, shared by the toolbar, sidebar, Library, menus, popups and
// shortcuts. The menus that offer these are built in menus.ts.
import { api } from "../../platform/ipc";
import { browser } from "../browser/browser.svelte";
import { bookmarks } from "./bookmarks.svelte";
import { settings } from "../settings/settings.svelte";
import { rememberFavicon } from "../../shared/favicons";
import { ui, type AnchorRect, type MenuMods } from "../../shared/ui/ui.svelte";
import { bookmarksIn, nodeLabel } from "./bookmarks";
import { isNewTab, isWebUrl } from "../../shared/url";
import type { BookmarkNode, Tab } from "../../shared/types";

// ---------------------------------------------------------------------------------------------
// Opening

/** In the tab you are on, in a new tab (in front or behind it), or in Glance. */
export type OpenHow = "here" | "tab" | "background" | "glance";

/** The same modifiers as links in a page (host/tab.rs): Alt opens Glance, Ctrl or the middle
 *  button a background tab, and Shift a new tab in front. In Zen the Glance key is the one
 *  chosen in its Glance settings (Ctrl by default); Zenith's is Alt. */
export function openHow(m: { ctrl?: boolean; shift?: boolean; alt?: boolean; middle?: boolean }): OpenHow {
  if (m.middle || m.ctrl) return "background";
  if (m.alt) return "glance";
  if (m.shift) return "tab";
  return "here";
}

export const mouseHow = (e: MouseEvent) =>
  openHow({ ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey, alt: e.altKey, middle: e.button === 1 });

export const menuHow = (m?: MenuMods) => openHow(m ?? {});

export function openBookmark(url: string, how: OpenHow = "here") {
  const tab = browser.activeTab;
  if (how === "glance") return void browser.openGlance(url, tab?.id);
  if (how === "tab") return void browser.openTab(url);
  if (how === "background") return void browser.openTab(url, { activate: false, openerId: tab?.id });
  if (!tab) return void browser.openTab(url);
  browser.navigate(tab, url);
}

/** Opens a folder's bookmarks (not its subfolders') in new tabs, the first one in front. */
export function openAll(folder: BookmarkNode) {
  const marks = bookmarksIn(bookmarks.index, folder.id);
  if (!marks.length) return;
  if (marks.length > 25 && !window.confirm(`Open all ${marks.length} bookmarks in new tabs?`)) return;
  const opener = browser.activeTab?.id;
  marks.forEach((m, i) => browser.openTab(m.url, { activate: i === 0, openerId: opener }));
}

// ---------------------------------------------------------------------------------------------
// The bookmark button and its panel

/** The rectangle the "Edit bookmark" panel hangs from: the page's bookmark button when it is on
 *  screen, else the address bar, else the top of the page area. */
export function bookmarkAnchor(): AnchorRect {
  const visible = (el: Element): AnchorRect | null => {
    const r = el.getBoundingClientRect();
    return r.width > 1 && r.height > 1 ? { left: r.left, right: r.right, top: r.top, bottom: r.bottom } : null;
  };
  for (const sel of ['[data-action="bookmark"]', ".pill"]) {
    for (const el of document.querySelectorAll(sel)) {
      const r = visible(el);
      if (r) return r;
    }
  }
  const mid = window.innerWidth / 2;
  return { left: mid - 20, right: mid + 20, top: 40, bottom: 48 };
}

/** The star button, Ctrl+D and the tab menu: bookmarks the page right away (in the folder used
 *  last) and opens the "Edit bookmark" panel; on a page that is already bookmarked, just opens the
 *  panel. */
export async function bookmarkPage(tab: Tab | undefined = browser.activeTab, anchor?: AnchorRect) {
  if (!tab || !isWebUrl(tab.url)) return;
  let id = bookmarks.find(tab.url)[0]?.id;
  if (id === undefined) {
    rememberFavicon(tab.url, tab.favicon);
    try {
      id = await bookmarks.addBookmark(tab.url, tab.customTitle || tab.title);
    } catch (e) {
      ui.toast(`Couldn't bookmark this page: ${e}`, { icon: "alert" });
      return;
    }
  }
  ui.menu = null;
  ui.bookmarkPopup = { id, anchor: anchor ?? bookmarkAnchor() };
}

/** "Bookmark Tab…": the detailed dialog (name, folder, tags, keyword); the bookmark is only made
 *  when it is saved. On a page that is already bookmarked it edits that bookmark. */
export function bookmarkTabDialog(tab: Tab | undefined = browser.activeTab) {
  if (!tab || !isWebUrl(tab.url)) return;
  rememberFavicon(tab.url, tab.favicon);
  const existing = bookmarks.find(tab.url)[0];
  ui.menu = null;
  ui.bookmarkPopup = null;
  ui.bookmarkDialog = existing
    ? { kind: "bookmark", id: existing.id, url: existing.url, title: existing.title, favicon: tab.favicon }
    : { kind: "bookmark", url: tab.url, title: tab.customTitle || tab.title, favicon: tab.favicon };
}

/** "Bookmark Tabs…" / "Bookmark All Tabs": the given tabs into a new folder. */
export function bookmarkTabsDialog(tabs: Tab[]) {
  const pages = tabs.filter((t) => isWebUrl(t.url));
  if (!pages.length) {
    ui.toast("There are no web pages to bookmark", { icon: "bookmark" });
    return;
  }
  for (const t of pages) rememberFavicon(t.url, t.favicon);
  ui.menu = null;
  ui.bookmarkPopup = null;
  ui.bookmarkDialog = {
    kind: "tabs",
    tabs: pages.map((t) => ({ url: t.url, title: t.customTitle || t.title, favicon: t.favicon })),
  };
}

/** Every tab of the space on screen (Essentials, pinned and regular ones). */
export function bookmarkAllTabs() {
  bookmarkTabsDialog(browser.order().map((id) => browser.tabs[id]));
}

/** The tabs picked in the sidebar that still exist, in the order they are shown. */
export function pickedTabs(): Tab[] {
  const picked = new Set(ui.selectedTabs);
  return browser
    .order()
    .filter((id) => picked.has(id))
    .map((id) => browser.tabs[id]);
}

export function editNode(node: BookmarkNode) {
  ui.menu = null;
  ui.bookmarkPopup = null;
  if (node.kind === "folder") ui.bookmarkDialog = { kind: "folder", id: node.id };
  else if (node.kind === "bookmark") ui.bookmarkDialog = { kind: "bookmark", id: node.id, url: node.url, title: node.title };
}

export function newFolderDialog(parent?: number) {
  ui.menu = null;
  ui.bookmarkDialog = { kind: "folder", parent };
}

/** Cut or copy nodes into the app's bookmark clipboard; one copied bookmark's address also goes to the
 *  system clipboard. */
export function copyNodes(ids: number[], cut = false) {
  const n = bookmarks.copyNodes(ids, cut);
  if (!n) return;
  const only = ids.length === 1 ? bookmarks.node(ids[0]) : undefined;
  if (!cut && only?.kind === "bookmark") void navigator.clipboard?.writeText(only.url).catch(() => {});
}

export async function pasteNodes(parent: number, index?: number) {
  try {
    const n = await bookmarks.paste(parent, index);
    if (n) ui.toast(n === 1 ? "Pasted" : `Pasted ${n} items`, { icon: "clipboard" });
  } catch (e) {
    ui.toast(`Couldn't paste: ${e}`, { icon: "alert" });
  }
}

/** Deletes nodes and offers to undo it. */
export async function deleteNodes(ids: number[]) {
  const nodes = ids.map((id) => bookmarks.node(id)).filter((n): n is BookmarkNode => !!n && n.parent !== null);
  if (!nodes.length) return;
  try {
    const removed = await bookmarks.remove(nodes.map((n) => n.id));
    const text =
      nodes.length > 1
        ? `${nodes.length} bookmarks removed`
        : nodes[0].kind === "folder"
          ? `Folder “${nodeLabel(nodes[0])}” removed`
          : nodes[0].kind === "separator"
            ? "Separator removed"
            : "Bookmark removed";
    ui.toast(text, {
      icon: "trash",
      timeout: 6000,
      action: { label: "Undo", run: () => void bookmarks.undoRemoval(removed).catch((e) => ui.toast(String(e), { icon: "alert" })) },
    });
  } catch (e) {
    ui.toast(`Couldn't remove: ${e}`, { icon: "alert" });
  }
}

// ---------------------------------------------------------------------------------------------
// Showing the toolbar and the sidebar

/** Whether the bookmarks toolbar has its own row under the toolbar now (as opposed to appearing
 *  only while the pointer is at the top edge of the single-toolbar layout). */
export function barPinned(): boolean {
  const mode = settings.value.bookmarksBar;
  if (mode === "never") return false;
  if (mode === "always") return true;
  const tab = browser.activeTab;
  return !tab || isNewTab(tab.url);
}

/** Ctrl+Shift+B: hides the toolbar if it is showing, shows it for good otherwise. */
export function toggleBookmarksBar() {
  settings.update({ bookmarksBar: barPinned() ? "never" : "always" });
}

/** Ctrl+B: opens the bookmarks sidebar, or closes it; from the history view it goes back to
 *  bookmarks. */
export function toggleBookmarksPanel() {
  const s = settings.value;
  if (s.bookmarksPanel && s.bookmarksPanelView === "bookmarks") settings.update({ bookmarksPanel: false });
  else settings.update({ bookmarksPanel: true, bookmarksPanelView: "bookmarks" });
}

// ---------------------------------------------------------------------------------------------
// Files

export async function importBookmarksFile() {
  try {
    const n = await bookmarks.importHtml();
    if (n > 0) ui.toast(`Imported ${n} ${n === 1 ? "bookmark" : "bookmarks"}`, { icon: "bookmark" });
    else ui.toast("No new bookmarks in that file", { icon: "bookmark" });
  } catch (e) {
    ui.toast(`Import failed: ${e}`, { icon: "alert" });
  }
}

export async function exportBookmarksFile() {
  try {
    const path = await api.bookmarksExport();
    if (path) ui.toast("Bookmarks exported", { icon: "check", action: { label: "Show", run: () => void api.showInFolder(path) } });
  } catch (e) {
    ui.toast(`Export failed: ${e}`, { icon: "alert" });
  }
}

export async function backupBookmarksFile() {
  try {
    const path = await api.bookmarksBackup();
    if (path) ui.toast("Backup saved", { icon: "check", action: { label: "Show", run: () => void api.showInFolder(path) } });
  } catch (e) {
    ui.toast(`Backup failed: ${e}`, { icon: "alert" });
  }
}

const REPLACE_WARNING =
  "Replace all your bookmarks with the ones in the backup?\n\nA copy of your current bookmarks is kept first.";

/** Restores a backup: from a file the user picks, or one of the automatic ones by name. */
export async function restoreBookmarks(name?: string) {
  if (!window.confirm(REPLACE_WARNING)) return;
  try {
    const n = name ? await bookmarks.restoreBackup(name) : await bookmarks.restoreFile();
    if (n !== null) ui.toast(`Restored ${n} ${n === 1 ? "bookmark" : "bookmarks"}`, { icon: "bookmark" });
  } catch (e) {
    ui.toast(`Restore failed: ${e}`, { icon: "alert" });
  }
}
