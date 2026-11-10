// Display helpers for torrents (no Svelte state, so they can be unit tested).
import type { DownloadItem, TorrentFile } from "./types.ts";
import { formatBytes } from "../../shared/format.ts";

/** "1.2 MB/s" */
export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** "45 s left", "12 min left", "3 h 5 min left", "2 days left"; "" when unknown. */
export function formatEta(seconds: number | undefined | null): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds <= 0) return "";
  const s = Math.round(seconds);
  if (s < 60) return `${s} s left`;
  if (s < 3600) return `${Math.round(s / 60)} min left`;
  if (s < 86400) {
    const h = Math.floor(s / 3600);
    const m = Math.round((s % 3600) / 60);
    return m ? `${h} h ${m} min left` : `${h} h left`;
  }
  const d = Math.round(s / 86400);
  return `${d} ${d === 1 ? "day" : "days"} left`;
}

/** "0.42" (two decimals at most). */
export function formatRatio(r: number): string {
  return String(Math.round((Number.isFinite(r) ? r : 0) * 100) / 100);
}

/** The status line of a torrent in the downloads list. */
export function torrentStatus(d: DownloadItem, timeAgo: (ms: number) => string): string {
  const t = d.torrent;
  const size = d.total > 0 ? `${formatBytes(d.received)} of ${formatBytes(d.total)}` : formatBytes(d.received);
  if (d.state === "canceled") return "Canceled";
  if (d.state === "interrupted") return d.reason || "Failed";
  if (!t) return size;
  switch (t.stage) {
    case "metadata":
      return d.detail || "Getting torrent info from peers…";
    case "choosing":
      return d.detail || "Choose the files to download";
    case "checking":
      return `Checking files… · ${size}`;
    case "paused":
      return `Paused · ${size}`;
    case "seeding": {
      const limit = t.ratioLimit > 0 ? ` of ${formatRatio(t.ratioLimit)}` : "";
      const peers = t.peers ? ` · ${t.peers} ${t.peers === 1 ? "peer" : "peers"}` : "";
      return `Seeding · ↑ ${formatSpeed(t.upSpeed)} · ratio ${formatRatio(t.ratio)}${limit}${peers}`;
    }
    case "done":
      return `${formatBytes(d.total || d.received)} · ${timeAgo(d.ended ?? d.started)}`;
    default: {
      if (d.state === "completed") return `${formatBytes(d.total || d.received)} · ${timeAgo(d.ended ?? d.started)}`;
      if (!t.peers) return `${size} · Looking for peers…`;
      const parts = [size, `↓ ${formatSpeed(t.downSpeed)}`, `${t.peers} ${t.peers === 1 ? "peer" : "peers"}`];
      const eta = formatEta(t.eta);
      if (eta) parts.push(eta);
      return parts.join(" · ");
    }
  }
}

/** A magnet link in pasted or dropped text (the first one, trimmed). */
export function findMagnet(text: string): string | null {
  const m = /magnet:\?[^\s"'<>]+/i.exec(text);
  return m ? m[0] : null;
}

/** A .torrent file name. */
export function isTorrentName(name: string): boolean {
  return /\.torrent$/i.test(name.trim());
}

const PLAYABLE = /\.(mp4|m4v|webm|mkv|mov|ogv|mp3|m4a|aac|ogg|oga|opus|flac|wav)$/i;

/** Video and audio a tab can play while the torrent downloads. */
export function isPlayable(name: string): boolean {
  return PLAYABLE.test(name);
}

/** "3 of 12 files · 4.2 GB" for the picker's choice. */
export function selectionSummary(files: readonly TorrentFile[], chosen: ReadonlySet<number>): string {
  const picked = files.filter((f) => chosen.has(f.index));
  const size = picked.reduce((a, f) => a + f.size, 0);
  return `${picked.length} of ${files.length} ${files.length === 1 ? "file" : "files"} · ${formatBytes(size)}`;
}
