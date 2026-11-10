// Multi-select rules shared by the History page and the sidebar history view: Ctrl/Cmd-click
// toggles a row, Shift-click selects the range from the last clicked row (the anchor).

export interface Selection {
  selected: Set<number>;
  /** The row a Shift-click range starts from. */
  anchor: number | null;
}

export interface Modifiers {
  ctrl: boolean;
  shift: boolean;
}

export const emptySelection = (): Selection => ({ selected: new Set(), anchor: null });

/** `ids` is every visible row id in display order. */
export function applyClick(ids: number[], cur: Selection, id: number, mod: Modifiers): Selection {
  const at = ids.indexOf(id);
  const from = cur.anchor === null ? -1 : ids.indexOf(cur.anchor);
  if (mod.shift && at >= 0 && from >= 0) {
    const [lo, hi] = from < at ? [from, at] : [at, from];
    const next = mod.ctrl ? new Set(cur.selected) : new Set<number>();
    for (const x of ids.slice(lo, hi + 1)) next.add(x);
    // The anchor stays put so the range can be extended or shrunk with further Shift-clicks.
    return { selected: next, anchor: cur.anchor };
  }
  const next = new Set(cur.selected);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return { selected: next, anchor: id };
}

export const selectAll = (ids: number[]): Selection => ({
  selected: new Set(ids),
  anchor: ids[0] ?? null,
});

/** Drops ids that are no longer listed (after a search or a delete). */
export function prune(cur: Selection, ids: number[]): Selection {
  const live = new Set(ids);
  const selected = new Set([...cur.selected].filter((x) => live.has(x)));
  return { selected, anchor: cur.anchor !== null && live.has(cur.anchor) ? cur.anchor : null };
}
