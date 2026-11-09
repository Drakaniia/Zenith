// Keeps the backend's webview rectangles in sync with the chrome UI's layout.
//
// Each visible web tab renders an empty "pane" element; its client rect (CSS px, which are
// window DIPs) becomes the webview bounds. Floating UI takes an "overlay lease" while mounted:
// with any lease held, the backend raises the transparent chrome above the page webviews so
// menus, the palette and Glance draw over live pages.
import type { LayoutSpec, PaneSpec } from "./webview-api";
import { webviewApi } from "./webview-api";
import { intersectRects, sameRects, type Rect } from "./rects";

export type { Rect };

interface Registry {
  isLive(id: string): boolean;
  /** False while a live tab's webview is still being created (the backend doesn't know it yet). */
  isReady(id: string): boolean;
}

let registry: Registry = { isLive: () => false, isReady: () => true };
export function setLayoutRegistry(r: Registry) {
  registry = r;
}

const panes = new Map<HTMLElement, string>();
let glanceEl: { el: HTMLElement; id: string } | null = null;
let pending = false;
let lastSent = "";
let pendingFocus: string | null | undefined = undefined;
let inflight: Promise<void> = Promise.resolve();
let applying = false;
let dirty = false;

class OverlayState {
  /** Elements still on screen that need the chrome above the pages, including ones playing
   *  their exit transition. Drives the z-order. */
  holds = $state(0);
  forced = $state(false);
  /** Window rectangles of the live page webviews (CSS px), as last sent to the backend. */
  panes = $state<Rect[]>([]);
}
export const overlay = new OverlayState();

// ResizeObserver runs after layout, before paint. Waiting for another animation frame here
// leaves the native page at the old bounds while the sidebar has already moved.
const observer = new ResizeObserver(() => applyLatestLayout());

/** One active IPC/paint pass and one request for the latest DOM geometry, never a queue of
 *  redundant flushes for frames missed while the backend is busy. */
function applyLatestLayout() {
  dirty = true;
  if (applying) return;
  applying = true;
  inflight = (async () => {
    try {
      do {
        dirty = false;
        await flush();
      } while (dirty);
    } finally {
      applying = false;
    }
  })();
}

export function scheduleLayout() {
  if (pending) return;
  pending = true;
  requestAnimationFrame(() => {
    pending = false;
    applyLatestLayout();
  });
}



let followUntil = 0;
/** Each pane's largest size during the running layout animation (see `measureMotion`). */
const motionSize = new Map<HTMLElement, { w: number; h: number }>();
let remeasure = false;

/** Seeks the running CSS transitions to their end for a moment to read where every pane ends
 *  up, and keeps the larger of its current and final size. The page webview keeps that size for
 *  the whole animation while its container clips it (placement.rs), so the page isn't re-laid
 *  out on every frame and never trails the chrome with an unpainted strip. The transitions are
 *  restored in the same task, so nothing is painted or fired in between. */
function measureMotion() {
  motionSize.clear();
  if (typeof document.getAnimations !== "function" || typeof CSSTransition === "undefined") return;
  const anims = document
    .getAnimations()
    .filter((a) => a instanceof CSSTransition && a.playState === "running" && a.currentTime !== null);
  if (!anims.length) return;
  const els = [...panes.keys()].filter((el) => el.isConnected);
  const before = els.map((el) => el.getBoundingClientRect());
  const saved = anims.map((a) => a.currentTime);
  try {
    for (const a of anims) {
      const end = Number(a.effect?.getComputedTiming().endTime ?? 0);
      // Just short of the end, so no animation reaches its finished state.
      a.currentTime = Math.max(0, end - 0.5);
    }
    els.forEach((el, i) => {
      const f = el.getBoundingClientRect();
      motionSize.set(el, { w: Math.max(before[i].width, f.width), h: Math.max(before[i].height, f.height) });
    });
  } finally {
    anims.forEach((a, i) => (a.currentTime = saved[i]));
  }
}

