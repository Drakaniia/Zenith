// This window: what the backend reported when its UI became ready, and whether it is incognito.
import type { StartupInfo } from "./types";

class AppInfo {
  info = $state<StartupInfo | null>(null);
}

export const appInfo = new AppInfo();

/** An incognito window (a separate process on a throwaway profile; see incognito.rs). The
 *  backend injects the flag before the UI loads, so it is known before the first render.
 *  The browser-only dev preview fakes it with `?incognito`. */
export const incognito: boolean =
  (window as { __ZENITH_INCOGNITO__?: boolean }).__ZENITH_INCOGNITO__ ===
    true ||
  (import.meta.env.DEV &&
    !("__TAURI_INTERNALS__" in window) &&
    new URLSearchParams(location.search).has("incognito"));
