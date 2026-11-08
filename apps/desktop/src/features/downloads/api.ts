// Backend calls for downloads and the media downloader.
import { invoke } from "@tauri-apps/api/core";
import type { DownloadItem, MediaReport } from "./types";

export const downloadsApi = {
  downloadsActive: () => invoke<DownloadItem[]>("downloads_active"),
  downloadsHistory: (limit = 200) =>
    invoke<DownloadItem[]>("downloads_history", { limit }),
  downloadAction: (
    id: number,
    action: "pause" | "resume" | "cancel" | "stop",
  ) => invoke<void>("download_action", { id, action }),
  downloadsRemove: (id: number | null) =>
    invoke<void>("downloads_remove", { id }),
  mediaList: (tabId: string, scan = true) =>
    invoke<MediaReport>("media_list", { tabId, scan }),
  mediaDownload: (
    tabId: string,
    itemId: number,
    variant: string | null,
    audio: string | null,
  ) => invoke<number>("media_download", { tabId, itemId, variant, audio }),
  mediaTools: () => invoke<{ ffmpeg: string | null }>("media_tools"),
  mediaPickFfmpeg: () => invoke<string | null>("media_pick_ffmpeg"),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    download: DownloadItem;
    "media-changed": { tabId: string; count: number };
    "media-request": { tabId: string; hint: string };
  }
}
