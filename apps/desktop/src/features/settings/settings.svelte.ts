// User settings: owned by the UI, persisted as JSON by the backend, and pushed to the backend
// wherever Rust needs them (appearance, shortcuts, blocker, downloads).
import { settingsApi } from "./api";
import { shortcutsApi } from "$features/shortcuts/api";
import { fadeChange } from "$shared/ui/fade";
import { defaultShortcuts, mergeShortcuts } from "$features/shortcuts/shortcuts";
import { SEARCH_ENGINES } from "$shared/url";
import { defaultStartPage, normalizeStartPage, type StartPageSettings } from "$features/startpage/startpage";
import { GALLERY_IDS } from "$features/startpage/wallpapers";
import { effectiveMode, normalizeSidebarPosition, type SidebarPosition } from "$features/sidebar/sidebar-position";

export interface FilterListSetting {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
  custom?: boolean;
}

export interface Settings {
  theme: "system" | "light" | "dark";
  contentScheme: "system" | "light" | "dark";
  compact: boolean;
  /** Compact mode also hides the toolbar above the page; it floats in from the top edge (Zen's
   *  floating toolbar, toggled with Ctrl+Alt+T). */
  compactHideToolbar: boolean;
  /** The AI chatbot opened by Ctrl+Alt+X (a key of CHATBOTS); empty until one is picked. */
  chatbot: string;
  /** Zen's layouts: the URL bar in the sidebar (single toolbar), a toolbar above the page
   *  (multiple toolbars), or an icon-only sidebar with the toolbar above the page (collapsed). */
  toolbarMode: ToolbarMode;
  /** Layout to return to when the collapsed sidebar is expanded again. */
  lastExpandedMode?: Exclude<ToolbarMode, "collapsed">;
  sidebarWidth: number;
  /** The window edge the sidebar is on; top and bottom make it a horizontal tab strip. */
  sidebarPosition: SidebarPosition;
  /** Zen's "Clear" button between pinned and regular tabs. */
  showClearButton: boolean;
  /** The "sidebar hidden, compact mode brings it back" hint was shown once. */
  compactHintSeen?: boolean;
  /** Zen's bookmarks toolbar: always shown, only on a new tab, or never (Firefox's
   *  browser.toolbars.bookmarks.visibility). Ctrl+Shift+B switches between always and never. */
  bookmarksBar: BookmarksBarMode;
  /** Show "Other Bookmarks" at the end of the toolbar (when something is in it). */
  bookmarksOther: boolean;
  /** The bookmarks sidebar (Ctrl+B): open or not, its width, and what it lists. */
  bookmarksPanel: boolean;
  bookmarksPanelWidth: number;
  bookmarksPanelView: "bookmarks" | "history";
  /** Folder new bookmarks go into: the one picked last (the Bookmarks Toolbar at first). */
  lastBookmarkFolder: number;
  /** A key of SEARCH_ENGINES. */
  searchEngine: string;
  /** Page opened when a window starts without tabs, and by Enter in an empty new-tab bar.
   *  Empty: Zenith's own start page. */
  homepage: string;
  searchSuggestions: boolean;
  sleepMinutes: number;
  suspendHidden: boolean;
  sleepOnSpaceSwitch: boolean;
  memoryPressure: boolean;
  blockerEnabled: boolean;
  filterLists: FilterListSetting[];
  filterListsUpdated: number;
  blockerDisabledSites: string[];
  blockPopups: boolean;
  recordHistory: boolean;
  historyDays: number;
  downloadDir: string | null;
  /** Media downloader: button over videos in pages. */
  mediaButton: boolean;
  /** Use FFmpeg (when found) to convert .ts recordings and merge streams. */
  useFfmpeg: boolean;
  ffmpegPath: string | null;
  /** Download updates in the background and install them when Zenith closes. */
  autoUpdate: boolean;
  /** Extensions whose icons show next to the extensions button (WebView2 extension ids). */
  pinnedExtensions: string[];
  /** Check the Chrome Web Store for extension updates at startup. */
  extensionUpdates: boolean;
  /** When the startup extension update check last ran. */
  extensionUpdatesChecked?: number;
  shortcuts: Record<string, string[]>;
  /** The notes shortcut also works while another app has the focus (a system-wide hotkey). */
  notesGlobalShortcut: boolean;
  /** The notes editor's list of notes is shown (wide windows). */
  notesSidebar: boolean;
  /** The note text uses the whole width instead of a centred column. */
  notesWide: boolean;
  /** Password manager: saving, suggestions and filling. Off turns all of it off. */
  passwordsEnabled: boolean;
  /** Offer to save logins submitted on pages. */
  passwordsOfferSave: boolean;
  /** Offer to save cards submitted on payment forms. */
  cardsOfferSave: boolean;
  /** Show saved cards when a card field is clicked (filling always asks for the master password). */
  autofillCards: boolean;
  /** Show saved accounts when a login field is clicked. */
  autofillSuggest: boolean;
  /** Fill usernames and emails. */
  autofillUsernames: boolean;
  /** Fill passwords (off: usernames only). */
  autofillPasswords: boolean;
  /** Fill the only account saved for a site as soon as its login form appears (while unlocked). */
  autofillOnLoad: boolean;
  /** Offer credentials on other subdomains of the site they were saved on. */
  autofillSameSite: boolean;
  /** Suggest a strong generated password in sign-up forms. */
  passwordGenerator: boolean;
  /** Ask for the master password before every fill, reveal or copy. */
  vaultAskEveryTime: boolean;
  /** Lock the password manager after this many idle minutes (0: only when Zenith closes). */
  vaultLockMinutes: number;
  siteZoom: Record<string, number>;
  restoreSession: boolean;
  showTopSites: boolean;
  /** The start page's layout and background (customised from its settings button). */
  startPage: StartPageSettings;
  newTabsAtTop: boolean;
  /** The setup assistant was completed (or skipped with "Use the defaults"). Until then it
   *  opens on every launch of the normal window. */
  onboarded: boolean;
  /** When setup was completed. */
  onboardedAt?: number;
  /** Setup step to resume at if Zenith was closed during setup. */
  onboardingStep?: string;
}

