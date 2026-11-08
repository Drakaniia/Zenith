// Window rectangles (CSS px) of the page webviews, taken as sets: the window tint is cut out
// under them (layout.svelte.ts).

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The area both sets cover: the overlap of every pair of rectangles that overlap. */
export function intersectRects(a: readonly Rect[], b: readonly Rect[]): Rect[] {
  const out: Rect[] = [];
  for (const p of a) {
    for (const q of b) {
      const x = Math.max(p.x, q.x);
      const y = Math.max(p.y, q.y);
      const w = Math.min(p.x + p.w, q.x + q.w) - x;
      const h = Math.min(p.y + p.h, q.y + q.h) - y;
      if (w > 0 && h > 0) out.push({ x, y, w, h });
    }
  }
  return out;
}

/** The same rectangles in the same order. */
export function sameRects(a: readonly Rect[], b: readonly Rect[]): boolean {
  return (
    a.length === b.length &&
    a.every((r, i) => r.x === b[i].x && r.y === b[i].y && r.w === b[i].w && r.h === b[i].h)
  );
}
