// Backend calls for the ad and tracker blocker.
import { invoke } from "@tauri-apps/api/core";
import type { BlockerStats } from "./types";

export const blockerApi = {
  blockerConfigure: (
    lists: { id: string; url: string }[],
    update: boolean,
    disabledSites: string[],
  ) =>
    invoke<BlockerStats>("blocker_configure", { lists, update, disabledSites }),
  blockerSetDisabledSites: (sites: string[]) =>
    invoke<void>("blocker_set_disabled_sites", { sites }),
  blockerStats: () => invoke<BlockerStats>("blocker_stats"),
};
