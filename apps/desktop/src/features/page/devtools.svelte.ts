// Per-tab docked DevTools state and dock layout preferences.
import { tabsApi } from "$features/browser/api";
import type { TabId } from "$features/browser/types";
import { scheduleLayout } from "$platform/layout.svelte";
import { ui } from "$shared/ui/ui.svelte";
import { parseDevToolsLayout, type DockSide } from "./devtools";

const STORAGE_KEY = "zenith.devtools.layout";

function loadLayout() {
  try {
    return parseDevToolsLayout(localStorage.getItem(STORAGE_KEY));
  } catch {
    return parseDevToolsLayout(null);
  }
}

class DevToolsStore {
  /** Tab ids that currently have a live docked DevTools instance. */
  openTabs = $state<Record<TabId, true>>({});
  dock = $state<DockSide>(loadLayout().dock);
  width = $state<number>(loadLayout().width);
  height = $state<number>(loadLayout().height);

  private persist() {
    try {
      localStorage.setItem(
        STORAGE_KEY,
        JSON.stringify({ dock: this.dock, width: this.width, height: this.height }),
      );
    } catch {
      /* Storage unavailable: keep in memory. */
    }
  }

  isOpen(id: TabId | undefined): boolean {
    return !!id && !!this.openTabs[id];
  }

  setOpen(id: TabId, open: boolean) {
    if (open) {
      if (this.openTabs[id]) return;
      this.openTabs = { ...this.openTabs, [id]: true };
    } else {
      if (!this.openTabs[id]) return;
      const next = { ...this.openTabs };
      delete next[id];
      this.openTabs = next;
    }
    scheduleLayout();
  }

  dropTab(id: TabId) {
    this.setOpen(id, false);
  }

  setDock(side: DockSide) {
    if (this.dock === side) return;
    this.dock = side;
    this.persist();
    scheduleLayout();
  }

  setSize(side: DockSide, px: number) {
    if (side === "bottom") this.height = px;
    else this.width = px;
    this.persist();
  }

  async toggle(id: TabId) {
    try {
      const open = await tabsApi.tabCommand<boolean>(id, "devtools", { mode: "toggle" });
      this.setOpen(id, !!open);
    } catch (e) {
      ui.toast(String(e), { icon: "alert" });
    }
  }

  async open(id: TabId, panel?: string) {
    try {
      const open = await tabsApi.tabCommand<boolean>(id, "devtools", {
        mode: "open",
        ...(panel ? { panel } : {}),
      });
      this.setOpen(id, !!open);
    } catch (e) {
      ui.toast(String(e), { icon: "alert" });
    }
  }

  async close(id: TabId) {
    this.setOpen(id, false);
    try {
      await tabsApi.tabCommand<boolean>(id, "devtools", { mode: "close" });
    } catch {
      /* Tab may already be closed or sleeping. */
    }
  }
}

export const devtools = new DevToolsStore();
