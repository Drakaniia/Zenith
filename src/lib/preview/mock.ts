// Development-only browser preview of the chrome UI (`pnpm dev`, then open http://127.0.0.1:1420).
// It stands in for the Rust backend with Tauri's own IPC mocks and a seeded session, so the shell
// can be checked visually without building the app. Page webviews do not exist here: live tabs
// show a placeholder page. Query parameters: ?mode=single|multiple|collapsed, ?theme=light|dark,
// ?compact=1, ?split=1, ?collapsedPins=1.
//
// main.ts only imports this module when running under Vite's dev server outside Tauri, so it
// never ships in a build.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";

const params = new URLSearchParams(location.search);

function favicon(letter: string, bg: string, fg = "#fff"): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="8" fill="${bg}"/><text x="16" y="22" font-family="Segoe UI, Arial" font-size="17" font-weight="700" text-anchor="middle" fill="${fg}">${letter}</text></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

interface Seed {
  id: string;
  url: string;
  title: string;
  fav: string;
  spaceId?: string;
  pinned?: boolean;
  essential?: boolean;
  audible?: boolean;
}

const seeds: Seed[] = [
  {
    id: "e1",
    url: "https://mail.example.com/",
    title: "Inbox (3)",
    fav: favicon("M", "#d93025"),
    essential: true,
  },
  {
    id: "e2",
    url: "https://calendar.example.com/",
    title: "Calendar",
    fav: favicon("C", "#1a73e8"),
    essential: true,
  },
  {
    id: "e3",
    url: "https://chat.example.com/",
    title: "Chat",
    fav: favicon("#", "#611f69"),
    essential: true,
  },
  {
    id: "e4",
    url: "https://music.example.com/",
    title: "Music",
    fav: favicon("♪", "#1db954"),
    essential: true,
    audible: true,
  },
  {
    id: "p1",
    url: "https://github.com/zen-browser/desktop",
    title: "zen-browser/desktop",
    fav: favicon("G", "#24292f"),
    spaceId: "home",
    pinned: true,
  },
  {
    id: "p2",
    url: "https://docs.example.com/guide",
    title: "Design guide",
    fav: favicon("D", "#e8710a"),
    spaceId: "home",
    pinned: true,
  },
  {
    id: "p3",
    url: "https://news.example.com/",
    title: "News — front page",
    fav: favicon("N", "#ff6600"),
    spaceId: "home",
    pinned: true,
  },
  {
    id: "t1",
    url: "https://developer.mozilla.org/en-US/docs/Web/CSS/corner-shape",
    title: "corner-shape - CSS | MDN",
    fav: favicon("M", "#000"),
    spaceId: "home",
  },
  {
    id: "t2",
    url: "https://svelte.dev/docs/svelte/transition",
    title: "transition: • Svelte Docs",
    fav: favicon("S", "#ff3e00"),
    spaceId: "home",
  },
  {
    id: "t3",
    url: "https://tauri.app/",
    title: "Tauri 2.0",
    fav: favicon("T", "#24c8db"),
    spaceId: "home",
  },
  {
    id: "t4",
    url: "https://www.wikipedia.org/",
    title: "Wikipedia, the free encyclopedia",
    fav: favicon("W", "#636466"),
    spaceId: "home",
  },
  {
    id: "t5",
    url: "https://example.com/",
    title: "Example Domain",
    fav: favicon("E", "#5b8def"),
    spaceId: "home",
  },
  {
    id: "w1",
    url: "https://tracker.example.com/board",
    title: "Sprint board",
    fav: favicon("J", "#0052cc"),
    spaceId: "work",
    pinned: true,
  },
  {
    id: "w2",
    url: "https://figma.example.com/file",
    title: "Shell redesign",
    fav: favicon("F", "#a259ff"),
    spaceId: "work",
  },
  {
    id: "w3",
    url: "https://meet.example.com/",
    title: "Standup",
    fav: favicon("V", "#00897b"),
    spaceId: "work",
  },
  {
    id: "r1",
    url: "https://read.example.com/a",
    title: "On squircles",
    fav: favicon("R", "#b5651d"),
    spaceId: "read",
  },
];
const byId = new Map(seeds.map((s) => [s.id, s]));

