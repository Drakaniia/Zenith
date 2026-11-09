// Backend calls for keyboard shortcuts.
import { invoke } from "@tauri-apps/api/core";

export const shortcutsApi = {
  /** Applies shortcut bindings; `global` ones also work while another app has the focus.
   *  Returns the global shortcuts that could not be registered (another app owns them). */
  setShortcuts: (
    bindings: { action: string; keys: string[]; global?: boolean }[],
  ) => invoke<string[] | null>("set_shortcuts", { bindings }),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    shortcut: {
      action: string;
      repeat: boolean;
      source: string | null;
      /** The keys pressed, for shortcuts pressed in the chrome itself (`vk`: virtual-key code). */
      keys?: { ctrl: boolean; alt: boolean; shift: boolean; vk: number };
    };
  }
}
