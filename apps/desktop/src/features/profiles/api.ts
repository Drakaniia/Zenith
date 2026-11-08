// Backend calls for browser profiles.
import { invoke } from "@tauri-apps/api/core";
import type { ProfileInfo, ProfilesView } from "./types";

export const profilesApi = {
  /** Every browser profile, which are open, and the colours to offer. */
  profilesList: () => invoke<ProfilesView>("profiles_list"),
  /**
   * Adds a profile (and, with `shortcut`, a desktop shortcut for it). `avatar` is `image`,
   * `emoji:<name>` or null (the initial); `icon` is the picture rendered as a 256 px PNG
   * (base64 or a data URL) for the emoji and image kinds.
   */
  profileCreate: (name: string, color: string, avatar: string | null, icon: string | null, shortcut: boolean) =>
    invoke<ProfileInfo>("profile_create", { name, color, avatar, icon, shortcut }),
  profileUpdate: (id: string, name: string, color: string, avatar: string | null, icon: string | null) =>
    invoke<ProfileInfo>("profile_update", { id, name, color, avatar, icon }),
  /** Opens a profile in its own window (or brings its open window to the front). */
  profileOpen: (id: string) => invoke<void>("profile_open", { id }),
  /** Deletes a profile that isn't open, with all of its data. */
  profileDelete: (id: string) => invoke<void>("profile_delete", { id }),
  /** Creates the profile's desktop shortcut; returns its path. */
  profileShortcut: (id: string) => invoke<string>("profile_shortcut", { id }),
};
