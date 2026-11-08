import type { ProfileInfo } from "../profiles/types";

/** What the backend tells a window about itself when its UI is ready (`ui_ready`). */

export interface StartupInfo {
  testProfile: boolean;
  /** The browser profile this window belongs to. */
  profile: ProfileInfo;
  /** An incognito window: nothing it does is saved. */
  incognito: boolean;
  startupUrls: string[];
  webviewVersion: string;
  version: string;
  dataDir: string;
  defaultDownloadDir: string;
}
