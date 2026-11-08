// Window actions: new, incognito and closed windows, full screen, quitting, the UI's devtools.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { registerActions } from "../../platform/actions";
import { ui } from "../../shared/ui/ui.svelte";
import { isWebUrl } from "../../shared/url";
import { tabsApi } from "../browser/api";
import { browser } from "../browser/browser.svelte";
import { notes } from "../notes/notes.svelte";
import { windowApi } from "./api";
import { incognito } from "./window.svelte";

/** Opens a new incognito window (a separate process), optionally with pages to load. */
export async function openIncognito(urls: string[] = []) {
  try {
    await windowApi.incognitoOpen(urls.filter(isWebUrl));
  } catch (e) {
    ui.toast(String(e), { icon: "incognito" });
  }
}

/** Saves the session, then quits. */
export async function quitApp() {
  try {
    await notes.flush();
    await tabsApi.sessionSave(browser.snapshot());
  } finally {
    await windowApi.quit();
  }
}

const devtools = () => windowApi.chromeDevtools().catch((e) => ui.toast(String(e), { icon: "alert" }));

registerActions({
  quit: () => quitApp(),
  newIncognitoWindow: () => openIncognito(),
  // From an incognito window a normal window opens, without that window's spaces.
  newWindow: () =>
    windowApi.windowOpen(incognito ? null : browser.snapshot()).catch((e) => ui.toast(String(e), { icon: "alert" })),
  closeWindow: () => void getCurrentWindow().close(),
  undoCloseWindow: async () => {
    if (!(await windowApi.windowUndoClose().catch(() => false)))
      ui.toast("No recently closed windows", { icon: "window" });
  },
  fullscreen: async () => {
    const w = getCurrentWindow();
    return w.setFullscreen(!(await w.isFullscreen()));
  },
  browserToolbox: devtools,
  browserConsole: devtools,
});
