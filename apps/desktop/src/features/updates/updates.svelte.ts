// App updates: mirrors the backend's update state and restarts into a downloaded update.
import { api } from "$platform/ipc";
import { browser } from "$features/browser/browser.svelte";
import { ui } from "$shared/ui/ui.svelte";
import type { UpdateStatus } from "./types";

class UpdateStore {
  status = $state<UpdateStatus>({ state: "idle", currentVersion: "" });
  restarting = $state(false);
  #announced = "";

  ready = $derived(this.status.state === "ready");

  async load() {
    try {
      this.apply(await api.updateStatus());
    } catch {
      /* the backend may not be ready yet */
    }
  }

  /** From `update-status` events and command results. */
  apply(s: UpdateStatus) {
    this.status = s;
    if (s.state === "ready" && s.version && this.#announced !== s.version) {
      this.#announced = s.version;
      ui.toast(
        `Zenith ${s.version} is ready. It installs when you close Zenith.`,
        {
          icon: "arrowDownToLine",
          timeout: 9000,
          action: { label: "Restart now", run: () => void this.restart() },
        },
      );
    }
  }

  async check() {
    this.apply(await api.updateCheck());
  }

  /** Saves the session, then quits into the update; the installer reopens Zenith. */
  async restart() {
    if (this.restarting) return;
    this.restarting = true;
    try {
      await api.sessionSave(browser.snapshot());
      await api.updateRestart();
    } catch (e) {
      this.restarting = false;
      ui.toast(`Could not install the update: ${e}`, { icon: "alert" });
    }
  }
}

export const updates = new UpdateStore();

/** "Checked 5 min ago" style text for the settings page. */
export function updateLine(
  s: UpdateStatus,
  ago: (ms: number) => string,
): string {
  switch (s.state) {
    case "checking":
      return "Checking for updates…";
    case "downloading":
      return `Downloading Zenith ${s.version ?? ""}${s.progress != null ? ` · ${Math.round(s.progress * 100)}%` : ""}`;
    case "ready":
      return `Zenith ${s.version ?? ""} is ready. It installs when you close Zenith.`;
    case "upToDate":
      return `Zenith is up to date${s.checked ? ` · checked ${ago(s.checked)}` : ""}`;
    case "error":
      return `Could not check for updates${s.error ? `: ${s.error}` : ""}`;
    default:
      return "Zenith checks for updates in the background.";
  }
}
