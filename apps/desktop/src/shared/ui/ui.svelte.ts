// Transient UI state: which overlay is open (palette, menu, find bar, popovers, dialogs), toasts
// and the compact-mode reveal. Feature stores own their data; this only coordinates what shows.
import type { IconName } from "./icons";
import type { TabId } from "../types";
import type { AnchorRect, MenuItem } from "./menu";
import { api } from "$platform/ipc";

/** `search`: Zen's Focus Search, where the typed text is always a web search. */
export type PaletteMode = "navigate" | "newTab" | "command" | "search";

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
  /** Milliseconds on screen; 0 keeps the toast (with a close button) until it is dismissed. */
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
  /** Bumped by Ctrl+F on the settings page to focus its search box. */
  settingsSearch = $state(0);
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

  toast(text: string, opts: Partial<Omit<Toast, "id" | "text">> = {}): number {
    const t: Toast = { id: ++this.#toastId, text, timeout: 3200, ...opts };
    // At most three on screen: the oldest passing toast makes room, a sticky one stays.
    const list = [...this.toasts, t];
    while (list.length > 3) {
      const i = list.findIndex((x) => x.timeout > 0 && x !== t);
      if (i < 0) break;
      list.splice(i, 1);
    }
    this.toasts = list;
    if (t.timeout > 0) setTimeout(() => this.dismiss(t.id), t.timeout);
    return t.id;
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }
}

export const ui = new UiStore();
