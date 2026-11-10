// Download speed and time left for the downloads popup. The backend reports only the bytes
// received so far; the speed is measured here from successive reports.

import type { DownloadItem } from "./types.ts";
import { formatBytes } from "../../shared/format.ts";
import { downloadStatus } from "./media.ts";

/** How much of each new measurement goes into the shown speed (the rest is the old one), so it
 *  doesn't jump with every report. */
const SMOOTHING = 0.3;
/** Reports closer together than this are too short to measure. */
const MIN_INTERVAL_MS = 200;

interface Sample {
  at: number;
  bytes: number;
  rate: number;
}

/** Bytes per second of each running download. */
export class RateMeter {
  #last = new Map<number, Sample>();

  /** Notes that download `id` has `bytes` at time `now` (ms) and returns its speed. */
  sample(id: number, bytes: number, now: number): number {
    const prev = this.#last.get(id);
    if (!prev || bytes < prev.bytes) {
      this.#last.set(id, { at: now, bytes, rate: 0 });
      return 0;
    }
    const dt = now - prev.at;
    if (dt < MIN_INTERVAL_MS) return prev.rate;
    const measured = ((bytes - prev.bytes) * 1000) / dt;
    const rate = prev.rate > 0 ? prev.rate + SMOOTHING * (measured - prev.rate) : measured;
    this.#last.set(id, { at: now, bytes, rate });
    return rate;
  }

  forget(id: number) {
    this.#last.delete(id);
  }
}

/** "8 s left", "3 min left", "1 h 5 min left"; "" when it can't be told. */
export function formatTimeLeft(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "";
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))} s left`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min left`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m ? `${h} h ${m} min left` : `${h} h left`;
}

/** The status line of a download in the popup: like the downloads page's, with the speed and
 *  time left of a running file ("12 MB of 150 MB · 2.1 MB/s · 1 min left"). */
export function panelStatus(d: DownloadItem, rate: number, timeAgo: (ms: number) => string): string {
  const plain = (d.kind ?? "file") === "file";
  if (!plain || d.state !== "progress" || d.paused) {
    if (d.state === "completed" && plain) return `Completed · ${formatBytes(d.total || d.received)} · ${timeAgo(d.ended ?? d.started)}`;
    return downloadStatus(d, timeAgo);
  }
  const parts = [d.total > 0 ? `${formatBytes(d.received)} of ${formatBytes(d.total)}` : formatBytes(d.received)];
  if (rate > 0) {
    parts.push(`${formatBytes(rate)}/s`);
    if (d.total > d.received) parts.push(formatTimeLeft((d.total - d.received) / rate));
  } else if (d.received === 0) {
    parts[0] = "Starting…";
  }
  return parts.filter(Boolean).join(" · ");
}
