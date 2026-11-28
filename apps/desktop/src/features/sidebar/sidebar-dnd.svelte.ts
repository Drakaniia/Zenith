// Drag and drop of tabs and folders in the sidebar: reorder, nest folders, auto-expand on hover,
// auto-switch spaces on hover, sidebar drag-over-split, and 4-way split drop onto the page.
import { browser } from "$features/browser/browser.svelte";
import { ui } from "$shared/ui/ui.svelte";
import { bookmarks } from "$features/bookmarks/bookmarks.svelte";
import { ROOT_TOOLBAR } from "$features/bookmarks/bookmarks";
import { isWebUrl } from "$shared/url";
import type { TabId } from "$features/browser/types";

export type Zone = "essentials" | "pinned" | "tabs";
export type SplitSide = "left" | "right" | "top" | "bottom";
export type Target =
  | {
      zone: Zone;
      index: number;
      markId?: TabId;
      where?: "before" | "after";
      folderId?: string | null;
    }
  | { zone: "folder"; folderId: string; where?: "inside" | "before" | "after" }
  | { zone: "space"; spaceId: string }
  | { zone: "split"; side: SplitSide }
  | { zone: "split-tab"; tabId: TabId }
  /** The bookmarks toolbar: dropping a tab there bookmarks it at `index` among the shown items. */
  | { zone: "bookmarks"; index: number };

interface DragState {
  kind: "tab" | "folder";
  id: string;
  x: number;
  y: number;
  sx: number;
  sy: number;
  started: boolean;
  target: Target | null;
}

const FOLDER_EXPAND_MS = 450;
const SPACE_SWITCH_MS = 500;
const SPLIT_HOVER_MS = 500;

class SidebarDnd {
  drag = $state<DragState | null>(null);
  #hoverKey = "";
  #hoverTimer: ReturnType<typeof setTimeout> | undefined;
  #splitHoverId: TabId | null = null;
  #splitHoverTimer: ReturnType<typeof setTimeout> | undefined;
  #splitArmedId: TabId | null = null;

  zoneIds(zone: Zone): TabId[] {
    const space = browser.activeSpace;
    if (zone === "essentials") return browser.essentials;
    return zone === "pinned" ? (space?.pinnedIds ?? []) : (space?.tabIds ?? []);
  }

  #clearTimers() {
    clearTimeout(this.#hoverTimer);
    clearTimeout(this.#splitHoverTimer);
    this.#hoverKey = "";
    this.#splitHoverId = null;
    this.#splitArmedId = null;
  }

  #scheduleHover(key: string, delay: number, run: () => void) {
    if (this.#hoverKey === key) return;
    clearTimeout(this.#hoverTimer);
    this.#hoverKey = key;
    this.#hoverTimer = setTimeout(() => {
      if (this.drag?.started && this.#hoverKey === key) run();
    }, delay);
  }

  #cancelHover() {
    clearTimeout(this.#hoverTimer);
    this.#hoverKey = "";
  }

  #trackSplitHover(tabId: TabId | null) {
    if (tabId === this.#splitHoverId) return;
    clearTimeout(this.#splitHoverTimer);
    this.#splitHoverId = tabId;
    this.#splitArmedId = null;
    if (!tabId) return;
    this.#splitHoverTimer = setTimeout(() => {
      if (this.drag?.started && this.drag.kind === "tab" && this.#splitHoverId === tabId) {
        this.#splitArmedId = tabId;
        this.drag.target = { zone: "split-tab", tabId };
      }
    }, SPLIT_HOVER_MS);
  }