/** Re-sends the page rectangles every frame for `ms`, while the chrome animates its layout.
 *  (The ResizeObserver only reports size changes, not moves.) */
export function followLayout(ms: number) {
  const running = performance.now() < followUntil;
  // A change mid-animation retargets the transitions: measure again on the next frame.
  remeasure = true;
  followUntil = Math.max(followUntil, performance.now() + ms);
  if (running) return;
  const step = () => {
    if (performance.now() >= followUntil) {
      // Back to content that matches its pane.
      motionSize.clear();
      applyLatestLayout();
      return;
    }
    if (remeasure) {
      remeasure = false;
      measureMotion();
    }
    // Already in an animation frame; scheduleLayout would defer the first update one more frame.
    applyLatestLayout();
    requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}

/** Resolves once the layout for the current DOM has reached the backend (Svelte has updated the
 *  DOM by the next frame, when the pass runs). A hidden window gets no frames, so it gives up
 *  waiting after a moment. */
export function layoutApplied(): Promise<void> {
  scheduleLayout();
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, 150);
    requestAnimationFrame(() => void inflight.then(resolve, resolve).finally(() => clearTimeout(timer)));
  });
}

/** Moves keyboard focus to a tab's page (or the chrome with `null`) once the layout is applied. */
export function focusAfterLayout(id: string | null) {
  pendingFocus = id;
  scheduleLayout();
}

/** A pane's window rectangle. Its edges are rounded, not its size, so the right and bottom edges
 *  stay put to the pixel while the left/top edge moves. */
function rectOf(el: HTMLElement, id: string): PaneSpec | null {
  const r = el.getBoundingClientRect();
  if (r.width < 2 || r.height < 2) return null;
  const right = Math.round(r.right);
  const bottom = Math.round(r.bottom);
  const x = Math.min(Math.round(r.left), right - 2);
  const y = Math.min(Math.round(r.top), bottom - 2);
  return { id, x, y, w: right - x, h: bottom - y };
}

async function flush() {
  const live = [...panes].filter(([el, id]) => el.isConnected && registry.isLive(id));
  const list: PaneSpec[] = [];
  // Both native bounds and tint cutouts follow the actual animated pane, in single and split
  // view alike. Holding an endpoint here makes the page snap while the sidebar slides.
  const cutouts: Rect[] = [];
  for (const [el, id] of live) {
    const r = rectOf(el, id);
    if (!r) continue;
    const m = motionSize.get(el);
    if (m && (m.w > r.w || m.h > r.h)) {
      r.cw = Math.ceil(Math.max(m.w, r.w));
      r.ch = Math.ceil(Math.max(m.h, r.h));
    }
    list.push(r);
    cutouts.push({ x: r.x, y: r.y, w: r.w, h: r.h });
  }
  const glance =
    glanceEl && glanceEl.el.isConnected && registry.isLive(glanceEl.id)
      ? rectOf(glanceEl.el, glanceEl.id)
      : null;
  const spec: LayoutSpec = {
    panes: list,
    glance,
    overlay: overlay.holds > 0 || overlay.forced || !!glance,
    viewportW: window.innerWidth,
    viewportH: window.innerHeight,
  };
  const key = JSON.stringify(spec);
  if (key !== lastSent) {
    lastSent = key;
    // The backend moves a webview before the chrome paints its next frame, so a cutout that
    // followed the move would leave the bare window background, not the tint, where a page has
    // just moved away (a blink as the bookmarks panel or the sidebar opens). So the tint first
    // comes back everywhere but where a page is both before and after the move, painted before
    // the webviews move; the new cutouts follow once the webviews are in place and cover them.
    const keep = intersectRects(overlay.panes, cutouts);
    const exposes = !sameRects(keep, overlay.panes);
    if (exposes) overlay.panes = keep;
    // Only raised chrome can cover a live page with its tint. Below the pages, waiting for
    // another paint on every resize needlessly leaves the webview a frame behind the sidebar.
    const root = document.documentElement;
    let paint = exposes && (spec.overlay || root.hasAttribute("data-raised")) && !document.hidden;
    // `data-raised` drops the window's opaque body background (app.css), which would
    // cover the pages once the chrome is above them. It goes off before the raise has painted
    // and comes back only after the chrome is below the pages again.
    if (spec.overlay && !root.hasAttribute("data-raised")) {
      root.setAttribute("data-raised", "");
      paint = true;
    }
    if (paint) await nextPaint();
    try {
      await webviewApi.setLayout(spec);
    } catch (e) {
      console.warn("set_layout failed", e);
    }
    if (!sameRects(cutouts, overlay.panes)) overlay.panes = cutouts;
    if (!spec.overlay) root.removeAttribute("data-raised");
  }
  // A page whose webview is still being created takes focus on the pass after it exists (`wake`
  // schedules one); the backend would drop it now.
  const waiting = pendingFocus != null && registry.isLive(pendingFocus) && !registry.isReady(pendingFocus);
  if (pendingFocus !== undefined && !waiting) {
    const target = pendingFocus;
    pendingFocus = undefined;
    if (target === null || registry.isLive(target)) {
      try {
        await webviewApi.focusView(target);
      } catch {
        /* ignore */
      }
    }
  }
}

