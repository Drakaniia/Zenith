// Zen Browser's motion, as easing functions for Svelte transitions. The spring's CSS equivalent
// is the --zen-spring token in app.css.

const W = 6.5;
const SPRING_END = 1 - (1 + W) * Math.exp(-W);

/** Zero-bounce spring that settles in the transition's duration (Zen animates spaces and the
 *  space background with { type: 'spring', bounce: 0, duration: 0.25 }). */
export function springEase(t: number): number {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  return (1 - (1 + W * t) * Math.exp(-W * t)) / SPRING_END;
}

// Zen's compact-mode reveal curve (--zen-compact-mode-func), sampled every 4 %.
const COMPACT = [
  0, 0.0388, 0.1313, 0.2499, 0.3757, 0.497, 0.6063, 0.7018, 0.7815, 0.8448,
  0.8948, 0.9327, 0.9606, 0.9804, 0.9938, 1.0025, 1.0075, 1.0101, 1.0109,
  1.0106, 1.0097, 1.0084, 1.0071, 1.006, 1.0048, 1.0034,
];

/** The compact-mode sidebar reveal: fast start with a slight overshoot. */
export function compactEase(t: number): number {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  const x = t * (COMPACT.length - 1);
  const i = Math.floor(x);
  return COMPACT[i] + (COMPACT[i + 1] - COMPACT[i]) * (x - i);
}

/** Svelte transition: slide in/out along X by a fraction of the element's own width. */
export function slideX(
  _node: Element,
  {
    from = -1,
    duration = 250,
    easing = springEase,
  }: { from?: number; duration?: number; easing?: (t: number) => number },
) {
  return {
    duration,
    easing,
    css: (_t: number, u: number) => `transform: translateX(${from * u * 100}%)`,
  };
}

/** Svelte transition: slides in from behind its own left (`from` -1) or right (1) edge, clipped
 *  to its box so it never draws over its neighbours. The box itself stays put, so whatever sits
 *  beside it (a page webview) is resized once, not on every frame. */
export function revealX(
  _node: Element,
  {
    from = -1,
    duration = 250,
    easing = springEase,
  }: { from?: number; duration?: number; easing?: (t: number) => number },
) {
  return {
    duration,
    easing,
    css: (_t: number, u: number) => {
      const d = u * 100;
      const clip = from < 0 ? `inset(0 0 0 ${d}%)` : `inset(0 ${d}% 0 0)`;
      return `transform: translateX(${Math.sign(from) * d}%); clip-path: ${clip}`;
    },
  };
}

/** CSS's ease-in-out, close enough: cubic in-out. */
function easeInOut(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

/** Svelte transition: Zen's hidden toolbar (compact-mode/toolbar.inc.css). The row folds into
 *  the top edge, clipped from the bottom as its height goes, while its contents fade; both over
 *  --zen-hidden-toolbar-transition (0.15s ease-in-out). */
export function foldY(
  _node: Element,
  { duration = 150, easing = easeInOut }: { duration?: number; easing?: (t: number) => number },
) {
  return {
    duration,
    easing,
    css: (t: number, u: number) => `clip-path: inset(0 0 ${u * 100}% 0); opacity: ${t}`,
  };
}

/** Svelte transition: slide along X by pixels with a fade, for floating chrome. */
export function slidePx(
  _node: Element,
  {
    x = 0,
    y = 0,
    duration = 250,
    easing = springEase,
  }: {
    x?: number;
    y?: number;
    duration?: number;
    easing?: (t: number) => number;
  },
) {
  return {
    duration,
    easing,
    css: (t: number, u: number) =>
      `transform: translate(${x * u}px, ${y * u}px); opacity: ${Math.min(1, t * 1.6)}`,
  };
}
