/** Scores how well `query` matches `text` (0 = no match). Substrings beat subsequences;
 *  matches at word starts and near the beginning rank higher. */
export function score(query: string, text: string): number {
  const q = query.trim().toLowerCase();
  if (!q) return 1;
  const t = text.toLowerCase();
  const at = t.indexOf(q);
  if (at >= 0) {
    const boundary = at === 0 || /[\s/._\-:]/.test(t[at - 1]);
    return (
      1000 -
      Math.min(at, 200) * 2 +
      (boundary ? 300 : 0) -
      Math.min(t.length, 300) * 0.2
    );
  }
  let from = 0;
  let total = 0;
  let last = -2;
  for (const ch of q) {
    if (ch === " ") continue;
    const j = t.indexOf(ch, from);
    if (j < 0) return 0;
    total += j === last + 1 ? 12 : /[\s/._\-:]/.test(t[j - 1] ?? " ") ? 8 : 2;
    last = j;
    from = j + 1;
  }
  return total;
}

/** Best score across several fields. */
export function scoreAny(
  query: string,
  ...fields: (string | undefined)[]
): number {
  let best = 0;
  for (const f of fields) if (f) best = Math.max(best, score(query, f));
  return best;
}
