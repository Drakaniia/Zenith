// Downloads, the media downloader and torrents, as the backend reports them.

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
  /** "media" for the media downloader's jobs, "torrent" for torrents, "file" for ordinary
   *  downloads. */
  kind?: "file" | "media" | "torrent";
  /** Progress detail of a media job ("Segment 12 of 80", "Recording 3:05") or a note. */
  detail?: string;
  /** A live recording: it can be stopped (keeping what was recorded) but not paused. */
  live?: boolean;
  /** First event of a page download: where the pointer was in that tab's window (CSS px) as it
   *  started, for the fly-to-downloads animation. */
  origin?: { x: number; y: number; window: string };
  /** Peers, speeds, ratio and stage of a torrent. */
  torrent?: TorrentInfo;
}

/** Where a torrent is: getting its info, waiting for the file choice, checking data on disk,
 *  downloading, paused, seeding, done (no longer seeding) or failed. */
export type TorrentStage =
  | "metadata"
  | "choosing"
  | "checking"
  | "downloading"
  | "paused"
  | "seeding"
  | "done"
  | "error";

export interface TorrentInfo {
  stage: TorrentStage | null;
  peers: number;
  seeders: number;
  /** Bytes per second. */
  downSpeed: number;
  upSpeed: number;
  /** Seconds left, when known. */
  eta?: number;
  /** Bytes uploaded over every run. */
  uploaded: number;
  ratio: number;
  /** Seeding stops at this ratio (0: no seeding). */
  ratioLimit: number;
  files: number;
  selected: number;
  /** Its files can be played while they download. */
  streamable: boolean;
}

/** One file of a torrent, for the file picker. */
export interface TorrentFile {
  index: number;
  /** Path inside the torrent ("Season 1/Episode 1.mkv"). */
  name: string;
  size: number;
  /** Bytes downloaded. */
  done: number;
  selected: boolean;
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