  hitTest(x: number, y: number, kind: "tab" | "folder", dragId: string): Target | null {
    for (const el of document.elementsFromPoint(x, y)) {
      if (!(el instanceof HTMLElement)) continue;
      if (kind === "tab") {
        const bar = el.closest<HTMLElement>("[data-bm-bar]");
        if (bar) {
          this.#cancelHover();
          this.#trackSplitHover(null);
          const items = [...bar.querySelectorAll<HTMLElement>("[data-bm-list] > [data-bm-node]")];
          return { zone: "bookmarks", index: items.filter((n) => n.getBoundingClientRect().left + n.offsetWidth / 2 < x).length };
        }
        const split = el.closest<HTMLElement>("[data-drop-split]");
        if (split) {
          this.#cancelHover();
          this.#trackSplitHover(null);
          return { zone: "split", side: split.dataset.dropSplit as SplitSide };
        }
        const sp = el.closest<HTMLElement>("[data-drop-space]");
        if (sp) {
          const spaceId = sp.dataset.dropSpace!;
          this.#trackSplitHover(null);
          if (spaceId !== browser.activeSpaceId) {
            this.#scheduleHover(`space:${spaceId}`, SPACE_SWITCH_MS, () => browser.switchSpace(spaceId));
          }
          return { zone: "space", spaceId };
        }
      }
      const folderHead = el.closest<HTMLElement>("[data-folder-head]");
      if (folderHead) {
        const folderId = folderHead.dataset.folderHead!;
        this.#trackSplitHover(null);
        const f = browser.activeSpace?.folders?.find((x) => x.id === folderId);
        if (f?.collapsed && folderId !== dragId) {
          this.#scheduleHover(`folder:${folderId}`, FOLDER_EXPAND_MS, () => browser.toggleFolder(folderId, false));
        } else {
          this.#cancelHover();
        }
        if (kind === "folder") {
          if (folderId === dragId) return null;
          const r = folderHead.getBoundingClientRect();
          const frac = (y - r.top) / Math.max(1, r.height);
          const where = frac < 0.25 ? "before" : frac > 0.75 ? "after" : "inside";
          return { zone: "folder", folderId, where };
        }
        return { zone: "folder", folderId, where: "inside" };
      }
      if (kind === "folder") {
        const zoneEl = el.closest<HTMLElement>("[data-zone]");
        if (zoneEl?.dataset.zone === "pinned") {
          this.#cancelHover();
          return { zone: "pinned", index: 0, folderId: null };
        }
        continue;
      }
      const zoneEl = el.closest<HTMLElement>("[data-zone]");
      if (!zoneEl) continue;
      this.#cancelHover();
      const zone = zoneEl.dataset.zone as Zone;
      const ids = this.zoneIds(zone);
      const item = el.closest<HTMLElement>("[data-tab-id]");
      const folderId = el.closest<HTMLElement>("[data-folder-id]")?.dataset.folderId ?? null;
      if (item && ids.includes(item.dataset.tabId!)) {
        const tid = item.dataset.tabId!;
        const r = item.getBoundingClientRect();
        const horizontal =
          zone === "essentials" ||
          !!item.closest("[data-split-group]") ||
          !!item.closest("[data-horizontal]");
        if (!horizontal && tid !== dragId) {
          const frac = (y - r.top) / Math.max(1, r.height);
          if (frac >= 0.28 && frac <= 0.72) {
            this.#trackSplitHover(tid);
            if (this.#splitArmedId === tid) return { zone: "split-tab", tabId: tid };
          } else {
            this.#trackSplitHover(null);
          }
        } else {
          this.#trackSplitHover(null);
        }
        const before = horizontal ? x < r.left + r.width / 2 : y < r.top + r.height / 2;
        const i = ids.indexOf(tid);
        return {
          zone,
          index: before ? i : i + 1,
          markId: tid,
          where: before ? "before" : "after",
          folderId,
        };
      }
      this.#trackSplitHover(null);
      return {
        zone,
        index: ids.length,
        markId: ids[ids.length - 1],
        where: "after",
        folderId: null,
      };
    }
    this.#cancelHover();
    this.#trackSplitHover(null);
    return null;
  }

  start(e: PointerEvent, id: TabId) {
    this.#begin(e, "tab", id);
  }

  startFolder(e: PointerEvent, folderId: string) {
    this.#begin(e, "folder", folderId);
  }

  #begin(e: PointerEvent, kind: "tab" | "folder", id: string) {
    if (
      e.button !== 0 ||
      ui.renaming ||
      (e.target as HTMLElement).closest("button, input")
    )
      return;
    this.#clearTimers();
    this.drag = { kind, id, x: e.clientX, y: e.clientY, sx: e.clientX, sy: e.clientY, started: false, target: null };
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
      if (d.started) d.target = this.hitTest(ev.clientX, ev.clientY, d.kind, d.id);
    };
    const end = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      const d = this.drag;
      this.drag = null;
      this.#clearTimers();
      ui.dragging = false;
      if (!d?.started) return;
      suppressNextClick();
      if (ev.type === "pointercancel" || !d.target) return;
      if (d.kind === "folder") this.#dropFolder(d.id, d.target);
      else this.drop(d.id, d.target);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
  }

  #dropFolder(folderId: string, t: Target) {
    if (t.zone === "folder") {
      if (t.where === "before" || t.where === "after") {
        const anchor = browser.activeSpace?.folders?.find((f) => f.id === t.folderId);
        browser.moveFolder(folderId, anchor?.parentId, t.folderId, t.where);
      } else {
        browser.moveFolder(folderId, t.folderId);
      }
      return;
    }
    if (t.zone === "pinned") {
      browser.moveFolder(folderId, undefined);
    }
  }

  drop(id: TabId, t: Target) {
    if (t.zone === "bookmarks") return bookmarkTab(id, t.index);
    if (t.zone === "space") return browser.moveToSpace(id, t.spaceId);
    if (t.zone === "folder") return browser.moveToFolder(id, t.folderId);
    if (t.zone === "split") return browser.addToSplit(id, t.side);
    if (t.zone === "split-tab") return browser.splitWith(t.tabId, id);
    const ids = this.zoneIds(t.zone);
    let index = t.index;
    const from = ids.indexOf(id);
    if (from >= 0 && from < index) index -= 1;
    browser.moveTab(id, t.zone, index);
    if (t.zone === "pinned") {
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
  mark(id: TabId): "before" | "after" | "split" | null {
    const d = this.drag;
    const t = d?.started && d.kind === "tab" ? d.target : null;
    if (!t || d?.id === id) return null;
    if (t.zone === "split-tab" && t.tabId === id) return "split";
    return "markId" in t && t.markId === id ? (t.where ?? null) : null;
  }

  /** Where the drop indicator goes for a folder header. */
  folderMark(folderId: string): "inside" | "before" | "after" | null {
    const d = this.drag;
    const t = d?.started ? d.target : null;
    if (!t || t.zone !== "folder" || t.folderId !== folderId) return null;
    if (d?.kind === "folder" && d.id === folderId) return null;
    return t.where ?? "inside";
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
