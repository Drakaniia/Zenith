// Keeps native page bounds and chrome cutouts in sync with the visible DOM panes.
import type { LayoutSpec, PaneSpec } from "./webview-api";
import { webviewApi } from "./webview-api";
import { intersectRects, sameRects, type Rect } from "./rects";

export type { Rect };

interface Registry {
  isLive(id: string): boolean;
  /** False while the backend is still creating the tab's webview. */
  isReady(id: string): boolean;
}

let registry: Registry = { isLive: () => false, isReady: () => true };
export function setLayoutRegistry(r: Registry) { registry = r; }

const panes = new Map<HTMLElement, string>();
let glanceEl: { el: HTMLElement; id: string } | null = null;
let pending = false;
let lastSent = "";
let pendingFocus: string | null | undefined = undefined;
let inflight: Promise<void> = Promise.resolve();
let applying = false;
let dirty = false;

class OverlayState {
  /** Includes floating elements playing their exit transition. */
  holds = $state(0);
  forced = $state(false);
  /** Page rectangles (CSS px) last applied to the native host. */
  panes = $state<Rect[]>([]);
}
export const overlay = new OverlayState();

// ResizeObserver runs after layout, before paint; don't defer it another frame.
const observer = new ResizeObserver(() => applyLatestLayout());

