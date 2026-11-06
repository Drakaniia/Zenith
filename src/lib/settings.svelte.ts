// User settings: owned by the UI, persisted as JSON by the backend, and pushed to the backend
// wherever Rust needs them (appearance, shortcuts, blocker, downloads).
import { api } from "./ipc";
import { defaultShortcuts } from "./shortcuts";

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
  searchEngine: "google" | "yandex";
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
  shortcuts: Record<string, string[]>;
  siteZoom: Record<string, number>;
  restoreSession: boolean;
  showTopSites: boolean;
  newTabsAtTop: boolean;
  onboarded: boolean;
}

export type ToolbarMode = "single" | "multiple" | "collapsed";

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
    shortcuts: defaultShortcuts(),
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
    const bindings = Object.entries(this.value.shortcuts).map(
      ([action, keys]) => ({ action, keys }),
    );
    await api.setShortcuts(bindings);
  }

  enabledLists() {
    return this.value.filterLists
      .filter((l) => l.enabled)
      .map(({ id, url }) => ({ id, url }));
  }
}

export const settings = new SettingsStore();
