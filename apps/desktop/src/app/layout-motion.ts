// How a layout change animates. Compact mode moves the way Zen's does (ZenCompactMode.mjs,
// animateCompactMode, and compact-mode/toolbar.inc.css):
//  - the sidebar slides out of / back into the window on a zero-bounce spring of 100 ms
//    ({ type: "spring", bounce: 0, duration: 0.1 }), the page edge travelling with it;
//  - a toolbar compact mode hides folds into the top edge while its contents fade, over
//    --zen-hidden-toolbar-transition (0.15s ease-in-out);
//  - leaving compact mode from the floating sidebar docks it where it is, without animating
//    (Zen skips the animation while the sidebar is "potentially open").
// Every other layout change (single/multiple toolbars, collapsing the sidebar, moving it to
// another edge) keeps the 300 ms spring.

export interface LayoutState {
  compact: boolean;
  mode: string;
  barOn: boolean;
  hideToolbar: boolean;
  side: string;
}

export interface LayoutMotion {
  /** Duration of the sidebar's (or tab strip's) move and of the page edges beside it, in ms. */
  side: number;
  /** Duration of the toolbar row's move and of the page's top edge, in ms. */
  bar: number;
  /** The toolbar folds into the top edge (Zen's hidden toolbar) instead of sliding. */
  fold: boolean;
}

export const STILL: LayoutMotion = { side: 0, bar: 0, fold: false };
export const LAYOUT_MS = 300;
export const COMPACT_MS = 100;
export const TOOLBAR_FOLD_MS = 150;

export function sameLayout(a: LayoutState, b: LayoutState): boolean {
  return a.compact === b.compact && a.mode === b.mode && a.barOn === b.barOn && a.hideToolbar === b.hideToolbar && a.side === b.side;
}

/** The motion for a change from `prev` to `next`. `flyoutOpen`: the floating sidebar of compact
 *  mode is showing as the change happens. */
export function layoutMotion(prev: LayoutState, next: LayoutState, flyoutOpen: boolean): LayoutMotion {
  const same = (k: keyof LayoutState) => prev[k] === next[k];
  if (sameLayout(prev, next)) return STILL;
  // Anything beyond compact mode and its hidden toolbar: the general layout motion.
  if (!(same("mode") && same("barOn") && same("side")))
    return { side: LAYOUT_MS, bar: LAYOUT_MS, fold: false };
  const toggled = !same("compact");
  if (toggled && !next.compact && flyoutOpen) return STILL;
  const fold = !same("hideToolbar");
  return {
    side: toggled ? COMPACT_MS : 0,
    bar: fold ? TOOLBAR_FOLD_MS : toggled ? COMPACT_MS : 0,
    fold,
  };
}

/** How long the page webviews follow the layout frame by frame for `m`. */
export function followMs(m: LayoutMotion): number {
  const ms = Math.max(m.side, m.bar);
  return ms ? ms + 80 : 0;
}
