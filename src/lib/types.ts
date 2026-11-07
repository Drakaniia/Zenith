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

export interface SplitState {
  ids: [TabId, TabId];
  ratio: number;
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

export interface Bookmark {
  id: number;
  url: string;
  title: string;
  folder: string;
  position: number;
  created: number;
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

/** An extension found in a page (a .crx download, "Add to Zenith" on the Web Store),
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

export interface StartupInfo {
  testProfile: boolean;
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
