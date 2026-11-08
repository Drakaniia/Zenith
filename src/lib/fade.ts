// Appearance changes (theme, window material) swap whole sets of CSS variables at once, which
// reads as a blink. Where the browser supports view transitions, the change is cross-faded
// instead: the old picture of the window fades into the new one.

type ViewTransitionDocument = Document & {
  startViewTransition?: (update: () => void) => unknown;
};

/** Runs `change` (a synchronous DOM update) as a cross-fade; plain when fading isn't possible. */
export function fadeChange(change: () => void) {
  const doc = document as ViewTransitionDocument;
  if (!doc.startViewTransition || matchMedia("(prefers-reduced-motion: reduce)").matches) {
    change();
    return;
  }
  doc.startViewTransition(change);
}
