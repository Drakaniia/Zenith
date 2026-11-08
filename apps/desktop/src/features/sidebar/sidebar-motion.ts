// Alt+B commits row geometry once; an unclipped icon layer connects the old and new positions.
// Only translation animates, so favicons never stretch or rewrap with the sidebar's width.
export const SIDEBAR_LAYOUT_MS = 300;
const EASING = 'linear(0, 0.139 10%, 0.373 20%, 0.58 30%, 0.733 40%, 0.835 50%, 0.901 60%, 0.941 70%, 0.966 80%, 0.98 90%, 1)';
type Icon = HTMLElement | SVGElement;
type Point = { x: number; y: number };
type Flight = { ghost: Icon; animation: Animation; restore(): void };

export function sidebarMotion(root: HTMLElement) {
  const running = new Map<string, Flight>();
  const icons = () => root.querySelectorAll<Icon>('[data-sidebar-icon]');
  function visible(icon: Icon, r: DOMRect): boolean {
    const clip = icon.closest('.scroller, .spaces')?.getBoundingClientRect();
    return !clip || (r.bottom > clip.top && r.top < clip.bottom && r.right > clip.left && r.left < clip.right);
  }
  const layer = document.createElement('div');
  layer.setAttribute('aria-hidden', 'true');
  layer.style.cssText = 'position:absolute;inset:0;pointer-events:none;z-index:5;overflow:visible';
  root.append(layer);
  function cancel() {
    for (const flight of running.values()) {
      flight.animation.cancel();
      flight.restore();
    }
    running.clear();
  }
  return {
    capture(): Map<string, Point> {
      const origin = root.getBoundingClientRect();
      const before = new Map<string, Point>();
      for (const icon of icons()) {
        const id = icon.dataset.sidebarIcon!;
        // Read the in-flight copy before cancelling so rapid reversals never jump to an endpoint.
        const flight = running.get(id);
        const r = (flight?.ghost ?? icon).getBoundingClientRect();
        if (flight || visible(icon, r)) before.set(id, { x: r.left - origin.left, y: r.top - origin.top });
      }
      cancel();
      return before;
    },
    play(before: Map<string, Point>) {
      if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
      const origin = root.getBoundingClientRect();
      const moves = Array.from(icons(), (icon) => {
        const r = icon.getBoundingClientRect();
        const previous = visible(icon, r) ? before.get(icon.dataset.sidebarIcon!) : undefined;
        return { icon, r, style: getComputedStyle(icon), x: previous ? previous.x - (r.left - origin.left) : 0, y: previous ? previous.y - (r.top - origin.top) : 0 };
      });
      for (const { icon, r, style, x, y } of moves) {
        if (Math.abs(x) < 0.1 && Math.abs(y) < 0.1) continue;
        const id = icon.dataset.sidebarIcon!;
        const ghost = icon.cloneNode(true) as Icon;
        ghost.removeAttribute('data-sidebar-icon');
        ghost.style.cssText = `position:absolute;left:${r.left - origin.left}px;top:${r.top - origin.top}px;width:${r.width}px;height:${r.height}px;margin:0;display:grid;place-items:center;color:${style.color};font:${style.font};opacity:${style.opacity};pointer-events:none`;
        const opacity = icon.style.opacity;
        icon.style.opacity = '0';
        layer.append(ghost);
        const animation = ghost.animate(
          [{ translate: `${x}px ${y}px` }, { translate: '0px 0px' }],
          { duration: SIDEBAR_LAYOUT_MS, easing: EASING },
        );
        const restore = () => { icon.style.opacity = opacity; ghost.remove(); };
        running.set(id, { ghost, animation, restore });
        animation.onfinish = () => { restore(); running.delete(id); };
      }
    },
    destroy() { cancel(); layer.remove(); },
  };
}
