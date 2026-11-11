// A tab row that is added to the sidebar slides in from just above its place.
//
// This replaces Svelte's `in:fly`, which hides the row (opacity 0) under a dummy animation and only
// reveals it from that animation's `finish` event. If that event never arrives (a starved or
// occluded window under heavy system load) the row stays invisible until the app restarts. This
// animation has no fill, so the row is visible by default, and a timer that does not depend on the
// animation clock cancels it if it stalls.
export const ROW_ENTER_MS = 160;
export const ROW_ENTER_OFFSET = 6;
export const ROW_ENTER_GUARD_MS = 600;

/** `play` is read when the row is created: rows that are part of the pane's first render do not
 *  animate, only rows added afterwards. */
export function rowEnter(node: HTMLElement, play: boolean) {
  if (!play) return;
  if (typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const animation = node.animate(
    [
      { opacity: 0, translate: `0px ${-ROW_ENTER_OFFSET}px` },
      { opacity: 1, translate: '0px 0px' },
    ],
    { duration: ROW_ENTER_MS, easing: 'cubic-bezier(0.23, 1, 0.32, 1)' },
  );
  const guard = setTimeout(() => {
    console.warn('sidebar row enter animation stalled; showing the row');
    animation.cancel();
  }, ROW_ENTER_GUARD_MS);
  animation.onfinish = () => clearTimeout(guard);
  return {
    destroy() {
      clearTimeout(guard);
      animation.cancel();
    },
  };
}
