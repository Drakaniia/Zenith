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

export interface PermissionRequest {
  requestId: number;
  tabId: TabId;
  origin: string;
  kind: string;
  userInitiated: boolean;
}

export interface DownloadItem {
  id: number;
  url: string;
  path: string;
  mime: string;
  total: number;
  received: number;
  state: "progress" | "completed" | "interrupted" | "canceled";
  started: number;
  ended?: number | null;
  fileName?: string;
  paused?: boolean;
  canResume?: boolean;
  tabId?: string;
  reason?: string;
  /** "media" for the media downloader's jobs, "file" for ordinary downloads. */
  kind?: "file" | "media";
  /** Progress detail of a media job ("Segment 12 of 80", "Recording 3:05") or a note. */
  detail?: string;
  /** A live recording: it can be stopped (keeping what was recorded) but not paused. */
  live?: boolean;
}

/** A quality or audio-language choice offered for a stream. */
export interface MediaChoice {
  id: string;
  label: string;
  height?: number | null;
  bandwidth?: number | null;
  audioOnly: boolean;
}

/** A video or audio item found on a page. */
export interface MediaItem {
  id: number;
  kind: "file" | "hls" | "dash";
  url: string;
  title: string;
  ext: string;
  size?: number | null;
  duration?: number | null;
  width?: number | null;
  height?: number | null;
  audioOnly: boolean;
  live: boolean;
  protected: boolean;
  foundBy: "network" | "page";
  detected: number;
  variants: MediaChoice[];
  audioTracks: MediaChoice[];
  note: string;
}

/** App update state, as reported by the backend (`update-status`). */
export interface UpdateStatus {
  state: "idle" | "checking" | "downloading" | "ready" | "upToDate" | "error";
  currentVersion: string;
  version?: string | null;
  notes?: string | null;
  progress?: number | null;
  error?: string | null;
  checked?: number | null;
}

export interface MediaReport {
  items: MediaItem[];
  /** Players streaming through Media Source (blob: sources). */
  blobs: number;
  /** Players using DRM (Encrypted Media Extensions). */
  drm: number;
  ffmpeg: boolean;
}

export interface HistoryEntry {
  id: number;
  url: string;
  title: string;
  visitCount: number;
  lastVisit: number;
}

export type BookmarkKind = "bookmark" | "folder" | "separator";

/** One node of the bookmark tree (Firefox's Places model, as in Zen): a bookmark, a folder or
 *  a separator. Three built-in folders are the roots (see `ROOT_*` in bookmarks.ts). */
export interface BookmarkNode {
  id: number;
  /** Null only for the three built-in roots. */
  parent: number | null;
  kind: BookmarkKind;
  title: string;
  /** Empty for folders and separators. */
  url: string;
  /** Contiguous 0..n-1 among the siblings. */
  position: number;
  created: number;
  modified: number;
  tags: string[];
  /** Typing it in the address bar opens the bookmark. */
  keyword: string;
  /** Workspaces the bookmark is shown in on the toolbar (Zen: workspace bookmarks). Empty: all. */
  spaces: string[];
}

/** A node to create; folders may carry children. */
export interface NewBookmarkNode {
  kind: BookmarkKind;
  title?: string;
  url?: string;
  tags?: string[];
  keyword?: string;
  spaces?: string[];
  children?: NewBookmarkNode[];
}

/** Fields to change on a node; omitted ones stay as they are. */
export interface BookmarkPatch {
  title?: string;
  url?: string;
  tags?: string[];
  keyword?: string;
  spaces?: string[];
}

/** An automatic or pre-restore backup file of the bookmarks. */
export interface BookmarkBackup {
  name: string;
  created: number;
  count: number;
}

/** Another browser's profile that can be imported (`import_sources`). */
export interface ImportSource {
  id: string;
  /** "chrome", "edge", "brave", "vivaldi", "chromium", "opera", "operagx", "firefox", "zen" */
  browser: string;
  name: string;
  /** Empty when the browser has a single profile folder. */
  profile: string;
  bookmarks: boolean;
  history: boolean;
}

export interface ImportResult {
  /** New bookmarks; ones already present are skipped. */
  bookmarks: number;
  /** History entries merged. */
  history: number;
  errors: string[];
}

export interface ExtensionInfo {
  id: string;
  name: string;
  enabled: boolean;
  version: string;
  description: string;
  optionsUrl?: string | null;
  popupUrl?: string | null;
  icon?: string | null;
  builtin: boolean;
  /** Chrome Web Store id, for extensions installed from the store. */
  storeId?: string | null;
}

/** An extension found in a page (a .crx download, "Add to Chrome" on the Web Store),
 *  staged and waiting for the user to accept or decline it. */
export interface InstallOffer {
  token: number;
  name: string;
  version: string;
  description: string;
  icon?: string | null;
  /** Chrome-style permission warnings. */
  warnings: string[];
  /** Installed version this would replace. */
  replaces?: string | null;
  storeId?: string | null;
}

export interface InstallEvent {
  tabId: string;
  state: "fetching" | "ready" | "failed";
  /** "Chrome Web Store" or the site's host name. */
  source: string;
  offer?: InstallOffer;
  error?: string;
}

export interface ExtensionUpdate {
  id: string;
  storeId: string;
  name: string;
  current: string;
  latest: string;
}

/** A browser profile (see profiles.rs). Each one runs in its own Zenith process. */
export interface ProfileInfo {
  id: string;
  name: string;
  /** `#rrggbb`. */
  color: string;
  isDefault: boolean;
  /** `image`, `emoji:<name>`, or absent for the initial on the colour. */
  avatar?: string | null;
  /** The chosen picture as a `data:image/png` URL (only for the `image` avatar). */
  image?: string | null;
}

export interface ProfileEntry extends ProfileInfo {
  /** A Zenith process has it open (the calling one included). */
  running: boolean;
  /** Zenith's desktop shortcut for it exists. */
  shortcut: boolean;
}

export interface ProfilesView {
  /** The calling window's profile. */
  active: string;
  profiles: ProfileEntry[];
  /** Colours offered for profiles. */
  colors: string[];
}

export interface StartupInfo {
  testProfile: boolean;
  /** The browser profile this window belongs to. */
  profile: ProfileInfo;
  /** An incognito window: nothing it does is saved. */
  incognito: boolean;
  startupUrls: string[];
  windows11: boolean;
  webviewVersion: string;
  version: string;
  dataDir: string;
  defaultDownloadDir: string;
}

export interface MemoryStats {
  privateBytes: number;
  workingSet: number;
  systemLoad: number;
  processes: {
    pid: number;
    name: string;
    privateBytes: number;
    workingSet: number;
  }[];
}

export interface BlockerStats {
  rules: number;
  totalBlocked: number;
  lists: {
    id: string;
    rules: number;
    updated?: number | null;
    error?: string | null;
  }[];
}