export type ToolbarMode = "single" | "multiple" | "collapsed";
export type BookmarksBarMode = "always" | "newtab" | "never";

/** Width limits of the bookmarks sidebar. */
export const PANEL_MIN = 200;
export const PANEL_MAX = 520;
export const PANEL_DEFAULT = 260;

/** Actions whose shortcuts also work while another app has the focus. */
export const GLOBAL_ACTIONS = new Set(["openNotes"]);

/** Zen's sidebar width limits (expanded --zen-toolbox-min-width, zen.view.sidebar-expanded.max-width)
 *  and the width below which dragging the edge collapses the sidebar into compact mode. */
export const SIDEBAR_MIN = 163;
export const SIDEBAR_MAX = 500;
export const SIDEBAR_DEFAULT = 220;
export const SIDEBAR_COLLAPSE_AT = 60;

export const BUILTIN_LISTS: FilterListSetting[] = [
  {
    id: "easylist",
    name: "EasyList — ads",
    url: "https://easylist.to/easylist/easylist.txt",
    enabled: true,
  },
  {
    id: "easyprivacy",
    name: "EasyPrivacy — trackers",
    url: "https://easylist.to/easylist/easyprivacy.txt",
    enabled: true,
  },
  {
    id: "peterlowe",
    name: "Peter Lowe's ad and tracking servers",
    url: "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=adblockplus&showintro=0&mimetype=plaintext",
    enabled: false,
  },
];

