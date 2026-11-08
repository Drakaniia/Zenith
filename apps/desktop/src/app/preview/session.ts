// Preview window session and startup: the seeded tabs and spaces, settings, the setup assistant's
// import, ui_ready and page webviews.
import { emit } from "@tauri-apps/api/event";
import { favicon, params } from "./fixtures";
import { UNHANDLED } from "./handler";
import { previewProfiles } from "./profiles";

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
  // ?page=passwords/settings opens an internal page in the active tab.
  const page = params.get("page");
  const tabs = seeds.map((s) => ({
    id: s.id,
    url: page && s.id === "t2" ? `zenith://${page}` : s.url,
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

const firstRun = params.has("fre");

function settingsJson(): string {
  const mode = params.get("mode");
  if (firstRun) return "";
  return JSON.stringify({
    onboarded: true,
    bookmarksBar: params.get("bar") ?? undefined,
    bookmarksPanel: params.get("panel") === "1",
    theme: params.get("theme") ?? "dark",
    compact: params.get("compact") === "1",
    toolbarMode: mode === "multiple" || mode === "collapsed" ? mode : "single",
    sidebarWidth: Number(params.get("width")) || undefined,
    sidebarPosition: params.get("side") ?? undefined,
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

/** Settings, session, import, ui_ready and tab commands. */
export function handleSession(cmd: string, a: Record<string, unknown>): unknown {
  switch (cmd) {
    case "settings_load":
      return settingsJson();
    case "session_load":
      return firstRun ? null : session();
    case "import_sources":
      return new Promise((r) =>
        setTimeout(
          () =>
            r(
              params.get("fre") === "import"
                ? []
                : [
                    { id: "chrome:Default", browser: "chrome", name: "Google Chrome", profile: "Person 1", bookmarks: true, history: true },
                    { id: "chrome:Profile 1", browser: "chrome", name: "Google Chrome", profile: "Work", bookmarks: true, history: true },
                    { id: "edge:Default", browser: "edge", name: "Microsoft Edge", profile: "Profile 1", bookmarks: true, history: true },
                    { id: "firefox:ab12.default-release", browser: "firefox", name: "Firefox", profile: "default-release", bookmarks: true, history: true },
                  ],
            ),
          600,
        ),
      );
    case "import_run":
      return new Promise((r) =>
        setTimeout(() => r({ bookmarks: a.bookmarks ? 214 : 0, history: a.history ? 1820 : 0, errors: [] }), 900),
      );
    case "set_shortcuts":
      return [];
    case "default_browser_status":
      return { registered: false, isDefault: false };
    case "apply_settings":
      return null;
    case "ui_ready":
      return {
        testProfile: false,
        profile: { ...previewProfiles[0] },
        incognito: new URLSearchParams(location.search).has("incognito"),
        startupUrls: [],
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
    default:
      return UNHANDLED;
  }
}
