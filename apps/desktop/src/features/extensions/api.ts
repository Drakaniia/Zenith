// Backend calls for browser extensions.
import { invoke } from "@tauri-apps/api/core";
import type { ExtensionInfo, ExtensionUpdate, InstallEvent } from "./types";

export const extensionsApi = {
  extList: () => invoke<ExtensionInfo[]>("ext_list"),
  extInstallFolder: () =>
    invoke<ExtensionInfo | null>("ext_install_folder", { path: null }),
  extInstallCrx: () => invoke<ExtensionInfo | null>("ext_install_crx"),
  extInstallStore: (input: string) =>
    invoke<ExtensionInfo>("ext_install_store", { input }),
  /** Installs an extension the user accepted in the install prompt. */
  extOfferAccept: (token: number) =>
    invoke<ExtensionInfo>("ext_offer_accept", { token }),
  extOfferDiscard: (token: number) =>
    invoke<void>("ext_offer_discard", { token }),
  extCheckUpdates: () => invoke<ExtensionUpdate[]>("ext_check_updates"),
  extUpdate: (id: string) => invoke<ExtensionInfo>("ext_update", { id }),
  extRemove: (id: string) => invoke<void>("ext_remove", { id }),
  extSetEnabled: (id: string, enabled: boolean) =>
    invoke<void>("ext_set_enabled", { id, enabled }),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    "ext-install": InstallEvent;
  }
}