export function defaultSettings(): Settings {
  return {
    theme: "system",
    contentScheme: "system",
    compact: false,
    compactHideToolbar: false,
    chatbot: "",
    toolbarMode: "single",
    sidebarWidth: SIDEBAR_DEFAULT,
    sidebarPosition: "left",
    showClearButton: true,
    bookmarksBar: "newtab",
    bookmarksOther: true,
    bookmarksPanel: false,
    bookmarksPanelWidth: PANEL_DEFAULT,
    bookmarksPanelView: "bookmarks",
    lastBookmarkFolder: 1,
    searchEngine: "google",
    homepage: "",
    searchSuggestions: false,
    sleepMinutes: 5,
    suspendHidden: true,
    sleepOnSpaceSwitch: false,
    memoryPressure: true,
    blockerEnabled: true,
    filterLists: BUILTIN_LISTS.map((l) => ({ ...l })),
    filterListsUpdated: 0,
    blockerDisabledSites: [],
    blockPopups: true,
    recordHistory: true,
    historyDays: 90,
    downloadDir: null,
    mediaButton: true,
    useFfmpeg: true,
    ffmpegPath: null,
    autoUpdate: true,
    pinnedExtensions: [],
    extensionUpdates: true,
    shortcuts: defaultShortcuts(),
    notesGlobalShortcut: true,
    notesSidebar: true,
    notesWide: false,
    passwordsEnabled: true,
    passwordsOfferSave: true,
    cardsOfferSave: true,
    autofillCards: true,
    autofillSuggest: true,
    autofillUsernames: true,
    autofillPasswords: true,
    autofillOnLoad: false,
    autofillSameSite: true,
    passwordGenerator: true,
    vaultAskEveryTime: false,
    vaultLockMinutes: 15,
    siteZoom: {},
    restoreSession: true,
    showTopSites: true,
    startPage: defaultStartPage(),
    newTabsAtTop: true,
    onboarded: false,
  };
}

class SettingsStore {
  value = $state<Settings>(defaultSettings());
  systemDark = $state(true);
  /** System-wide shortcuts another app already owns (they still work inside Zenith). */
  globalShortcutConflicts = $state<string[]>([]);
  #saveTimer: ReturnType<typeof setTimeout> | undefined;

  resolvedTheme = $derived<"light" | "dark">(
    this.value.theme === "system"
      ? this.systemDark
        ? "dark"
        : "light"
      : this.value.theme,
  );

  /** The layout actually drawn: a tab strip (sidebar at the top or bottom) keeps the toolbar. */
  layoutMode = $derived<ToolbarMode>(
    effectiveMode(this.value.toolbarMode, this.value.sidebarPosition),
  );

  async load() {
    const media = matchMedia("(prefers-color-scheme: dark)");
    this.systemDark = media.matches;
    media.addEventListener("change", (e) => {
      this.systemDark = e.matches;
      void this.push();
    });
    await this.#read();
  }

  /** Another window saved the settings: take its values. The backend already has them (that
   *  window pushed them), so only this window's own UI follows. */
  async reload() {
    clearTimeout(this.#saveTimer);
    await this.#read();
    const root = document.documentElement;
    root.dataset.theme = this.resolvedTheme;
  }

  async #read() {
    try {
      const raw = await settingsApi.settingsLoad();
      if (raw) {
        const saved = JSON.parse(raw) as Partial<Settings>;
        // The translucent sidebar was removed: drop its keys from settings saved before.
        Reflect.deleteProperty(saved, "translucentSidebar");
        Reflect.deleteProperty(saved, "material");
        const base = defaultSettings();
        // Keep defaults for new actions; respect user edits (including cleared bindings).
        const shortcuts = mergeShortcuts(saved.shortcuts, base.shortcuts);
        const lists = [...base.filterLists];
        for (const l of saved.filterLists ?? []) {
          const i = lists.findIndex((x) => x.id === l.id);
          if (i >= 0) lists[i] = { ...lists[i], enabled: l.enabled };
          else if (l.custom) lists.push(l);
        }
        this.value = {
          ...base,
          ...saved,
          shortcuts,
          filterLists: lists,
          startPage: normalizeStartPage(saved.startPage, GALLERY_IDS),
        };
        this.value.sidebarWidth = Math.min(
          SIDEBAR_MAX,
          Math.max(SIDEBAR_MIN, this.value.sidebarWidth || SIDEBAR_DEFAULT),
        );
        if (
          !["single", "multiple", "collapsed"].includes(this.value.toolbarMode)
        )
          this.value.toolbarMode = "single";
        this.value.sidebarPosition = normalizeSidebarPosition(this.value.sidebarPosition);
        if (!["always", "newtab", "never"].includes(this.value.bookmarksBar))
          this.value.bookmarksBar = "newtab";
        this.value.bookmarksPanelWidth = Math.min(
          PANEL_MAX,
          Math.max(PANEL_MIN, Number(this.value.bookmarksPanelWidth) || PANEL_DEFAULT),
        );
        if (this.value.bookmarksPanelView !== "history")
          this.value.bookmarksPanelView = "bookmarks";
        if (!Number.isInteger(this.value.lastBookmarkFolder))
          this.value.lastBookmarkFolder = 1;
        if (!SEARCH_ENGINES[this.value.searchEngine])
          this.value.searchEngine = "google";
        if (typeof this.value.homepage !== "string") this.value.homepage = "";
      }
    } catch (e) {
      console.warn("settings could not be read, using defaults", e);
    }
  }

