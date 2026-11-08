import { cubicOut } from 'svelte/easing';

/** Whether the system asks for reduced motion (read once by each new-tab page part). */
export function prefersReducedMotion(): boolean {
  return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

// Blocks shown or hidden from the settings collapse (height, spacing) while fading, so the
// rest of the page glides into place instead of jumping.
export function revealTransition(reduced: boolean) {
  return function reveal(node: HTMLElement) {
    const h = node.offsetHeight;
    return {
      duration: reduced ? 120 : 240,
      easing: cubicOut,
      css: (t: number) =>
        reduced
          ? `opacity:${t}`
          : `opacity:${t};height:${t * h}px;margin-block:${t * 9}px;overflow:clip;` +
            `transform:translateY(${(1 - t) * -6}px) scale(${0.97 + 0.03 * t});filter:blur(${(1 - t) * 4}px)`,
    };
  };
}
