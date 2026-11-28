// Pure split-view layout & ratio math. Tested directly under Node.
import type { Space, SplitLayout, SplitState, TabId } from "./types.ts";

export const MIN_PANE_SHARE = 0.12;

/** All valid split groups of a space (migrating legacy single `space.split` seamlessly). */
export function spaceSplits(space: Space | undefined): SplitState[] {
  if (!space) return [];
  if (Array.isArray(space.splits)) return space.splits;
  return space.split ? [space.split] : [];
}

/** Ensures `space.splits` is populated and drops legacy `space.split`. */
export function ensureSplits(space: Space): SplitState[] {
  if (!Array.isArray(space.splits)) {
    space.splits = space.split ? [space.split] : [];
  }
  space.split = undefined;
  return space.splits;
}

export function defaultRatios(count: number, layout: SplitLayout = "vsep"): number[] {
  if (layout === "grid") return [0.5, 0.5];
  const n = Math.max(2, count);
  return Array.from({ length: n }, () => 1 / n);
}

/** Normalized ratios for a split: `n` weights summing to 1 for linear layouts, or `[col, row]` for grid. */
export function effectiveRatios(split: SplitState): number[] {
  const layout = split.layout ?? "vsep";
  if (layout === "grid") {
    if (split.ratios?.length === 2) {
      return [
        Math.min(1 - MIN_PANE_SHARE, Math.max(MIN_PANE_SHARE, split.ratios[0])),
        Math.min(1 - MIN_PANE_SHARE, Math.max(MIN_PANE_SHARE, split.ratios[1])),
      ];
    }
    return [0.5, 0.5];
  }
  const n = split.ids.length;
  if (split.ratios?.length === n) {
    const sum = split.ratios.reduce((a, b) => a + Math.max(0, b), 0);
    if (sum > 0) return split.ratios.map((r) => Math.max(0, r) / sum);
  }
  if (n === 2) {
    const r = Math.min(1 - MIN_PANE_SHARE, Math.max(MIN_PANE_SHARE, split.ratio || 0.5));
    return [r, 1 - r];
  }
  return defaultRatios(n, layout);
}

/** Moves the divider between pane `index` and `index + 1` to normalized coordinate `pos` (`0..1`). */
export function resizeAdjacent(
  ratios: readonly number[],
  index: number,
  pos: number,
  min = MIN_PANE_SHARE,
): number[] {
  if (index < 0 || index >= ratios.length - 1) return [...ratios];
  const out = [...ratios];
  let prefix = 0;
  for (let i = 0; i < index; i++) prefix += out[i];
  const pair = out[index] + out[index + 1];
  const floor = Math.min(min, pair / 2);
  const first = Math.min(pair - floor, Math.max(floor, pos - prefix));
  out[index] = first;
  out[index + 1] = pair - first;
  return out;
}

/** Adjusts either the column (`axis = 0`) or row (`axis = 1`) divider of a grid split. */
export function resizeGridAxis(
  ratios: readonly number[],
  axis: 0 | 1,
  pos: number,
  min = MIN_PANE_SHARE,
): [number, number] {
  const next: [number, number] = [ratios[0] ?? 0.5, ratios[1] ?? 0.5];
  next[axis] = Math.min(1 - min, Math.max(min, pos));
  return next;
}

/** Removes `id` from every split in `space`, dropping any group left with fewer than two tabs. */
export function detachFromSplits(space: Space, id: TabId) {
  const splits = ensureSplits(space);
  for (let i = splits.length - 1; i >= 0; i--) {
    const g = splits[i];
    if (!g.ids.includes(id)) continue;
    g.ids = g.ids.filter((x) => x !== id);
    if (g.ids.length < 2) {
      splits.splice(i, 1);
    } else {
      g.ratios = defaultRatios(g.ids.length, g.layout ?? "vsep");
      g.ratio = g.ratios[0] ?? 0.5;
    }
  }
}