  /** Persist (debounced) and push backend-relevant values. */
  save(push = true) {
    clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(
      () => void settingsApi.settingsSave(JSON.stringify(this.value)),
      300,
    );
    if (push) void this.push();
  }

  update(patch: Partial<Settings>) {
    Object.assign(this.value, patch);
    this.save();
  }

  pageBackground(): [number, number, number] {
    return this.resolvedTheme === "dark" ? [32, 32, 36] : [255, 255, 255];
  }

  /** What was last sent to the backend. Re-applying the theme and the page background repaints
   *  the whole window, so a change that touches neither (the bookmarks panel, a width) sends
   *  nothing. */
  #pushed = "";

  async push() {
    const s = this.value;
    const root = document.documentElement;
    const theme = this.resolvedTheme;
    // A change after startup fades; the first application (nothing set yet) is immediate.
    if (!root.dataset.theme) root.dataset.theme = theme;
    else if (root.dataset.theme !== theme) fadeChange(() => (root.dataset.theme = theme));
    const args: Parameters<typeof settingsApi.applySettings> = [
      {
        downloadDir: s.downloadDir,
        recordHistory: s.recordHistory,
        blockPopups: s.blockPopups,
        blockerEnabled: s.blockerEnabled,
        mediaButton: s.mediaButton,
        useFfmpeg: s.useFfmpeg,
        ffmpegPath: s.ffmpegPath,
        autoUpdate: s.autoUpdate,
        passwordsEnabled: s.passwordsEnabled,
        passwordsOfferSave: s.passwordsOfferSave,
        cardsOfferSave: s.cardsOfferSave,
        autofillCards: s.autofillCards,
        autofillSuggest: s.autofillSuggest,
        autofillUsernames: s.autofillUsernames,
        autofillPasswords: s.autofillPasswords,
        autofillOnLoad: s.autofillOnLoad,
        autofillSameSite: s.autofillSameSite,
        passwordGenerator: s.passwordGenerator,
        vaultAskEveryTime: s.vaultAskEveryTime,
        vaultLockMinutes: s.vaultLockMinutes,
      },
      {
        theme: s.theme,
        pageBackground: this.pageBackground(),
        contentScheme: s.contentScheme,
      },
    ];
    const key = JSON.stringify(args);
    if (key === this.#pushed) return;
    this.#pushed = key;
    try {
      await settingsApi.applySettings(...args);
    } catch (e) {
      if (this.#pushed === key) this.#pushed = ""; // sent again next time
      throw e;
    }
  }

  async pushShortcuts() {
    const global = this.value.notesGlobalShortcut;
    const bindings = Object.entries(this.value.shortcuts).map(
      ([action, keys]) => ({
        action,
        keys,
        global: global && GLOBAL_ACTIONS.has(action),
      }),
    );
    this.globalShortcutConflicts = (await shortcutsApi.setShortcuts(bindings)) ?? [];
  }

  enabledLists() {
    return this.value.filterLists
      .filter((l) => l.enabled)
      .map(({ id, url }) => ({ id, url }));
  }
}

export const settings = new SettingsStore();
