// Typed bridge to the Rust backend. Only the chrome webview can call these commands.
import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type {
  BlockerStats,
  BookmarkBackup,
  BookmarkNode,
  BookmarkPatch,
  DownloadItem,
  ExtensionInfo,
  ExtensionUpdate,
  HistoryEntry,
  InstallEvent,
  ImportResult,
  ImportSource,
  MediaReport,
  MemoryStats,
  NewBookmarkNode,
  OpenRequest,
  PermissionRequest,
  ProfileInfo,
  ProfilesView,
  StartupInfo,
  TabCreated,
  TabPatch,
  UpdateStatus,
} from "./types";
import type { Note } from "./notes";
import type { PasswordSaveOffer, PasswordSuggest, VaultStatus } from "./passwords";

export interface PaneSpec {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  /** During a layout animation the webview keeps its largest size and only this much of it, from
   *  its top-left, shows (see `layoutHold` in layout.svelte.ts). */
  visW?: number;
  visH?: number;
}

export interface LayoutSpec {
  panes: PaneSpec[];
  glance: PaneSpec | null;
  overlay: boolean;
  viewportW: number;
  viewportH: number;
}

export interface RuntimeConfig {
  downloadDir: string | null;
  recordHistory: boolean;
  blockPopups: boolean;
  blockerEnabled: boolean;
  mediaButton: boolean;
  useFfmpeg: boolean;
  ffmpegPath: string | null;
  autoUpdate: boolean;
  /** Password manager and autofill (see settings.svelte.ts). */
  passwordsEnabled: boolean;
  passwordsOfferSave: boolean;
  autofillSuggest: boolean;
  autofillUsernames: boolean;
  autofillPasswords: boolean;
  autofillOnLoad: boolean;
  autofillSameSite: boolean;
  passwordGenerator: boolean;
  vaultAskEveryTime: boolean;
  vaultLockMinutes: number;
}

export interface Appearance {
  theme: string;
  material: string;
  pageBackground: [number, number, number];
  contentScheme: string;
}

