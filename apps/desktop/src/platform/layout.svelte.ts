// Keeps the backend's webview rectangles in sync with the chrome UI's layout.
//
// Each visible web tab renders an empty "pane" element; its client rect (CSS px, which are
// window DIPs) becomes the webview bounds. Floating UI takes an "overlay lease" while mounted:
// with any lease held, the backend raises the transparent chrome above the page webviews so
// menus, the palette and Glance draw over live pages.
import type { LayoutSpec, PaneSpec } from "./webview-api";
import { webviewApi } from "./webview-api";

interface Registry {
  isLive(id: string): boolean;
}

let registry: Registry = { isLive: () => false };
export function setLayoutRegistry(r: Registry) {
  registry = r;
}

const panes = new Map<HTMLElement, string>();
let glanceEl: { el: HTMLElement; id: string } | null = null;
let pending = false;
let lastSent = "";
let pendingFocus: string | null | undefined = undefined;
let inflight: Promise<void> = Promise.resolve();

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

class OverlayState {
  /** Floating UI that is showing (not on its way out). Drives the page scrim. */
  leases = $state(0);
  /** Elements still on screen that need the chrome above the pages, including ones playing
   *  their exit transition and the scrim while it fades out. Drives the z-order. */
  holds = $state(0);
  forced = $state(false);
  /** Window rectangles of the live page webviews (CSS px), as last sent to the backend. */
  panes = $state<Rect[]>([]);
}
export const overlay = new OverlayState();

const observer = new ResizeObserver(() => scheduleLayout());

export function scheduleLayout() {
  if (pending) return;
  pending = true;
  requestAnimationFrame(() => {
    pending = false;
    inflight = inflight.then(flush, flush);
  });
}

/** Re-sends the page rectangles every frame for `ms`, while the chrome animates its layout.
 *  (The ResizeObserver only reports size changes, not moves.) */
