// Backend calls for this window and the app's other windows.
import { invoke } from "@tauri-apps/api/core";
import type { StartupInfo } from "./types";

export const windowApi = {
  uiReady: () => invoke<StartupInfo>("ui_ready"),
  quit: () => invoke<void>("app_quit"),
  /** Closes this window (call after saving the session); the last window quits Zenith. */
  windowClose: () => invoke<void>("window_close"),
  /** Opens a new incognito window (same process, InPrivate pages), optionally loading pages. */
  incognitoOpen: (urls: string[] = []) =>
    invoke<void>("incognito_open", { urls }),
  /** Opens another normal window. `session` (this window's snapshot) gives it the same spaces,
   *  essentials and pinned tabs; its regular tabs are not copied. */
  windowOpen: (session: string | null) => invoke<void>("window_open", { session }),
  /** Reopens the most recently closed normal window; false when there is none. */
  windowUndoClose: () => invoke<boolean>("window_undo_close"),
  /** Called once by the main window at startup: reopens the other windows that were open when
   *  Zenith quit (`keep`), or forgets them. */
  windowsRestore: (keep: boolean) => invoke<void>("windows_restore", { keep }),
  /** Opens the developer tools of this window's own UI (Firefox's Browser Toolbox). */
  chromeDevtools: () => invoke<void>("chrome_devtools"),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    "open-urls": string[];
    "app-close-requested": null;
  }
}