/** Resolves once the chrome has painted the current DOM. */
function nextPaint() {
  return new Promise<void>((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
}

/** Where a live tab's page is on screen (window CSS px), or null when it isn't showing. */
export function paneRectOf(id: string): Rect | null {
  const els = [...panes].filter(([, t]) => t === id).map(([el]) => el);
  if (glanceEl?.id === id) els.push(glanceEl.el);
  for (const el of els) {
    if (!el.isConnected) continue;
    const r = el.getBoundingClientRect();
    if (r.width > 1 && r.height > 1) return { x: r.left, y: r.top, w: r.width, h: r.height };
  }
  return null;
}

/** `use:pane={tabId}` on the element a web page should cover. */
export function pane(node: HTMLElement, id: string) {
  panes.set(node, id);
  observer.observe(node);
  scheduleLayout();
  return {
    update(next: string) {
      panes.set(node, next);
      scheduleLayout();
    },
    destroy() {
      panes.delete(node);
      observer.unobserve(node);
      scheduleLayout();
    },
  };
}

/** `use:glancePane={tabId}` on the Glance card body. */
export function glancePane(node: HTMLElement, id: string) {
  glanceEl = { el: node, id };
  observer.observe(node);
  scheduleLayout();
  return {
    update(next: string) {
      glanceEl = { el: node, id: next };
      scheduleLayout();
    },
    destroy() {
      if (glanceEl?.el === node) glanceEl = null;
      observer.unobserve(node);
      scheduleLayout();
    },
  };
}

/** `use:overlayLease` on floating UI: the chrome stays above pages while the element is on
 *  screen, including its exit transition. */
export function overlayLease(node: HTMLElement) {
  return overlayHold(node);
}

/** Keeps the chrome above the pages until the returned function is called (any number of times).
 *  Floating UI takes it before it mounts, so its entrance plays above the page from the first
 *  frame instead of showing only once the z-order switch lands. */
export function raiseChrome(): () => void {
  let held = true;
  overlay.holds++;
  scheduleLayout();
  return () => {
    if (!held) return;
    held = false;
    overlay.holds = Math.max(0, overlay.holds - 1);
    scheduleLayout();
  };
}

/** `use:overlayHold`: keeps the chrome above the pages while the element is mounted. */
export function overlayHold(_node: HTMLElement) {
  overlay.holds++;
  scheduleLayout();
  return {
    destroy() {
      overlay.holds = Math.max(0, overlay.holds - 1);
      scheduleLayout();
    },
  };
}

window.addEventListener("resize", () => scheduleLayout());
