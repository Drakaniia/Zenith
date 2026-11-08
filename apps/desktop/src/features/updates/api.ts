// Backend calls for app updates.
import { invoke } from "@tauri-apps/api/core";
import type { UpdateStatus } from "./types";

export const updatesApi = {
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  updateCheck: () => invoke<UpdateStatus>("update_check"),
  /** Quits into the downloaded update (save the session first); the installer reopens Zenith. */
  updateRestart: () => invoke<void>("update_restart"),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    "update-status": UpdateStatus;
  }
}
