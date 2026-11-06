// Transient UI state: palette, menus, find bar, toasts, compact-mode reveal, dialogs.
import type { IconName } from "./icons";
import type { DownloadItem, PermissionRequest, TabId } from "./types";
import { api } from "./ipc";

export type PaletteMode = "navigate" | "newTab" | "command";

export interface MenuItem {
  label?: string;
  icon?: IconName;
  hint?: string;
  danger?: boolean;
  checked?: boolean;
  disabled?: boolean;
  separator?: boolean;
  /** Zen's app-menu zoom row (− 100% + and full screen) for the active tab. */
  zoom?: boolean;
  run?: () => void;
  children?: MenuItem[];
}

export interface Toast {
  id: number;
  text: string;
  icon?: IconName;
  action?: { label: string; run: () => void };
  timeout: number;
}

class UiStore {
  palette = $state({
    open: false,
    mode: "navigate" as PaletteMode,
    query: "",
    selectAll: true,
  });
  menu = $state<{
    x: number;
    y: number;
    items: MenuItem[];
    key: number;
  } | null>(null);
  find = $state({
    open: false,
    tabId: "" as TabId,
    query: "",
    matches: 0,
    active: -1,
    caseSensitive: false,
  });
  toasts = $state<Toast[]>([]);
  flyout = $state(false);
  topReveal = $state(false);
  downloadsOpen = $state(false);
  /** The media downloader panel: which tab, and the media the user pointed at (if any). */
  media = $state<{ tabId: TabId; hint: string } | null>(null);
  spaceEditor = $state<{ spaceId: string | null } | null>(null);
  dragging = $state(false);
  /** A tab label is being edited inline (Zen: zen-renaming-tab). */
  renaming = $state(false);
  /** Window controls revealed from the top edge (single-toolbar layout). */
  captionReveal = $state(false);
  shortcutRecording = $state(false);
  #toastId = 0;

  openPalette(mode: PaletteMode, query = "") {
    this.menu = null;
    this.downloadsOpen = false;
    this.media = null;
    this.palette = { open: true, mode, query, selectAll: true };
    void api.focusView(null);
  }

  closePalette() {
    this.palette.open = false;
  }

  openMedia(tabId: TabId, hint = "") {
    this.menu = null;
    this.downloadsOpen = false;
    this.palette.open = false;
    this.media = { tabId, hint };
    void api.focusView(null);
  }

  openMenu(x: number, y: number, items: MenuItem[]) {
    this.menu = { x, y, items, key: Date.now() };
  }

  closeMenu() {
    this.menu = null;
  }

  toast(text: string, opts: Partial<Omit<Toast, "id" | "text">> = {}) {
    const t: Toast = { id: ++this.#toastId, text, timeout: 3200, ...opts };
    this.toasts = [...this.toasts.slice(-2), t];
    setTimeout(() => this.dismiss(t.id), t.timeout);
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }
}

export const ui = new UiStore();

class DownloadStore {
  items = $state<DownloadItem[]>([]);
  loaded = false;

  active = $derived(this.items.filter((d) => d.state === "progress"));
  progress = $derived.by(() => {
    const running = this.items.filter(
      (d) => d.state === "progress" && d.total > 0,
    );
    if (!running.length) return -1;
    const total = running.reduce((a, d) => a + d.total, 0);
    const got = running.reduce((a, d) => a + d.received, 0);
    return total ? got / total : -1;
  });

  async load() {
    if (this.loaded) return;
    this.loaded = true;
    const [history, active] = await Promise.all([
      api.downloadsHistory(200),
      api.downloadsActive(),
    ]);
    const map = new Map<number, DownloadItem>();
    for (const d of history) map.set(d.id, d);
    for (const d of active) map.set(d.id, d);
    this.items = [...map.values()].sort((a, b) => b.started - a.started);
  }

  upsert(d: DownloadItem) {
    const i = this.items.findIndex((x) => x.id === d.id);
    if (i >= 0) this.items[i] = { ...this.items[i], ...d };
    else this.items = [d, ...this.items];
  }

  /** A WebView2 download is running in the tab (closing its webview would cancel it). Media
   *  downloader jobs run on their own and do not keep a tab awake. */
  busyTab(tabId: string) {
    return this.items.some(
      (d) => d.state === "progress" && d.tabId === tabId && d.kind !== "media",
    );
  }

  async remove(id: number | null) {
    await api.downloadsRemove(id);
    this.items =
      id === null
        ? this.items.filter((d) => d.state === "progress")
        : this.items.filter((d) => d.id !== id);
  }
}

export const downloads = new DownloadStore();

class PermissionStore {
  queue = $state<PermissionRequest[]>([]);
  push(p: PermissionRequest) {
    this.queue = [...this.queue, p];
  }
  async answer(p: PermissionRequest, allow: boolean, remember: boolean) {
    this.queue = this.queue.filter((x) => x.requestId !== p.requestId);
    await api.permissionRespond(p.requestId, allow, remember);
  }
  dropTab(tabId: string) {
    this.queue = this.queue.filter((x) => x.tabId !== tabId);
  }
}

export const permissions = new PermissionStore();

export function formatBytes(n: number): string {
  if (!n || n < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(
    units.length - 1,
    Math.floor(Math.log(n) / Math.log(1024)),
  );
  const v = n / 1024 ** i;
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

export function timeAgo(ms: number): string {
  const s = Math.max(0, (Date.now() - ms) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  const d = new Date(ms);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}
