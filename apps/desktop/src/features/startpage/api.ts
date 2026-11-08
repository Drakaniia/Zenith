// Backend storage for uploaded start-page backgrounds.
import { invoke } from "@tauri-apps/api/core";

export const startpageApi = {
  /** Start-page backgrounds the user uploaded, stored as data URLs. */
  wallpaperSave: (id: string, data: string) => invoke<void>("wallpaper_save", { id, data }),
  wallpaperLoad: (id: string) => invoke<string | null>("wallpaper_load", { id }),
  wallpaperDelete: (id: string) => invoke<void>("wallpaper_delete", { id }),
};
