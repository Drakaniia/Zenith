// The rest of the preview's commands: start-page wallpapers kept in memory, and fixed answers.
import { UNHANDLED } from "./handler";

// Uploaded start-page backgrounds, kept in memory.
const previewWallpapers = new Map<string, string>();

/** Wallpapers, empty lists, blocker, updates, media and window queries. */
export function handleOther(cmd: string, a: Record<string, unknown>): unknown {
  switch (cmd) {
    case "wallpaper_save":
      previewWallpapers.set(a.id as string, a.data as string);
      return null;
    case "wallpaper_load":
      return previewWallpapers.get(a.id as string) ?? null;
    case "wallpaper_delete":
      previewWallpapers.delete(a.id as string);
      return null;
    case "history_search":
    case "history_top":

    case "downloads_active":
    case "downloads_history":
    case "ext_list":
    case "ext_check_updates":
    case "search_suggest":
      return [];
    case "blocker_configure":
    case "blocker_stats":
      return { rules: 93000, totalBlocked: 0, lists: [] };
    case "update_status":
      return { state: "idle", currentVersion: "0.0.0-preview" };
    case "media_list":
      return { items: [], blobs: 0, drm: 0, ffmpeg: false };
    case "plugin:window|is_maximized":
    case "plugin:window|is_fullscreen":
      return false;
    default:
      return UNHANDLED;
  }
}
