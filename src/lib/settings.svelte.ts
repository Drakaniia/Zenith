// User settings: owned by the UI, persisted as JSON by the backend, and pushed to the backend
// wherever Rust needs them (appearance, shortcuts, blocker, downloads).
import { api } from "./ipc";
import { defaultShortcuts } from "./shortcuts";
import { SEARCH_ENGINES } from "./url";

export interface FilterListSetting {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
  custom?: boolean;
}

export interface Settings {
  theme: "system" | "light" | "dark";
  material: "mica" | "mica-alt" | "acrylic" | "solid";
  contentScheme: "system" | "light" | "dark";
  compact: boolean;
  /** Zen's layouts: the URL bar in the sidebar (single toolbar), a toolbar above the page
   *  (multiple toolbars), or an icon-only sidebar with the toolbar above the page (collapsed). */
  toolbarMode: ToolbarMode;
  /** Layout to return to when the collapsed sidebar is expanded again. */
  lastExpandedMode?: Exclude<ToolbarMode, "collapsed">;
  sidebarWidth: number;
  /** Zen's "Clear" button between pinned and regular tabs. */
  showClearButton: boolean;
  /** The "sidebar hidden, Ctrl+B brings it back" hint was shown once. */
  compactHintSeen?: boolean;
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
    material: "mica",
    contentScheme: "system",
    compact: false,
    toolbarMode: "single",
    sidebarWidth: SIDEBAR_DEFAULT,
    showClearButton: true,
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

  async load() {
    const media = matchMedia("(prefers-color-scheme: dark)");
    this.systemDark = media.matches;
    media.addEventListener("change", (e) => {
      this.systemDark = e.matches;
      void this.push();
    });
    try {
      const raw = await api.settingsLoad();
      if (raw) {
        const saved = JSON.parse(raw) as Partial<Settings>;
        const base = defaultSettings();
        // Keep defaults for new actions; respect user edits (including cleared bindings).
        const shortcuts = { ...base.shortcuts, ...(saved.shortcuts ?? {}) };
        const lists = [...base.filterLists];
        for (const l of saved.filterLists ?? []) {
          const i = lists.findIndex((x) => x.id === l.id);
          if (i >= 0) lists[i] = { ...lists[i], enabled: l.enabled };
          else if (l.custom) lists.push(l);
        }
        this.value = { ...base, ...saved, shortcuts, filterLists: lists };
        this.value.sidebarWidth = Math.min(
          SIDEBAR_MAX,
          Math.max(SIDEBAR_MIN, this.value.sidebarWidth || SIDEBAR_DEFAULT),
        );
        if (
          !["single", "multiple", "collapsed"].includes(this.value.toolbarMode)
        )
          this.value.toolbarMode = "single";
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
      () => void api.settingsSave(JSON.stringify(this.value)),
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

  async push() {
    const s = this.value;
    const root = document.documentElement;
    root.dataset.theme = this.resolvedTheme;
    root.dataset.material = s.material;
    const applied = await api.applySettings(
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
        material: s.material,
        pageBackground: this.pageBackground(),
        contentScheme: s.contentScheme,
      },
    );
    // Windows 10 has no Mica: the backend reports the solid fallback it applied.
    root.dataset.material = applied.material;
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
    this.globalShortcutConflicts = (await api.setShortcuts(bindings)) ?? [];
  }

  enabledLists() {
    return this.value.filterLists
      .filter((l) => l.enabled)
      .map(({ id, url }) => ({ id, url }));
  }
}

export const settings = new SettingsStore();