let followUntil = 0;
export function followLayout(ms: number) {
  const running = performance.now() < followUntil;
  followUntil = Math.max(followUntil, performance.now() + ms);
  if (running) return;
  const step = () => {
    scheduleLayout();
    if (performance.now() < followUntil) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}

/** Moves keyboard focus to a tab's page (or the chrome with `null`) once the layout is applied. */
export function focusAfterLayout(id: string | null) {
  pendingFocus = id;
  scheduleLayout();
}

/** A pane's window rectangle. Its edges are rounded, not its size, so the right and bottom edges
 *  stay put to the pixel while the left/top edge moves. `dx`/`dy` move the left/top edge. */
function rectOf(el: HTMLElement, id: string, dx = 0, dy = 0): PaneSpec | null {
  const r = el.getBoundingClientRect();
  if (r.width < 2 || r.height < 2) return null;
  const right = Math.round(r.right);
  const bottom = Math.round(r.bottom);
  const x = Math.min(Math.round(r.left + dx), right - 2);
  const y = Math.min(Math.round(r.top + dy), bottom - 2);
  return { id, x, y, w: right - x, h: bottom - y };
}

/** How far the page's left/top edges travel during a layout animation (sidebar, toolbars,
 *  compact mode), relative to where they are now: `out` reaches the furthest-out edge of the
 *  animation (<= 0), `in` the furthest-in one (>= 0). All zero when nothing animates.
 *
 *  Resizing a page webview shows its old frame pinned to the new top-left corner for a frame, with
 *  an unpainted strip beside it: a blink, and on every frame if the webview followed the edge. So
 *  during the animation the webview keeps the largest size it reaches, anchored where it is, and
 *  only moves with the edge (a move repaints nothing); the backend hides the part that sticks out
 *  past the page's right/bottom edge. The page lays out once, its content travels with the edge,
 *  and the real resize happens where nothing new is exposed: growing at the start (the new strip
 *  is hidden), shrinking at the end. */
interface Hold {
  outX: number;
  inX: number;
  outY: number;
  inY: number;
  /** The same for the right/bottom insets (a sidebar on the right, a tab strip at the bottom):
   *  <= 0 grows the page past its current right/bottom edge, >= 0 pulls it in. */
  outR: number;
  inR: number;
  outB: number;
  inB: number;
}
const NO_HOLD: Hold = { outX: 0, inX: 0, outY: 0, inY: 0, outR: 0, inR: 0, outB: 0, inB: 0 };
const HOLD_PROPS: Record<string, [keyof Hold, keyof Hold]> = {
  "--lay-left": ["outX", "inX"],
  "--lay-top": ["outY", "inY"],
  "--lay-right": ["outR", "inR"],
  "--lay-bottom": ["outB", "inB"],
};

function layoutHold(): Hold {
  const app = document.querySelector<HTMLElement>(".app");
  if (!app) return NO_HOLD;
  const hold = { ...NO_HOLD };
  for (const a of app.getAnimations()) {
    if (!(a instanceof CSSTransition)) continue;
    const prop = a.transitionProperty;
    const keys = HOLD_PROPS[prop];
    if (!keys) continue;
    const frames = (a.effect as KeyframeEffect | null)?.getKeyframes() ?? [];
    if (frames.length < 2) continue;
    const from = parseFloat(String(frames[0][prop]));
    const to = parseFloat(String(frames[frames.length - 1][prop]));
    const cur = parseFloat(getComputedStyle(app).getPropertyValue(prop));
    if (![from, to, cur].every(Number.isFinite)) continue;
    const out = Math.min(0, from - cur, to - cur);
    const inn = Math.max(0, from - cur, to - cur);
    hold[keys[0]] = out;
    hold[keys[1]] = inn;
  }
  return hold;
}

async function flush() {
  const live = [...panes].filter(([el, id]) => el.isConnected && registry.isLive(id));
  // (Split view keeps following the layout frame by frame: its panes share the width.)
  const hold = live.length === 1 ? layoutHold() : NO_HOLD;
  const list: PaneSpec[] = [];
  // Where the window tint is cut out: the part of each page that shows for the whole animation.
  // It changes only as an animation starts and ends, never trailing the moving edge (the chrome
  // paints a frame after the page webview moves, which would flash the bare window material).
  const cutouts: Rect[] = [];
  for (const [el, id] of live) {
    const shown = rectOf(el, id);
    if (!shown) continue;
    const r: PaneSpec = { ...shown };
    const big = rectOf(el, id, hold.outX, hold.outY);
    const bigW = Math.max(big?.w ?? 0, shown.w) - Math.round(hold.outR);
    const bigH = Math.max(big?.h ?? 0, shown.h) - Math.round(hold.outB);
    if (bigW > shown.w || bigH > shown.h) {
      // The largest size, anchored at the current edge; only `shown` of it is visible.
      r.w = bigW;
      r.h = bigH;
      r.visW = shown.w;
      r.visH = shown.h;
    }
    list.push(r);
    const core = rectOf(el, id, hold.inX, hold.inY) ?? shown;
    cutouts.push({
      x: core.x,
      y: core.y,
      w: Math.max(2, core.w - Math.round(hold.inR)),
      h: Math.max(2, core.h - Math.round(hold.inB)),
    });
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
    if (JSON.stringify(cutouts) !== JSON.stringify(overlay.panes)) overlay.panes = cutouts;
    // `data-raised` drops the solid material's opaque body background (app.css), which would
    // cover the pages once the chrome is above them. It goes off before the raise has painted
    // and comes back only after the chrome is below the pages again.
    const root = document.documentElement;
    if (spec.overlay && !root.hasAttribute("data-raised")) {
      root.setAttribute("data-raised", "");
      if (root.dataset.material === "solid") await nextPaint();
    }
    try {
      await webviewApi.setLayout(spec);
    } catch (e) {
      console.warn("set_layout failed", e);
    }
    if (!spec.overlay) root.removeAttribute("data-raised");
  }
  if (pendingFocus !== undefined) {
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
 *  screen. The lease stops counting as "showing" as soon as the element's exit transition
 *  starts, so the page scrim fades out together with it rather than after it. */
export function overlayLease(node: HTMLElement) {
  let showing = true;
  overlay.leases++;
  overlay.holds++;
  scheduleLayout();
  const outro = () => {
    if (!showing) return;
    showing = false;
    overlay.leases = Math.max(0, overlay.leases - 1);
  };
  // A transition reversed mid-exit (the pointer came back) shows the element again.
  const intro = () => {
    if (showing) return;
    showing = true;
    overlay.leases++;
  };
  node.addEventListener("outrostart", outro);
  node.addEventListener("introstart", intro);
  return {
    destroy() {
      node.removeEventListener("outrostart", outro);
      node.removeEventListener("introstart", intro);
      outro();
      overlay.holds = Math.max(0, overlay.holds - 1);
      scheduleLayout();
    },
  };
}

/** `use:overlayHold`: keeps the chrome above the pages while mounted without counting as
 *  floating UI (the page scrim uses it to finish fading out before the chrome drops). */
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
