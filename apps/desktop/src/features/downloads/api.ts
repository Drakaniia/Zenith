// Backend calls for downloads, the media downloader and torrents.
import { invoke } from "@tauri-apps/api/core";
import type {
  AudioFormat,
  DownloadItem,
  MediaReport,
  MediaToolName,
  MediaToolProgress,
  MediaTools,
  TorrentFile,
  UrlProbeResult,
} from "./types";

export const downloadsApi = {
  downloadsActive: () => invoke<DownloadItem[]>("downloads_active"),
  downloadsHistory: (limit = 200) =>
    invoke<DownloadItem[]>("downloads_history", { limit }),
  downloadAction: (
    id: number,
    action: "pause" | "resume" | "cancel" | "stop",
  ) => invoke<void>("download_action", { id, action }),
  /** Fetches a failed or canceled download that can't resume once more; the new download's id. */
  downloadAgain: (id: number) => invoke<number>("download_again", { id }),
  downloadsRemove: (id: number | null) =>
    invoke<void>("downloads_remove", { id }),
  /** Drags a finished download's file from the pointer until it is dropped; true when it was. */
  downloadDrag: (id: number) => invoke<boolean>("download_drag", { id }),
  mediaList: (tabId: string, scan = true) =>
    invoke<MediaReport>("media_list", { tabId, scan }),
  mediaDownload: (
    tabId: string,
    itemId: number,
    variant: string | null,
    audio: string | null,
    audioFormat: AudioFormat | null = null,
  ) =>
    invoke<number>("media_download", {
      tabId,
      itemId,
      variant,
      audio,
      audioFormat,
    }),
  mediaTools: () => invoke<MediaTools>("media_tools"),
  mediaPickFfmpeg: () => invoke<string | null>("media_pick_ffmpeg"),
  mediaPickYtdlp: () => invoke<string | null>("media_pick_ytdlp"),
  mediaInstallTool: (tool: MediaToolName) =>
    invoke<string>("media_install_tool", { tool }),
  mediaRemoveTool: (tool: MediaToolName) =>
    invoke<void>("media_remove_tool", { tool }),
  mediaProbeUrl: (url: string, preferYtdlp = false) =>
    invoke<UrlProbeResult>("media_probe_url", { url, preferYtdlp }),
  mediaDownloadProbed: (
    probeId: string,
    itemId: number,
    variant: string | null,
    audio: string | null,
    audioFormat: AudioFormat | null = null,
    tabId: string | null = null,
  ) =>
    invoke<number>("media_download_probed", {
      probeId,
      itemId,
      variant,
      audio,
      audioFormat,
      tabId,
    }),
  /** Torrents: each returns the new download's id. */
  torrentAddMagnet: (uri: string) =>
    invoke<number>("torrent_add_magnet", { uri }),
  torrentAddBytes: (bytes: number[]) =>
    invoke<number>("torrent_add_bytes", { bytes }),
  /** Asks for a .torrent file; null when the user canceled. */
  torrentOpenFile: () => invoke<number | null>("torrent_open_file"),
  torrentFiles: (id: number) => invoke<TorrentFile[]>("torrent_files", { id }),
  torrentChooseFiles: (id: number, files: number[]) =>
    invoke<void>("torrent_choose_files", { id, files }),
  /** A local address that plays the file while it downloads. */
  torrentStreamUrl: (id: number, file: number) =>
    invoke<string>("torrent_stream_url", { id, file }),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    download: DownloadItem;
    "media-changed": { tabId: string; count: number };
    "media-request": {
      tabId: string;
      hint: string;
      anchor?: { x: number; y: number; w: number; h: number; dpr: number };
    };
    "media-tool-progress": MediaToolProgress;
    /** A torrent could not be added or resumed (shown as a toast). */
    "torrent-error": string;
    /** A download was skipped because the file is already downloaded (shown as a toast). */
    "download-exists": string;
  }
}
