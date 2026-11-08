// The browser model's data: tabs, spaces, tab folders, split view and Glance.

export type TabId = string;
export type SpaceId = string;

/** A tab as the chrome UI models it. Runtime fields come from the backend's `tab-update`. */
export interface Tab {
  id: TabId;
  url: string;
  title: string;
  /** Name the user gave the tab (Zen: rename tab); overrides the page title. */
  customTitle?: string;
  favicon?: string;
  spaceId: SpaceId;
  pinned: boolean;
  essential: boolean;
  /** Original URL of a pinned/essential tab, used by "Reset to pinned URL". */
  pinnedUrl?: string;
  openerId?: TabId;
  lastActive: number;
  scroll?: { x: number; y: number };
  zoom: number;
  muted: boolean;
  /** Has a live webview in the backend. */
  live: boolean;
  /** Webview suspended (cheap first sleep stage). */
  suspended: boolean;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  audible: boolean;
  blocked: number;
  crashed: string;
  error: string;
  fullscreen: boolean;
  /** Videos and audio the media downloader found on the current page. */
  media: number;
}

/** Zen's split layouts: side by side (`vsep`, a vertical separator), stacked (`hsep`) or a grid. */
export type SplitLayout = "vsep" | "hsep" | "grid";

export interface SplitState {
  /** Two to four tabs, in pane order. */
  ids: TabId[];
  /** Share of the first pane with two panes side by side or stacked. */
  ratio: number;
  /** Missing in sessions from earlier versions: side by side. */
  layout?: SplitLayout;
}

/** A Zen tab folder in a space's pinned section. Its tabs stay in `Space.pinnedIds` (so every
 *  pinned-tab rule applies to them); the folder only groups them and orders them visually. */
export interface Folder {
  id: string;
  name: string;
  collapsed: boolean;
  tabIds: TabId[];
}

export interface Space {
  id: SpaceId;
  name: string;
  icon: string;
  color: string;
  tabIds: TabId[];
  pinnedIds: TabId[];
  folders?: Folder[];
  /** Zen: collapse the pinned section from the space indicator. */
  pinnedCollapsed?: boolean;
  activeTabId?: TabId;
  split?: SplitState;
}

export interface ClosedTab {
  url: string;
  title: string;
  favicon?: string;
  spaceId: SpaceId;
  index: number;
  pinned: boolean;
  closedAt: number;
}

export interface GlanceState {
  tabId: TabId;
  openerId?: TabId;
  /** A real popup window (window.open with a size) shown as a floating card. */
  popup: boolean;
  /** An extension's toolbar popup: a small card anchored at this point instead of a preview. */
  anchor?: PopupAnchor;
}

/** A toolbar point a menu or popup hangs from (`below`: it opens downward from `y`). */
export interface PopupAnchor {
  x: number;
  y: number;
  below: boolean;
}

/** Partial update emitted by the backend for a tab webview. */
export interface TabPatch {
  id: TabId;
  url?: string;
  title?: string;
  loading?: boolean;
  favicon?: string;
  canGoBack?: boolean;
  canGoForward?: boolean;
  audible?: boolean;
  muted?: boolean;
  zoom?: number;
  blocked?: number;
  crashed?: string;
  fullscreen?: boolean;
  error?: string;
}

export interface OpenRequest {
  url: string;
  openerId?: TabId;
  kind: "glance" | "background" | "split" | "foreground";
}

export interface TabCreated {
  id: TabId;
  openerId: TabId;
  url: string;
  popup: boolean;
}
