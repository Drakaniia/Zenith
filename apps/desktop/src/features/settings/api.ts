// Backend calls for settings, browsing data and memory statistics.
import { invoke } from "@tauri-apps/api/core";
import type { MemoryStats } from "./types";

export interface RuntimeConfig {
  downloadDir: string | null;
  recordHistory: boolean;
  blockPopups: boolean;
  blockerEnabled: boolean;
  mediaButton: boolean;
  mediaDisabledSites: string[];
  useFfmpeg: boolean;
  ffmpegPath: string | null;
  useYtdlp: boolean;
  ytdlpPath: string | null;
  autoUpdate: boolean;
  previewUpdates: boolean;
  /** Torrents (see settings.svelte.ts). */
  torrentsEnabled: boolean;
  torrentSeedRatio: number;
  torrentDownKib: number;
  torrentUpKib: number;
  torrentChooseFiles: boolean;
  /** Hidden fast mode (several connections per download). */
  fastDownload: boolean;
  /** Password manager and autofill (see settings.svelte.ts). */
  passwordsEnabled: boolean;
  passwordsOfferSave: boolean;
  cardsOfferSave: boolean;
  autofillCards: boolean;
  autofillSuggest: boolean;
  autofillUsernames: boolean;
  autofillPasswords: boolean;
  autofillOnLoad: boolean;
  autofillSameSite: boolean;
  passwordGenerator: boolean;
  vaultAskEveryTime: boolean;
  vaultLockMinutes: number;
  extensionsInIncognito: boolean;
}

export interface Appearance {
  theme: string;
  material: string;
  pageBackground: [number, number, number];
  contentScheme: string;
}

export const settingsApi = {
  applySettings: (config: RuntimeConfig, appearance: Appearance) =>
    invoke<boolean | null>("apply_settings", { config, appearance }),
  clearBrowsingData: (
    kinds: string[],
    sinceMs: number | null,
    history: boolean,
  ) => invoke<void>("clear_browsing_data", { kinds, sinceMs, history }),
  settingsLoad: () => invoke<string | null>("settings_load"),
  settingsSave: (data: string) => invoke<void>("settings_save", { data }),
  memoryStats: () => invoke<MemoryStats>("memory_stats"),
  systemMemoryLoad: () => invoke<number>("system_memory_load"),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    /** Another normal window saved the settings. */
    "settings-changed": { origin: string };
  }
}
