// Display helpers for the media downloader (kept free of Svelte state so they can be unit tested).
import type { DownloadItem, MediaItem } from "./types";
import { formatBytes as formatSize } from "../../shared/format.ts";
import { torrentStatus } from "./torrent.ts";

/** 3:07, 1:02:05 */
export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}


/** "HLS stream", "Live stream", "MP4 video", "MP3 audio". */
export function kindLabel(
  item: Pick<MediaItem, "kind" | "ext" | "audioOnly" | "live">,
): string {
  if (item.live) return "Live stream";
  if (item.kind === "hls")
    return item.audioOnly ? "Audio stream (HLS)" : "Video stream (HLS)";
  if (item.kind === "dash")
    return item.audioOnly ? "Audio stream (DASH)" : "Video stream (DASH)";
  const ext = (item.ext || (item.audioOnly ? "audio" : "video")).toUpperCase();
  return `${ext} ${item.audioOnly ? "audio" : "video"}`;
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

/** One line of facts: "Video stream (HLS) · 1080p · 12:34 · cdn.example.com". */
export function mediaMeta(item: MediaItem): string {
  const parts = [kindLabel(item)];
  if (item.height && !item.audioOnly) parts.push(`${item.height}p`);
  if (item.duration) parts.push(formatDuration(item.duration));
  if (item.size) parts.push(formatSize(item.size));
  const host = hostOf(item.url);
  if (host) parts.push(host);
  return parts.filter(Boolean).join(" · ");
}

/** The heading of an item: its page title, else the file name from its address. */
export function mediaTitle(item: MediaItem): string {
  if (item.title.trim()) return item.title.trim();
  try {
    const last = decodeURIComponent(
      new URL(item.url).pathname.split("/").filter(Boolean).pop() ?? "",
    );
    if (last) return last;
  } catch {
    /* fall through */
  }
  return kindLabel(item);
}

/** A file download that failed (and can't pick up where it stopped) or was canceled, from a web
 *  address: it can be fetched once more from scratch ("Download again"). */
export function canDownloadAgain(d: DownloadItem): boolean {
  if ((d.kind ?? "file") !== "file" || !/^https?:\/\//i.test(d.url)) return false;
  return (d.state === "interrupted" && !d.canResume) || d.state === "canceled";
}

/** The status line of a row in the downloads list. */
export function downloadStatus(
  d: DownloadItem,
  timeAgo: (ms: number) => string,
): string {
  if (d.kind === "torrent") return torrentStatus(d, timeAgo);
  const detail = d.detail ? ` · ${d.detail}` : "";
  if (d.state === "progress") {
    if (d.live) return `${d.detail || "Recording"} · ${formatSize(d.received)}`;
    const size =
      d.total > 0
        ? `${formatSize(d.received)} of ${formatSize(d.total)}`
        : formatSize(d.received);
    return d.paused ? `Paused · ${size}` : `${size}${detail}`;
  }
  if (d.state === "completed") {
    return d.detail
      ? `${formatSize(d.total || d.received)} · ${d.detail}`
      : `${formatSize(d.total || d.received)} · ${timeAgo(d.ended ?? d.started)}`;
  }
  if (d.state === "canceled") return "Canceled";
  return d.reason || "Failed";
}
