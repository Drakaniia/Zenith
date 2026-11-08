// Site icons fetched by the backend (the page CSP allows no remote images).
import { invoke } from "@tauri-apps/api/core";

export const faviconsApi = {
  /** A site's `/favicon.ico` as a data URL (fetched by the backend; the page CSP allows no remote images). */
  faviconFetch: (url: string) => invoke<string | null>("favicon_fetch", { url }),
};
