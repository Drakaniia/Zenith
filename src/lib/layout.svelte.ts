// Keeps the backend's webview rectangles in sync with the chrome UI's layout.
//
// Each visible web tab renders an empty "pane" element; its client rect (CSS px, which are
// window DIPs) becomes the webview bounds. Floating UI takes an "overlay lease" while mounted:
// with any lease held, the backend raises the transparent chrome above the page webviews so
// menus, the palette and Glance draw over live pages.
import { api, type LayoutSpec, type PaneSpec } from "./ipc";

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

function rectOf(el: HTMLElement, id: string): PaneSpec | null {
  const r = el.getBoundingClientRect();
  if (r.width < 2 || r.height < 2) return null;
  return {
    id,
    x: Math.round(r.left),
    y: Math.round(r.top),
    w: Math.round(r.width),
    h: Math.round(r.height),
  };
}

async function flush() {
  const list: PaneSpec[] = [];
  for (const [el, id] of panes) {
    if (!el.isConnected || !registry.isLive(id)) continue;
    const r = rectOf(el, id);
    if (r) list.push(r);
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
    const panesKey = JSON.stringify(list);
    if (panesKey !== JSON.stringify(overlay.panes))
      overlay.panes = list.map(({ x, y, w, h }) => ({ x, y, w, h }));
    try {
      await api.setLayout(spec);
    } catch (e) {
      console.warn("set_layout failed", e);
    }
  }
  if (pendingFocus !== undefined) {
    const target = pendingFocus;
    pendingFocus = undefined;
    if (target === null || registry.isLive(target)) {
      try {
        await api.focusView(target);
      } catch {
        /* ignore */
      }
    }
  }
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
