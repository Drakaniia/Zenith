// Drag and drop of bookmarks between the toolbar, the sidebar tree and the Library, with pointer
// events (as the tabs do in sidebar-dnd.svelte.ts: the page views are native windows, so the
// browser's own drag and drop doesn't reach the chrome reliably). Elements take part through
// data attributes:
//   data-bm-node="<id>"   a bookmark, folder or separator: draggable, and a drop target
//   data-bm-axis="x"      on the node or a parent: items run sideways (the toolbar); default is down
//   data-bm-list="<id>"   empty space that appends to folder <id> when no node is under the pointer
//   data-bm-into="<id>"   a place that only drops *into* folder <id> (the Library's folder tree)
// The moves themselves are planned by planMoves() in bookmarks.ts.
import { bookmarks } from "./bookmarks.svelte";
import { canMoveInto, kids } from "./bookmarks";
import { ui } from "./ui.svelte";
import type { BookmarkNode } from "./types";

export interface DropTarget {
  parent: number;
  /** Position among the folder's children as drawn, before the dragged nodes leave. */
  index: number;
  /** What to draw: a line before or after a node, a folder lit up, or a whole list. */
  mark: { id: number; where: "before" | "after" | "into" } | { list: number };
}

interface DragState {
  ids: number[];
  label: string;
  url: string;
  folder: boolean;
  x: number;
  y: number;
  sx: number;
  sy: number;
  started: boolean;
  target: DropTarget | null;
}

class BookmarkDnd {
  drag = $state<DragState | null>(null);

  /** A drag is under way (the chrome must stay above the page views until it ends). */
  get active() {
    return !!this.drag?.started;
  }

  /** The indicator to draw on a node, if the pointer is deciding where to drop next to it. */
  markFor(id: number): "before" | "after" | "into" | null {
    const t = this.drag?.started ? this.drag.target : null;
    const m = t?.mark;
    return m && "id" in m && m.id === id && !this.drag?.ids.includes(id) ? m.where : null;
  }

  /** True while `id` is one of the nodes being dragged. */
  dragging(id: number): boolean {
    return !!this.drag?.started && this.drag.ids.includes(id);
  }

  hitTest(x: number, y: number, ids: number[]): DropTarget | null {
    const ix = bookmarks.index;
    const allowed = (parent: number) => ids.some((id) => canMoveInto(ix, id, parent));
    for (const el of document.elementsFromPoint(x, y)) {
      if (!(el instanceof HTMLElement)) continue;
      const into = el.closest<HTMLElement>("[data-bm-into]");
      if (into) {
        const id = Number(into.dataset.bmInto);
        return allowed(id) ? { parent: id, index: kids(ix, id).length, mark: { id, where: "into" } } : null;
      }
      const item = el.closest<HTMLElement>("[data-bm-node]");
      const node = item ? ix.byId.get(Number(item.dataset.bmNode)) : undefined;
      if (item && node) return this.#over(node, item, x, y, allowed);
      const list = el.closest<HTMLElement>("[data-bm-list]");
      if (list) {
        const id = Number(list.dataset.bmList);
        return allowed(id) ? { parent: id, index: kids(ix, id).length, mark: { list: id } } : null;
      }
    }
    return null;
  }

  #over(node: BookmarkNode, el: HTMLElement, x: number, y: number, allowed: (p: number) => boolean): DropTarget | null {
    const ix = bookmarks.index;
    const r = el.getBoundingClientRect();
    const horizontal = el.closest<HTMLElement>("[data-bm-axis]")?.dataset.bmAxis === "x";
    const frac = horizontal ? (x - r.left) / Math.max(1, r.width) : (y - r.top) / Math.max(1, r.height);
    const into = (): DropTarget | null =>
      allowed(node.id) ? { parent: node.id, index: kids(ix, node.id).length, mark: { id: node.id, where: "into" } } : null;
    // The built-in folders can't be reordered, only dropped into.
    if (node.parent === null) return into();
    // The middle of a folder drops into it; its edges drop next to it.
    if (node.kind === "folder" && frac > 0.28 && frac < 0.72) return into();
    if (!allowed(node.parent)) return null;
    const at = kids(ix, node.parent).findIndex((s) => s.id === node.id);
    const before = frac < 0.5;
    return { parent: node.parent, index: before ? at : at + 1, mark: { id: node.id, where: before ? "before" : "after" } };
  }

  /** Starts a drag from a pointer press on a node: it begins after the pointer moves a few pixels. */
  start(e: PointerEvent, ids: number[], ghost: { label: string; url?: string; folder?: boolean }) {
    if (e.button !== 0 || !ids.length || (e.target as HTMLElement).closest("input, textarea")) return;
    this.drag = {
      ids,
      label: ghost.label,
      url: ghost.url ?? "",
      folder: !!ghost.folder,
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
        ui.closeMenu();
      }
      if (d.started) d.target = this.hitTest(ev.clientX, ev.clientY, d.ids);
    };
    const key = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") finish(true);
    };
    const end = (ev: PointerEvent) => finish(ev.type === "pointercancel");
    const finish = (cancel: boolean) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("keydown", key, true);
      const d = this.drag;
      this.drag = null;
      if (!d?.started) return;
      suppressNextClick();
      if (cancel || !d.target) return;
      bookmarks.moveTo(d.ids, d.target.parent, d.target.index).catch((err) => ui.toast(`Couldn't move it: ${err}`, { icon: "alert" }));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    window.addEventListener("keydown", key, true);
  }
}

/** Swallows the click that follows the end of a drag (it would open the bookmark). */
function suppressNextClick() {
  const stop = (e: Event) => {
    e.stopPropagation();
    e.preventDefault();
  };
  window.addEventListener("click", stop, { capture: true, once: true });
  setTimeout(() => window.removeEventListener("click", stop, { capture: true }), 50);
}

export const bmDnd = new BookmarkDnd();
