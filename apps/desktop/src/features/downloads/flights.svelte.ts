// Fly-to-downloads: when a download starts, its file lifts off where it was clicked and arcs
// into the downloads button. This holds the flights in the air; DownloadFlights.svelte draws
// them above the pages, and flight.ts has the geometry.
import { paneRectOf, raiseChrome } from '$platform/layout.svelte';
import { settings } from '$features/settings/settings.svelte';
import type { IconName } from '$shared/ui/icons';
import { boxAt, centre, fileBadge, type Box, type FileBadge } from './flight';
import type { DownloadItem } from './types';

export interface Flight {
  id: number;
  from: Box;
  icon: IconName;
  label: string;
  color: string | null;
  /** Lets the chrome drop below the pages again. */
  release: () => void;
}

/** A burst of downloads (a "download all") flies a few files, not a swarm. */
const MAX_IN_AIR = 4;

const reducedMotion = () =>
  typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

class FlightStore {
  list = $state<Flight[]>([]);
  #id = 0;

  /** Flies a file from `from` (window CSS px) into the downloads button. */
  launch(from: Box, badge: FileBadge) {
    if (document.hidden) return;
    if (reducedMotion()) return void pulseDownloads();
    if (this.list.length >= MAX_IN_AIR) return;
    // The file starts over the page, so the chrome goes above the pages for the flight.
    const release = raiseChrome();
    this.list.push({ id: ++this.#id, from, ...badge, release });
  }

  /** A download a page started: from where the pointer was (the click on its link), else
   *  from the middle of the tab's page. */
  launchFromPage(d: DownloadItem) {
    const o = d.origin;
    const inView = !!o && o.x >= 0 && o.y >= 0 && o.x <= window.innerWidth && o.y <= window.innerHeight;
    const pane = d.tabId ? paneRectOf(d.tabId) : null;
    const at = inView ? o : pane ? centre(pane) : null;
    if (!at) return;
    this.launch(boxAt(at), fileBadge(d.fileName ?? d.path, d.mime));
  }

  land(id: number) {
    const f = this.list.find((x) => x.id === id);
    if (!f) return;
    f.release();
    this.list = this.list.filter((x) => x.id !== id);
  }
}

export const flights = new FlightStore();

function visible(el: HTMLElement): DOMRect | null {
  const r = el.getBoundingClientRect();
  if (r.width < 4 || r.height < 4) return null;
  if (r.right <= 0 || r.bottom <= 0 || r.left >= window.innerWidth || r.top >= window.innerHeight) return null;
  if (el.checkVisibility && !el.checkVisibility({ opacityProperty: true, visibilityProperty: true })) return null;
  return r;
}

/** The downloads button on screen (sidebar footer, floating sidebar or tab strip), if any. */
function downloadsButton(): { el: HTMLElement; rect: DOMRect } | null {
  for (const el of document.querySelectorAll<HTMLElement>('[data-downloads-anchor]')) {
    const rect = visible(el);
    if (rect) return { el, rect };
  }
  return null;
}

/** Where the file lands: the downloads button, or with the sidebar hidden (compact mode) the
 *  window edge it would come in from. */
export function downloadsTarget(): Box {
  const b = downloadsButton();
  if (b) return { x: b.rect.left, y: b.rect.top, w: b.rect.width, h: b.rect.height };
  const W = window.innerWidth;
  const H = window.innerHeight;
  const S = 32;
  switch (settings.value.sidebarPosition) {
    case 'right':
      return { x: W - S, y: H - S - 8, w: S, h: S };
    case 'top':
      return { x: 8, y: 0, w: S, h: S };
    default:
      return { x: 0, y: H - S - 8, w: S, h: S };
  }
}

/** The downloads button takes the file in: it dips, then springs back. */
export function pulseDownloads() {
  const icon = downloadsButton()?.el.querySelector<SVGElement>('svg');
  icon?.animate(
    [
      { transform: 'none' },
      { transform: 'translateY(2px) scale(0.84)', offset: 0.3 },
      { transform: 'scale(1.14)', offset: 0.65 },
      { transform: 'none' },
    ],
    { duration: reducedMotion() ? 0 : 420, easing: 'ease-out' },
  );
}
