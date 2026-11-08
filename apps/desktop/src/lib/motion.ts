// Zen Browser's motion, as easing functions for Svelte transitions. The CSS equivalents are the
// --zen-spring and --zen-compact-ease tokens in app.css.

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
