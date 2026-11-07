// Transient UI state: palette, menus, find bar, toasts, compact-mode reveal, dialogs.
import type { IconName } from "./icons";
import type { DownloadItem, PermissionRequest, TabId } from "./types";
import { api } from "./ipc";

/** `search`: Zen's Focus Search, where the typed text is always a web search. */
export type PaletteMode = "navigate" | "newTab" | "command" | "search";

/** Keys held (or the middle button) when a menu item was chosen: a bookmark in a menu opens in
 *  a new tab with Ctrl or the middle button, in Glance with Shift. */
export interface MenuMods {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  middle: boolean;
}

export interface MenuItem {
  label?: string;
  icon?: IconName;
  /** A page's own icon (a bookmark in a menu) in place of a drawn one. */
  site?: { url: string; favicon?: string };
  /**
   * A coloured round badge with a letter (a browser profile's avatar) in place of an icon.
   * `img` replaces the letter (an emoji on the colour); with `photo` it fills the whole badge.
   */
  swatch?: { color: string; fg: string; text: string; img?: string; photo?: boolean };
  hint?: string;
  danger?: boolean;
  checked?: boolean;
  disabled?: boolean;
  separator?: boolean;
  /** A greyed label with no action (a heading, or "(Empty)"). */
  heading?: boolean;
  /** Zen's app-menu zoom row (− 100% + and full screen) for the active tab. */
  zoom?: boolean;
  run?: (mods?: MenuMods) => void;
  /** A submenu, to any depth. A function builds it when it opens (big bookmark folders). */
  children?: MenuItem[] | (() => MenuItem[]);
}

/** A rectangle on screen (CSS px) that a popup hangs from. */
export interface AnchorRect {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

export function rectOf(el: Element): AnchorRect {
  const r = el.getBoundingClientRect();
  return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
}

/** The "Edit bookmark" panel under the address bar's bookmark button. */
export interface BookmarkPopupState {
  /** The bookmark being edited. */
  id: number;
  anchor: AnchorRect;
}

/** The detailed bookmark dialogs (Zen: "Bookmark Tab…", "Bookmark Tabs…", new folder). */
export type BookmarkDialogSpec =
  /** One page: a new bookmark when `id` is missing, otherwise editing it. */
  | { kind: "bookmark"; id?: number; url: string; title: string; favicon?: string; parent?: number }
  /** A folder: new, or renaming `id`. */
  | { kind: "folder"; id?: number; parent?: number }
  /** Several tabs into a new folder. */
  | { kind: "tabs"; tabs: { url: string; title: string; favicon?: string }[] };

/** The profile dialog: the list of profiles, or the form for a new (`id` null) or existing one. */
export type ProfileDialogSpec = { mode: "manage" } | { mode: "edit"; id: string | null };

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
  /** The floating sidebar was shown from the keyboard (Ctrl+Alt+S) and stays until pressed again. */
  flyoutPinned = $state(false);
  /** Compact mode's floating toolbar is showing (it was hidden with Ctrl+Alt+T). */
  topReveal = $state(false);
  downloadsOpen = $state(false);
  /** The media downloader panel: which tab, and the media the user pointed at (if any). */
  media = $state<{ tabId: TabId; hint: string } | null>(null);
  spaceEditor = $state<{ spaceId: string | null } | null>(null);
  dragging = $state(false);
  /** A tab label is being edited inline (Zen: zen-renaming-tab). */
  renaming = $state(false);
  bookmarkPopup = $state<BookmarkPopupState | null>(null);
  bookmarkDialog = $state<BookmarkDialogSpec | null>(null);
  /** Tabs picked with Ctrl+click or Shift+click in the sidebar (for "Bookmark Tabs…"). */
  selectedTabs = $state<TabId[]>([]);
  /** The "import bookmarks from another browser" dialog is open. */
  bookmarkImport = $state(false);
  profileDialog = $state<ProfileDialogSpec | null>(null);
  /** Window controls revealed from the top edge (single-toolbar layout). */
  captionReveal = $state(false);
  shortcutRecording = $state(false);
  #toastId = 0;

  openPalette(mode: PaletteMode, query = "") {
    this.menu = null;
    this.downloadsOpen = false;
    this.media = null;
    this.bookmarkPopup = null;
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

  clearTabSelection() {
    if (this.selectedTabs.length) this.selectedTabs = [];
  }

  /** Ctrl+click: adds the tab to the picked ones or takes it out; the tab you are on counts as
   *  picked from the first click on. */
  toggleTabSelected(id: TabId, activeId?: TabId) {
    const base = !this.selectedTabs.length && activeId && activeId !== id ? [activeId] : this.selectedTabs;
    this.selectedTabs = base.includes(id) ? base.filter((t) => t !== id) : [...base, id];
  }

  /** Shift+click: everything between `from` and `to` in the order tabs are shown. */
  selectTabRange(order: TabId[], from: TabId | undefined, to: TabId) {
    const a = from ? order.indexOf(from) : -1;
    const b = order.indexOf(to);
    if (a < 0 || b < 0) {
      this.selectedTabs = [to];
      return;
    }
    this.selectedTabs = order.slice(Math.min(a, b), Math.max(a, b) + 1);
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
