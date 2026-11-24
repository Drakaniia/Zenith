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
    case "ext_badges":
    case "ext_native_hosts":
    case "search_suggest":
      return [];
    case "ext_sync_state":
    case "ext_native_host_consent":
    case "security_respond_cert":
    case "security_respond_protocol":
    case "security_respond_download":
    case "security_safebrowsing_bypass":
    case "migration_notice_dismiss":
      return null;
    case "security_safebrowsing_status":
    case "security_safebrowsing_update":
      return { enabled: true, entries: 4, updated: null, feeds: ["urlhaus", "openphish"] };
    case "engine_info":
      return {
        cefVersion: "156.0.31+ge603d97",
        chromiumVersion: "156.0.8078.100",
        sandboxEnabled: true,
        customCefBuild: false,
        proprietaryCodecs: false,
        widevineEnabled: false,
        patches: [],
      };
    case "blocker_configure":
    case "blocker_stats":
      return { rules: 93000, totalBlocked: 0, lists: [] };
    case "update_status":
      return { state: "idle", currentVersion: "0.0.0-preview" };
    case "media_list":
      return { items: [], blobs: 0, drm: 0, ffmpeg: false };
    case "media_tools":
      return { ffmpeg: null };
    case "plugin:window|is_maximized":
    case "plugin:window|is_fullscreen":
      return false;
    default:
      return UNHANDLED;
  }
}
