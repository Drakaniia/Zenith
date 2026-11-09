// The latest download as a shortcut: for a few minutes after a file finishes, the downloads
// button shows its type and drags it out (onto a page's upload area).

import type { DownloadItem } from "./types.ts";

/** How long a finished file stays on the downloads button. */
export const RECENT_MS = 10 * 60 * 1000;

/** The most recently finished file, while it is recent, and when it stops being. */
export function latestFinished(
  items: readonly DownloadItem[],
  now: number,
  windowMs = RECENT_MS,
): { item: DownloadItem; until: number } | null {
  let best: DownloadItem | null = null;
  for (const d of items) {
    if (d.state !== "completed" || !d.ended || !d.path) continue;
    if (!best || d.ended > (best.ended ?? 0)) best = d;
  }
  if (!best) return null;
  const until = (best.ended ?? 0) + windowMs;
  return until > now ? { item: best, until } : null;
}

/** The file's name as shown: its own, else the last part of its path. */
export function fileNameOf(d: DownloadItem): string {
  return d.fileName || d.path.split(/[\\/]/).pop() || d.url;
}
