// Drag and drop of tabs in the sidebar: reorder within essentials, pinned tabs, folders and
// regular tabs; drop on a space button to move a tab there; drop on the page to split.
import { browser } from "$features/browser/browser.svelte";
import { ui } from "$shared/ui/ui.svelte";
import { bookmarks } from "$features/bookmarks/bookmarks.svelte";
import { ROOT_TOOLBAR } from "$features/bookmarks/bookmarks";
import { isWebUrl } from "$shared/url";
import type { TabId } from "$shared/types";

export type Zone = "essentials" | "pinned" | "tabs";
export type Target =
  | {
      zone: Zone;
      index: number;
      markId?: TabId;
      where?: "before" | "after";
      folderId?: string | null;
    }
  | { zone: "folder"; folderId: string }
  | { zone: "space"; spaceId: string }
  | { zone: "split"; side: "left" | "right" }
  /** The bookmarks toolbar: dropping a tab there bookmarks it at `index` among the shown items. */
  | { zone: "bookmarks"; index: number };

interface DragState {
  id: TabId;
  x: number;
  y: number;
  sx: number;
  sy: number;
  started: boolean;
  target: Target | null;
}

class SidebarDnd {
  drag = $state<DragState | null>(null);

  zoneIds(zone: Zone): TabId[] {
    const space = browser.activeSpace;
    if (zone === "essentials") return browser.essentials;
    return zone === "pinned" ? (space?.pinnedIds ?? []) : (space?.tabIds ?? []);
  }

  hitTest(x: number, y: number): Target | null {
    for (const el of document.elementsFromPoint(x, y)) {
      if (!(el instanceof HTMLElement)) continue;
      const bar = el.closest<HTMLElement>("[data-bm-bar]");
      if (bar) {
        const items = [...bar.querySelectorAll<HTMLElement>("[data-bm-list] > [data-bm-node]")];
        return { zone: "bookmarks", index: items.filter((n) => n.getBoundingClientRect().left + n.offsetWidth / 2 < x).length };
      }
      const split = el.closest<HTMLElement>("[data-drop-split]");
      if (split)
        return {
          zone: "split",
          side: split.dataset.dropSplit as "left" | "right",
        };
      const sp = el.closest<HTMLElement>("[data-drop-space]");
      if (sp) return { zone: "space", spaceId: sp.dataset.dropSpace! };
      const folderHead = el.closest<HTMLElement>("[data-folder-head]");
      if (folderHead)
        return { zone: "folder", folderId: folderHead.dataset.folderHead! };
      const zoneEl = el.closest<HTMLElement>("[data-zone]");
      if (!zoneEl) continue;
      const zone = zoneEl.dataset.zone as Zone;
      const ids = this.zoneIds(zone);
      const item = el.closest<HTMLElement>("[data-tab-id]");
      const folderId =
        el.closest<HTMLElement>("[data-folder-id]")?.dataset.folderId ?? null;
      if (item && ids.includes(item.dataset.tabId!)) {
        const r = item.getBoundingClientRect();
        // Side by side: Essentials, a split group, and everything in the horizontal tab strip.
        const horizontal =
          zone === "essentials" ||
          !!item.closest("[data-split-group]") ||
          !!item.closest("[data-horizontal]");
        const before = horizontal
          ? x < r.left + r.width / 2
          : y < r.top + r.height / 2;
        const i = ids.indexOf(item.dataset.tabId!);
        return {
          zone,
          index: before ? i : i + 1,
          markId: item.dataset.tabId,
          where: before ? "before" : "after",
          folderId,
        };
      }
      return {
        zone,
        index: ids.length,
        markId: ids[ids.length - 1],
        where: "after",
        folderId: null,
      };
    }
    return null;
  }

  start(e: PointerEvent, id: TabId) {
    if (
      e.button !== 0 ||
      ui.renaming ||
      (e.target as HTMLElement).closest("button, input")
    )
      return;
    this.drag = {
      id,
      x: e.clientX,
      y: e.clientY,
      sx: e.clientX,
      sy: e.clientY,
      started: false,
      target: null,
    };
    const move = (ev: PointerEvent) => {
      const d = this.drag;
      if (!d) return;
      d.x = ev.clientX;
      d.y = ev.clientY;
      if (!d.started && Math.hypot(ev.clientX - d.sx, ev.clientY - d.sy) > 5) {
        d.started = true;
        ui.dragging = true;
        ui.closeMenu();
      }
      if (d.started) d.target = this.hitTest(ev.clientX, ev.clientY);
    };
    const end = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      const d = this.drag;
      this.drag = null;
      ui.dragging = false;
      if (!d?.started) return;
      suppressNextClick();
      if (ev.type === "pointercancel" || !d.target) return;
      this.drop(d.id, d.target);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
  }

  drop(id: TabId, t: Target) {
    if (t.zone === "bookmarks") return bookmarkTab(id, t.index);
    if (t.zone === "space") return browser.moveToSpace(id, t.spaceId);
    if (t.zone === "folder") return browser.moveToFolder(id, t.folderId);
    if (t.zone === "split") {
      const current = browser.activeTab;
      if (!current || current.id === id) return;
      return t.side === "left"
        ? browser.splitWith(id, current.id)
        : browser.splitWith(current.id, id);
    }
    const ids = this.zoneIds(t.zone);
    let index = t.index;
    const from = ids.indexOf(id);
    if (from >= 0 && from < index) index -= 1;
    browser.moveTab(id, t.zone, index);
    if (t.zone === "pinned") {
      // Dropping next to a tab inside a folder puts it in that folder; elsewhere takes it out.
      const before =
        t.where === "before" ? t.markId : ids[ids.indexOf(t.markId ?? "") + 1];
      browser.moveToFolder(
        id,
        t.folderId ?? null,
        t.folderId ? before : undefined,
      );
    }
  }

  /** Where the drop indicator goes for a tab, if anywhere. */
  mark(id: TabId): "before" | "after" | null {
    const d = this.drag;
    const t = d?.started ? d.target : null;
    return t && "markId" in t && t.markId === id && d?.id !== id
      ? (t.where ?? null)
      : null;
  }

  get active() {
    return !!this.drag?.started;
  }
}

/** A tab dropped on the bookmarks toolbar becomes a bookmark there (at the drop position). */
function bookmarkTab(id: TabId, shown: number) {
  const tab = browser.tabs[id];
  if (!tab || !isWebUrl(tab.url)) {
    ui.toast("Only web pages can be bookmarked", { icon: "bookmark" });
    return;
  }
  // The drop position counts the items shown in this workspace; the list holds all of them.
  const visible = bookmarks.visibleKids(ROOT_TOOLBAR);
  const at = visible[shown]?.position;
  bookmarks
    .addBookmark(tab.url, tab.customTitle || tab.title, { parent: ROOT_TOOLBAR, index: at })
    .then(() => ui.toast("Bookmarked", { icon: "bookmark" }))
    .catch((e) => ui.toast(`Couldn't bookmark: ${e}`, { icon: "alert" }));
}

function suppressNextClick() {
  const stop = (e: Event) => {
    e.stopPropagation();
    e.preventDefault();
  };
  window.addEventListener("click", stop, { capture: true, once: true });
  setTimeout(
    () => window.removeEventListener("click", stop, { capture: true }),
    50,
  );
}

export const dnd = new SidebarDnd();