/** One active pass and one request for fresh geometry, never a backlog of missed frames. */
function applyLatestLayout() {
  dirty = true;
  if (applying) return;
  applying = true;
  inflight = (async () => {
    try {
      do {
        dirty = false;
        const resume = holdLayoutMotion();
        try {
          await flush();
        } finally {
          resume();
        }
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

/** Only transitions owned by shell layout, never unrelated hover or fade motion. */
function layoutTransitions(): CSSTransition[] {
  if (typeof document.getAnimations !== "function" || typeof CSSTransition === "undefined") return [];
  return document.getAnimations().filter((a): a is CSSTransition => {
    if (!(a instanceof CSSTransition) || a.currentTime === null || a.playState === "finished" || a.playState === "idle") return false;
    const target = (a.effect as KeyframeEffect | null)?.target;
    return target instanceof HTMLElement && (
      (a.transitionProperty.startsWith("--lay-") && target.hasAttribute("data-layout-root")) ||
      (a.transitionProperty === "width" && target.hasAttribute("data-layout-sidebar"))
    );
  });
}

/** Native placement is asynchronous. Hold the coupled CSS clocks while it catches up so the
 *  shell cannot advance through several frames against one stale native rectangle. */
function holdLayoutMotion() {
  const held = layoutTransitions().filter((a) => a.playState === "running");
  for (const a of held) a.pause();
  return () => {
    for (const a of held) if (a.playState === "paused") a.play();
  };
}

let followUntil = 0;
let following = false;
const followed: (() => void)[] = [];
const motionSize = new Map<HTMLElement, { w: number; h: number }>();
let remeasure = false;

function retainSize(el: HTMLElement, w: number, h: number) {
  const held = motionSize.get(el);
  motionSize.set(el, { w: Math.max(held?.w ?? 0, w), h: Math.max(held?.h ?? 0, h) });
}

/** Sample only layout-owned endpoints, then restore them in the same task. Retargeting must
 *  not shrink the content allocation: the container clips it until all motion has settled. */
function measureMotion() {
  const anims = layoutTransitions();
  if (!anims.length) return;
  const els = [...panes.keys()].filter((el) => el.isConnected);
  for (const el of els) {
    const r = el.getBoundingClientRect();
    retainSize(el, r.width, r.height);
  }
  const saved = anims.map((a) => a.currentTime);
  try {
    for (const a of anims) {
      const end = Number(a.effect?.getComputedTiming().endTime ?? 0);
      a.currentTime = Math.max(0, end - 0.5);
    }
    for (const el of els) {
      const r = el.getBoundingClientRect();
      retainSize(el, r.width, r.height);
    }
  } finally {
    anims.forEach((a, i) => (a.currentTime = saved[i]));
  }
}

/** Follow position changes as well as resizes. Resolves after gated CSS transitions settle,
 *  which can take longer than their nominal duration when the native host is busy. */
export function followLayout(ms: number): Promise<void> {
  const done = new Promise<void>((resolve) => followed.push(resolve));
  // Called before Svelte changes the DOM, so the original allocation is retained too.
  for (const el of panes.keys()) {
    if (!el.isConnected) continue;
    const r = el.getBoundingClientRect();
    retainSize(el, r.width, r.height);
  }
  remeasure = true;
  followUntil = Math.max(followUntil, performance.now() + ms);
  if (following) return done;
  following = true;
  const step = () => {
    if (performance.now() >= followUntil && !layoutTransitions().length) {
      motionSize.clear();
      following = false;
      applyLatestLayout();
      for (const resolve of followed.splice(0)) resolve();
      return;
    }
    if (remeasure) {
      remeasure = false;
      measureMotion();
    }
    applyLatestLayout();
    requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
  return done;
}

/** A hidden window gets no frames, so don't wait indefinitely. */
export function layoutApplied(): Promise<void> {
  scheduleLayout();
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, 150);
    requestAnimationFrame(() => void inflight.then(resolve, resolve).finally(() => clearTimeout(timer)));
  });
}

export function focusAfterLayout(id: string | null) {
  pendingFocus = id;
  scheduleLayout();
}

/** Round edges rather than sizes so fixed right/bottom edges don't wobble. */
function rectOf(el: HTMLElement, id: string): PaneSpec | null {
  const r = el.getBoundingClientRect();
  if (r.width < 2 || r.height < 2) return null;
  const right = Math.round(r.right);
  const bottom = Math.round(r.bottom);
  const x = Math.min(Math.round(r.left), right - 2);
  const y = Math.min(Math.round(r.top), bottom - 2);
  return { id, x, y, w: right - x, h: bottom - y };
}

function readLayout() {
  const list: PaneSpec[] = [];
  const cutouts: Rect[] = [];
  for (const [el, id] of panes) {
    if (!el.isConnected || !registry.isLive(id)) continue;
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
  const glance = glanceEl && glanceEl.el.isConnected && registry.isLive(glanceEl.id)
    ? rectOf(glanceEl.el, glanceEl.id) : null;
  const spec: LayoutSpec = {
    panes: list,
    glance,
    overlay: overlay.holds > 0 || overlay.forced || !!glance,
    viewportW: window.innerWidth,
    viewportH: window.innerHeight,
  };
  return { spec, cutouts, key: JSON.stringify(spec) };
}

async function flush() {
  let layout = readLayout();
  const root = document.documentElement;
  while (layout.key !== lastSent) {
    // Restore tint before moving away from a cutout. Only raised chrome needs a paint fence;
    // waiting below pages would put every native update another frame behind the sidebar.
    const keep = intersectRects(overlay.panes, layout.cutouts);
    const exposes = !sameRects(keep, overlay.panes);
    if (exposes) overlay.panes = keep;
    let paint = exposes && (layout.spec.overlay || root.hasAttribute("data-raised")) && !document.hidden;
    if (layout.spec.overlay && !root.hasAttribute("data-raised")) {
      root.setAttribute("data-raised", "");
      paint = !document.hidden;
    }
    if (paint) {
      await nextPaint();
      // Layout can change during the fence (resize, reversal, closing a pane or an overlay).
      // Recheck exposure as well as geometry; never submit the pre-fence snapshot.
      layout = readLayout();
      continue;
    }
    try {
      await webviewApi.setLayout(layout.spec);
      lastSent = layout.key;
    } catch (e) {
      console.warn("set_layout failed", e);
      break;
    }
    if (!sameRects(layout.cutouts, overlay.panes)) overlay.panes = layout.cutouts;
    if (!layout.spec.overlay) root.removeAttribute("data-raised");
    break;
  }
  const waiting = pendingFocus != null && registry.isLive(pendingFocus) && !registry.isReady(pendingFocus);
  if (pendingFocus !== undefined && !waiting) {
    const target = pendingFocus;
    pendingFocus = undefined;
    if (target === null || registry.isLive(target)) {
      try { await webviewApi.focusView(target); } catch { /* Ignore stale focus targets. */ }
    }
  }
}

function nextPaint() {
  return new Promise<void>((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
}

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
    update(next: string) { panes.set(node, next); scheduleLayout(); },
    destroy() {
      panes.delete(node);
      motionSize.delete(node);
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
    update(next: string) { glanceEl = { el: node, id: next }; scheduleLayout(); },
    destroy() {
      if (glanceEl?.el === node) glanceEl = null;
      observer.unobserve(node);
      scheduleLayout();
    },
  };
}

export function overlayLease(node: HTMLElement) { return overlayHold(node); }

/** Raise before floating UI mounts, then release after its exit transition. */
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

export function overlayHold(_node: HTMLElement) {
  overlay.holds++;
  scheduleLayout();
  return {
    destroy() { overlay.holds = Math.max(0, overlay.holds - 1); scheduleLayout(); },
  };
}

window.addEventListener("resize", () => scheduleLayout());
