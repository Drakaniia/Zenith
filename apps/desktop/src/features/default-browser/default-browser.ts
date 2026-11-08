// The "make Zenith your default browser" prompt: a toast on every launch while Zenith isn't the
// default. Windows doesn't let apps take over links silently, so the action opens its settings.
import { api } from "../../platform/ipc";
import { ui } from "../../shared/ui/ui.svelte";

let promptId = 0;

/** Shows the prompt (it stays until acted on or closed) if Zenith isn't the default browser. */
export async function promptDefaultBrowser() {
  const status = await api.defaultBrowserStatus().catch(() => null);
  if (!status || status.isDefault) return;
  ui.dismiss(promptId);
  promptId = ui.toast("Zenith isn't your default browser", {
    icon: "globe",
    timeout: 0,
    action: { label: "Make default", run: () => void makeDefaultBrowser() },
  });
}

/** Registers Zenith and opens the Windows default-apps page on it. */
export async function makeDefaultBrowser() {
  ui.dismiss(promptId);
  try {
    await api.defaultBrowserRegister();
    ui.toast("Choose Zenith in the Windows settings window that opened", { icon: "globe", timeout: 6000 });
  } catch (e) {
    ui.toast(`Registration failed: ${e}`, { icon: "alert" });
    throw e;
  }
}