function session(): string {
  const split = params.get("split") === "1";
  const spaces = [
    {
      id: "home",
      name: "Home",
      icon: "🏠",
      color: "#8b7cf6",
      pinnedIds: ["p1", "p2", "p3"],
      tabIds: ["t1", "t2", "t3", "t4", "t5"],
      folders: [
        { id: "f1", name: "Reading", collapsed: false, tabIds: ["p2", "p3"] },
      ],
      pinnedCollapsed: params.get("collapsedPins") === "1",
      activeTabId: "t2",
      split: split ? { ids: ["t2", "t3"], ratio: 0.5 } : undefined,
    },
    {
      id: "work",
      name: "Work",
      icon: "💼",
      color: "#2bb5a8",
      pinnedIds: ["w1"],
      tabIds: ["w2", "w3"],
      activeTabId: "w2",
    },
    {
      id: "read",
      name: "Reading list",
      icon: "📚",
      color: "#e0a83a",
      pinnedIds: [],
      tabIds: ["r1"],
      activeTabId: "r1",
    },
  ];
  const tabs = seeds.map((s) => ({
    id: s.id,
    url: s.url,
    title: s.title,
    favicon: s.fav,
    spaceId: s.spaceId ?? "",
    pinned: !!s.pinned,
    essential: !!s.essential,
    pinnedUrl:
      s.pinned || s.essential
        ? s.id === "p3"
          ? "https://news.example.com/top"
          : s.url
        : undefined,
    lastActive: Date.now(),
    zoom: 1,
    muted: false,
  }));
  return JSON.stringify({
    v: 1,
    activeSpaceId: "home",
    essentials: ["e1", "e2", "e3", "e4"],
    spaces,
    tabs,
    closed: [],
  });
}

function settingsJson(): string {
  const mode = params.get("mode");
  return JSON.stringify({
    onboarded: true,
    theme: params.get("theme") ?? "dark",
    material: "solid",
    compact: params.get("compact") === "1",
    toolbarMode: mode === "multiple" || mode === "collapsed" ? mode : "single",
    sidebarWidth: Number(params.get("width")) || undefined,
  });
}

/** Simulates the backend's reports for a newly created page webview. */
function fakeLoad(id: string) {
  const seed = byId.get(id);
  setTimeout(() => {
    void emit("tab-update", {
      id,
      loading: false,
      title: seed?.title,
      favicon: seed?.fav,
      canGoBack: true,
      blocked: 12,
      audible: !!seed?.audible,
    });
  }, 350);
}

export function installPreviewMock() {
  document.documentElement.dataset.preview = "true";
  const style = document.createElement("style");
  // A stand-in page under each live tab's pane, since there are no native page views here.
  style.textContent = `
    html[data-preview] .web:not(.waiting) {
      background:
        linear-gradient(var(--page-bg), var(--page-bg)) padding-box,
        var(--page-bg);
      background-image:
        linear-gradient(180deg, color-mix(in srgb, var(--text) 6%, transparent) 0 64px, transparent 64px),
        repeating-linear-gradient(180deg, transparent 0 28px, color-mix(in srgb, var(--text) 5%, transparent) 28px 40px);
      background-size: 100% 100%, 60% 100%;
      background-position: 0 0, 40px 110px;
      background-repeat: no-repeat;
    }`;
  document.head.append(style);

  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "settings_load":
          return settingsJson();
        case "session_load":
          return session();
        case "apply_settings":
          return { material: "solid" };
        case "ui_ready":
          return {
            testProfile: false,
            incognito: new URLSearchParams(location.search).has("incognito"),
            startupUrls: [],
            windows11: true,
            webviewVersion: "preview",
            version: "0.0.0-preview",
            dataDir: "",
            defaultDownloadDir: "",
          };
        case "tab_create":
          fakeLoad((a.args as { id: string }).id);
          return null;
        case "tab_sleep":
          return null;
        case "history_search":
        case "history_top":
        case "bookmarks_list":
        case "downloads_active":
        case "downloads_history":
        case "ext_list":
        case "search_suggest":
          return [];
        case "blocker_configure":
        case "blocker_stats":
          return { rules: 93000, totalBlocked: 0, lists: [] };
        case "update_status":
          return { state: "idle", currentVersion: "0.0.0-preview" };
        case "media_list":
          return { items: [], blobs: 0, drm: 0, ffmpeg: false };
        case "plugin:window|is_maximized":
        case "plugin:window|is_fullscreen":
          return false;
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
