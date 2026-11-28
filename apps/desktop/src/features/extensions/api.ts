// Backend calls for browser extensions.
import { invoke } from "@tauri-apps/api/core";
import type {
  ActionOutcome,
  BadgeEvent,
  ExtensionInfo,
  ExtensionUpdate,
  InstallEvent,
  NativeHostInfo,
  WindowSyncSnapshot,
} from "./types";

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
  extSetIncognito: (id: string, allowed: boolean) =>
    invoke<void>("ext_set_incognito", { id, allowed }),
  extSetEnabled: (id: string, enabled: boolean) =>
    invoke<void>("ext_set_enabled", { id, enabled }),
  extSyncState: (snapshot: WindowSyncSnapshot) =>
    invoke<void>("ext_sync_state", { snapshot }),
  extBadges: () => invoke<BadgeEvent[]>("ext_badges"),
  extNativeHosts: () => invoke<NativeHostInfo[]>("ext_native_hosts"),
  extNativeHostConsent: (hostName: string, allowed: boolean) =>
    invoke<void>("ext_native_host_consent", { hostName, allowed }),
  extTriggerAction: (id: string, tabId?: string | null) =>
    invoke<ActionOutcome>("ext_trigger_action", { id, tabId: tabId ?? null }),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    "ext-install": InstallEvent;
    "ext-badge": BadgeEvent;
  }
}
