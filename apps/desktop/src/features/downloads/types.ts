// Downloads and the media downloader, as the backend reports them.

export interface DownloadItem {
  id: number;
  url: string;
  path: string;
  mime: string;
  total: number;
  received: number;
  state: "progress" | "completed" | "interrupted" | "canceled";
  started: number;
  ended?: number | null;
  fileName?: string;
  paused?: boolean;
  canResume?: boolean;
  tabId?: string;
  reason?: string;
  /** "media" for the media downloader's jobs, "file" for ordinary downloads. */
  kind?: "file" | "media";
  /** Progress detail of a media job ("Segment 12 of 80", "Recording 3:05") or a note. */
  detail?: string;
  /** A live recording: it can be stopped (keeping what was recorded) but not paused. */
  live?: boolean;
}

/** A quality or audio-language choice offered for a stream. */
export interface MediaChoice {
  id: string;
  label: string;
  height?: number | null;
  bandwidth?: number | null;
  audioOnly: boolean;
}

/** A video or audio item found on a page. */
export interface MediaItem {
  id: number;
  kind: "file" | "hls" | "dash";
  url: string;
  title: string;
  ext: string;
  size?: number | null;
  duration?: number | null;
  width?: number | null;
  height?: number | null;
  audioOnly: boolean;
  live: boolean;
  protected: boolean;
  foundBy: "network" | "page";
  detected: number;
  variants: MediaChoice[];
  audioTracks: MediaChoice[];
  note: string;
}

export interface MediaReport {
  items: MediaItem[];
  /** Players streaming through Media Source (blob: sources). */
  blobs: number;
  /** Players using DRM (Encrypted Media Extensions). */
  drm: number;
  ffmpeg: boolean;
}
