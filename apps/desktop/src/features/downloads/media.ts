// Display helpers for the media downloader (kept free of Svelte state so they can be unit tested).
import type { DownloadItem, MediaItem } from "../../shared/types";

/** 3:07, 1:02:05 */
export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

export function formatSize(n: number): string {
  if (!n || n < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(
    units.length - 1,
    Math.floor(Math.log(n) / Math.log(1024)),
  );
  const v = n / 1024 ** i;
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
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

/** The status line of a row in the downloads list. */
export function downloadStatus(
  d: DownloadItem,
  timeAgo: (ms: number) => string,
): string {
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