export const api = {
  uiReady: () => invoke<StartupInfo>("ui_ready"),
  quit: () => invoke<void>("app_quit"),
  /** Closes this window (call after saving the session); the last window quits Zenith. */
  windowClose: () => invoke<void>("window_close"),
  /** Opens a new incognito window (same process, InPrivate pages), optionally loading pages. */
  incognitoOpen: (urls: string[] = []) =>
    invoke<void>("incognito_open", { urls }),
  /** Opens another normal window. `session` (this window's snapshot) gives it the same spaces,
   *  essentials and pinned tabs; its regular tabs are not copied. */
  windowOpen: (session: string | null) => invoke<void>("window_open", { session }),
  /** Reopens the most recently closed normal window; false when there is none. */
  windowUndoClose: () => invoke<boolean>("window_undo_close"),
  /** Called once by the main window at startup: reopens the other windows that were open when
   *  Zenith quit (`keep`), or forgets them. */
  windowsRestore: (keep: boolean) => invoke<void>("windows_restore", { keep }),
  /** Opens the developer tools of this window's own UI (Firefox's Browser Toolbox). */
  chromeDevtools: () => invoke<void>("chrome_devtools"),
  /** A local file chosen in the system dialog (Ctrl+O), or null. */
  pickFile: () => invoke<string | null>("pick_file"),

  tabCreate: (args: {
    id: string;
    url?: string | null;
    glance?: boolean;
    muted?: boolean;
    zoom?: number;
    scrollX?: number;
    scrollY?: number;
    /** An extension popup's opener page (what its `tabs.query` should find). */
    openerUrl?: string | null;
  }) => invoke<void>("tab_create", { args }),
  tabClose: (id: string) => invoke<void>("tab_close", { id }),
  tabNavigate: (id: string, url: string) =>
    invoke<void>("tab_navigate", { id, url }),
  tabCommand: <T = unknown>(id: string, cmd: string, arg?: unknown) =>
    invoke<T>("tab_command", { id, cmd, arg: arg ?? null }),
  tabSleep: (id: string) =>
    invoke<{ x: number; y: number } | null>("tab_sleep", { id }),
  tabSetGlance: (id: string, glance: boolean) =>
    invoke<void>("tab_set_glance", { id, glance }),
  tabContentSize: (id: string) =>
    invoke<{ x: number; y: number } | null>("tab_content_size", { id }),
  setLayout: (spec: LayoutSpec) => invoke<void>("set_layout", { spec }),
  focusView: (target: string | null) => invoke<void>("focus_view", { target }),
  find: (
    id: string,
    op: "start" | "next" | "prev" | "stop",
    text = "",
    caseSensitive = false,
    wholeWord = false,
  ) => invoke<void>("find_in_page", { id, op, text, caseSensitive, wholeWord }),
  permissionRespond: (requestId: number, allow: boolean, remember: boolean) =>
    invoke<void>("permission_respond", { requestId, allow, remember }),

  /** Applies shortcut bindings; `global` ones also work while another app has the focus.
   *  Returns the global shortcuts that could not be registered (another app owns them). */
  setShortcuts: (
    bindings: { action: string; keys: string[]; global?: boolean }[],
  ) => invoke<string[] | null>("set_shortcuts", { bindings }),
  applySettings: (config: RuntimeConfig, appearance: Appearance) =>
    invoke<{ material: string; fallback?: string | null }>("apply_settings", { config, appearance }),
  clearBrowsingData: (
    kinds: string[],
    sinceMs: number | null,
    history: boolean,
  ) => invoke<void>("clear_browsing_data", { kinds, sinceMs, history }),

  /** This window's saved tabs. `keep` says whether to restore its regular tabs too: null leaves
   *  it to the "restore previous session" setting, false keeps only spaces, essentials and pinned
   *  tabs (a new window), true restores everything (a reopened window). `fresh` asks for new tab
   *  ids, as the tabs came from another window. */
  sessionLoad: () =>
    invoke<{ data: string | null; keep: boolean | null; fresh: boolean }>("session_load"),
  sessionSave: (data: string) => invoke<void>("session_save", { data }),
  settingsLoad: () => invoke<string | null>("settings_load"),
  settingsSave: (data: string) => invoke<void>("settings_save", { data }),
  /** Start-page backgrounds the user uploaded, stored as data URLs. */
  wallpaperSave: (id: string, data: string) => invoke<void>("wallpaper_save", { id, data }),
  wallpaperLoad: (id: string) => invoke<string | null>("wallpaper_load", { id }),
  wallpaperDelete: (id: string) => invoke<void>("wallpaper_delete", { id }),

  historySearch: (text: string, limit = 50, offset = 0) =>
    invoke<HistoryEntry[]>("history_search", { text, limit, offset }),
  historyTop: (limit = 40) => invoke<HistoryEntry[]>("history_top", { limit }),
  historyDelete: (ids: number[]) => invoke<number>("history_delete", { ids }),
  historyClear: (sinceMs: number | null) =>
    invoke<number>("history_clear", { sinceMs }),
  historyPrune: (days: number) => invoke<void>("history_prune", { days }),

  /** Every node of the bookmark tree: the three roots first, then by parent and position. */
  bookmarksList: () => invoke<BookmarkNode[]>("bookmarks_list"),
  /** Adds nodes (folders may carry children) under `parent` at `index` (null: at the end);
   *  returns the ids of the new top-level nodes. */
  bookmarksAdd: (parent: number, index: number | null, nodes: NewBookmarkNode[]) =>
    invoke<number[]>("bookmarks_add", { parent, index, nodes }),
  bookmarkUpdate: (id: number, patch: BookmarkPatch) =>
    invoke<void>("bookmark_update", { id, patch }),
  /** `index` counts the destination's children after the node has left its old place. */
  bookmarkMove: (id: number, parent: number, index: number) =>
    invoke<void>("bookmark_move", { id, parent, index }),
  bookmarksReorder: (parent: number, order: number[]) =>
    invoke<void>("bookmarks_reorder", { parent, order }),
  bookmarksRemove: (ids: number[]) => invoke<void>("bookmarks_remove", { ids }),
  bookmarksRemoveUrl: (url: string) =>
    invoke<void>("bookmarks_remove_url", { url }),
  /** Netscape HTML (Firefox and Chrome read it); null when the dialog is cancelled. */
  bookmarksExport: () => invoke<string | null>("bookmarks_export"),
  /** JSON backup of the whole tree; null when the dialog is cancelled. */
  bookmarksBackup: () => invoke<string | null>("bookmarks_backup"),
  /** Replaces every bookmark with a backup file's; null when the dialog is cancelled. */
  bookmarksRestore: () => invoke<number | null>("bookmarks_restore"),
  bookmarkBackups: () => invoke<BookmarkBackup[]>("bookmark_backups"),
  bookmarksRestoreBackup: (name: string) =>
    invoke<number>("bookmarks_restore_backup", { name }),
  /** A site's `/favicon.ico` as a data URL (fetched by the backend; the page CSP allows no remote images). */
  faviconFetch: (url: string) => invoke<string | null>("favicon_fetch", { url }),
  /** Writes today's automatic backup (once a day). */
  bookmarksAutoBackup: () => invoke<void>("bookmarks_auto_backup"),

  notesList: () => invoke<Note[]>("notes_list"),
  /** Saves a note; false when a newer copy (from another window) is already stored. */
  noteSave: (note: Note) => invoke<boolean>("note_save", { note }),
  noteDelete: (id: string) => invoke<void>("note_delete", { id }),
  bookmarksImport: () => invoke<number>("bookmarks_import"),
  /** Other browsers' profiles on this PC with bookmarks or history to import. */
  importSources: () => invoke<ImportSource[]>("import_sources"),
  importRun: (
    source: string,
    bookmarks: boolean,
    history: boolean,
    historyDays: number,
  ) =>
    invoke<ImportResult>("import_run", {
      source,
      bookmarks,
      history,
      historyDays,
    }),

  downloadsActive: () => invoke<DownloadItem[]>("downloads_active"),
  downloadsHistory: (limit = 200) =>
    invoke<DownloadItem[]>("downloads_history", { limit }),
  downloadAction: (
    id: number,
    action: "pause" | "resume" | "cancel" | "stop",
  ) => invoke<void>("download_action", { id, action }),
  downloadsRemove: (id: number | null) =>
    invoke<void>("downloads_remove", { id }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  showInFolder: (path: string) => invoke<void>("show_in_folder", { path }),
  pickFolder: (title?: string) =>
    invoke<string | null>("pick_folder", { title: title ?? null }),

  mediaList: (tabId: string, scan = true) =>
    invoke<MediaReport>("media_list", { tabId, scan }),
  mediaDownload: (
    tabId: string,
    itemId: number,
    variant: string | null,
    audio: string | null,
  ) => invoke<number>("media_download", { tabId, itemId, variant, audio }),
  mediaTools: () => invoke<{ ffmpeg: string | null }>("media_tools"),
  mediaPickFfmpeg: () => invoke<string | null>("media_pick_ffmpeg"),

  blockerConfigure: (
    lists: { id: string; url: string }[],
    update: boolean,
    disabledSites: string[],
  ) =>
    invoke<BlockerStats>("blocker_configure", { lists, update, disabledSites }),
  blockerSetDisabledSites: (sites: string[]) =>
    invoke<void>("blocker_set_disabled_sites", { sites }),
  blockerStats: () => invoke<BlockerStats>("blocker_stats"),

  extList: () => invoke<ExtensionInfo[]>("ext_list"),
  extInstallFolder: () =>
    invoke<ExtensionInfo | null>("ext_install_folder", { path: null }),
  extInstallCrx: () => invoke<ExtensionInfo | null>("ext_install_crx"),
  extInstallStore: (input: string) =>
    invoke<ExtensionInfo>("ext_install_store", { input }),
  /** Installs an extension the user accepted in the install prompt. */
  extOfferAccept: (token: number) =>
    invoke<ExtensionInfo>("ext_offer_accept", { token }),
  extOfferDiscard: (token: number) =>
    invoke<void>("ext_offer_discard", { token }),
  extCheckUpdates: () => invoke<ExtensionUpdate[]>("ext_check_updates"),
  extUpdate: (id: string) => invoke<ExtensionInfo>("ext_update", { id }),
  extRemove: (id: string) => invoke<void>("ext_remove", { id }),
  extSetEnabled: (id: string, enabled: boolean) =>
    invoke<void>("ext_set_enabled", { id, enabled }),

  defaultBrowserStatus: () =>
    invoke<{ registered: boolean; isDefault: boolean }>(
      "default_browser_status",
    ),
  defaultBrowserRegister: (openSettings = true) =>
    invoke<void>("default_browser_register", { openSettings }),
  defaultBrowserUnregister: () => invoke<void>("default_browser_unregister"),
  searchSuggest: (engine: string, query: string) =>
    invoke<string[]>("search_suggest", { engine, query }),
  memoryStats: () => invoke<MemoryStats>("memory_stats"),
  systemMemoryLoad: () => invoke<number>("system_memory_load"),
  clipboardWrite: (text: string) => invoke<void>("clipboard_write", { text }),

  updateStatus: () => invoke<UpdateStatus>("update_status"),
  updateCheck: () => invoke<UpdateStatus>("update_check"),
  /** Quits into the downloaded update (save the session first); the installer reopens Zenith. */
  updateRestart: () => invoke<void>("update_restart"),

  /** Every browser profile, which are open, and the colours to offer. */
  profilesList: () => invoke<ProfilesView>("profiles_list"),
  /**
   * Adds a profile (and, with `shortcut`, a desktop shortcut for it). `avatar` is `image`,
   * `emoji:<name>` or null (the initial); `icon` is the picture rendered as a 256 px PNG
   * (base64 or a data URL) for the emoji and image kinds.
   */
  profileCreate: (name: string, color: string, avatar: string | null, icon: string | null, shortcut: boolean) =>
    invoke<ProfileInfo>("profile_create", { name, color, avatar, icon, shortcut }),
  profileUpdate: (id: string, name: string, color: string, avatar: string | null, icon: string | null) =>
    invoke<ProfileInfo>("profile_update", { id, name, color, avatar, icon }),
  /** Opens a profile in its own window (or brings its open window to the front). */
  profileOpen: (id: string) => invoke<void>("profile_open", { id }),
  /** Deletes a profile that isn't open, with all of its data. */
  profileDelete: (id: string) => invoke<void>("profile_delete", { id }),
  /** Creates the profile's desktop shortcut; returns its path. */
  profileShortcut: (id: string) => invoke<string>("profile_shortcut", { id }),
};

export interface BackendEvents {
  "tab-update": TabPatch;
  "tab-created": TabCreated;
  "tab-open-request": OpenRequest;
  "tab-close-request": { id: string };
  "tab-focused": { id: string };
  "popup-blocked": { tabId: string; url: string };
  shortcut: { action: string; repeat: boolean; source: string | null };
  "permission-request": PermissionRequest;
  download: DownloadItem;
  "find-result": { id: string; matches: number; active: number };
  "open-urls": string[];
  "app-close-requested": null;
  "media-changed": { tabId: string; count: number };
  "media-request": { tabId: string; hint: string };
  "update-status": UpdateStatus;
  "ext-install": InstallEvent;
  "vault-status": VaultStatus;
  "password-suggest": PasswordSuggest;
  "password-suggest-close": { tabId: string };
  "password-save-offer": PasswordSaveOffer;
  "notes-changed": { origin: string; id: string; deleted: boolean };
  /** Another window changed the bookmarks. */
  "bookmarks-changed": { origin: string };
  /** Another normal window saved the settings. */
  "settings-changed": { origin: string };
  /** Windows started or stopped allowing the window material (transparency effects, battery
   *  saver): re-apply the appearance. */
  "translucency-changed": null;
}

export function on<K extends keyof BackendEvents>(
  event: K,
  handler: (payload: BackendEvents[K]) => void,
): Promise<UnlistenFn> {
  // Window-scoped: the backend sends each window its own tab, shortcut and close events (and
  // app-wide ones to all). The global `listen` would also receive the other windows' events.
  return getCurrentWebviewWindow().listen<BackendEvents[K]>(event, (e) =>
    handler(e.payload),
  );
}
