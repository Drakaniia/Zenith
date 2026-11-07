// Typed bridge to the Rust backend. Only the chrome webview can call these commands.
import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type {
  BlockerStats,
  Bookmark,
  DownloadItem,
  ExtensionInfo,
  ExtensionUpdate,
  HistoryEntry,
  InstallEvent,
  ImportResult,
  ImportSource,
  MediaReport,
  MemoryStats,
  OpenRequest,
  PermissionRequest,
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

  tabCreate: (args: {
    id: string;
    url?: string | null;
    glance?: boolean;
    muted?: boolean;
    zoom?: number;
    scrollX?: number;
    scrollY?: number;
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
    invoke<{ material: string }>("apply_settings", { config, appearance }),
  clearBrowsingData: (
    kinds: string[],
    sinceMs: number | null,
    history: boolean,
  ) => invoke<void>("clear_browsing_data", { kinds, sinceMs, history }),

  sessionLoad: () => invoke<string | null>("session_load"),
  sessionSave: (data: string) => invoke<void>("session_save", { data }),
  settingsLoad: () => invoke<string | null>("settings_load"),
  settingsSave: (data: string) => invoke<void>("settings_save", { data }),

  historySearch: (text: string, limit = 50, offset = 0) =>
    invoke<HistoryEntry[]>("history_search", { text, limit, offset }),
  historyTop: (limit = 40) => invoke<HistoryEntry[]>("history_top", { limit }),
  historyDelete: (ids: number[]) => invoke<number>("history_delete", { ids }),
  historyClear: (sinceMs: number | null) =>
    invoke<number>("history_clear", { sinceMs }),
  historyPrune: (days: number) => invoke<void>("history_prune", { days }),

  bookmarksList: () => invoke<Bookmark[]>("bookmarks_list"),
  bookmarkAdd: (bookmark: Partial<Bookmark> & { url: string }) =>
    invoke<number>("bookmark_add", {
      bookmark: {
        id: 0,
        title: "",
        folder: "",
        position: 0,
        created: 0,
        ...bookmark,
      },
    }),
  bookmarkUpdate: (bookmark: Bookmark) =>
    invoke<void>("bookmark_update", { bookmark }),
  bookmarkRemove: (id: number | null, url: string | null = null) =>
    invoke<void>("bookmark_remove", { id, url }),
  bookmarksExport: () => invoke<string | null>("bookmarks_export"),

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
