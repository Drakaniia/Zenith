// Backend calls for default-browser registration.
import { invoke } from "@tauri-apps/api/core";

export const defaultBrowserApi = {
  defaultBrowserStatus: () =>
    invoke<{ registered: boolean; isDefault: boolean }>(
      "default_browser_status",
    ),
  defaultBrowserRegister: (openSettings = true) =>
    invoke<void>("default_browser_register", { openSettings }),
  defaultBrowserUnregister: () => invoke<void>("default_browser_unregister"),
};
