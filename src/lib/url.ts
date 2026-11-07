// Address-bar input handling and URL display helpers.

export const NEW_TAB_URL = "zenith://newtab";

export const INTERNAL_PAGES = [
  "newtab",
  "settings",
  "history",
  "bookmarks",
  "downloads",
  "extensions",
  "notes",
  "passwords",
] as const;
export type InternalPage = (typeof INTERNAL_PAGES)[number];

export interface SearchEngine {
  name: string;
  url: string;
  home: string;
  /** One line about what sets it apart, for the setup assistant. */
  blurb: string;
}

/** Search engines in the order they are offered. The backend's suggestion endpoints
 *  (net.rs `suggest_endpoint`) use the same keys. */
export const SEARCH_ENGINES: Record<string, SearchEngine> = {
  google: {
    name: "Google",
    url: "https://www.google.com/search?q=%s",
    home: "https://www.google.com/",
    blurb: "The largest index, and the results most people expect.",
  },
  bing: {
    name: "Bing",
    url: "https://www.bing.com/search?q=%s",
    home: "https://www.bing.com/",
    blurb: "Microsoft's search, with Copilot answers alongside.",
  },
  duckduckgo: {
    name: "DuckDuckGo",
    url: "https://duckduckgo.com/?q=%s",
    home: "https://duckduckgo.com/",
    blurb: "Doesn't keep a profile of you or your searches.",
  },
  brave: {
    name: "Brave Search",
    url: "https://search.brave.com/search?q=%s",
    home: "https://search.brave.com/",
    blurb: "Its own independent index, without tracking.",
  },
  ecosia: {
    name: "Ecosia",
    url: "https://www.ecosia.org/search?q=%s",
    home: "https://www.ecosia.org/",
    blurb: "Spends its profits on planting trees.",
  },
  yandex: {
    name: "Yandex",
    url: "https://yandex.com/search/?text=%s",
    home: "https://yandex.com/",
    blurb: "Strongest for Russian-language results.",
  },
};

const LONG_TLDS = new Set(
  (
    "com net org edu gov mil int info biz name pro aero coop museum mobi asia jobs travel tel cat post " +
    "io dev app ai xyz tech site online store blog news cloud design page link live life world today space " +
    "website art studio agency digital media network systems solutions software email shop club fun top vip " +
    "win wiki zone guru ninja rocks social chat game games team tools codes center company works plus run " +
    "one icu best moe lol pub pics photos video tube music radio fm wtf help support academy school university " +
    "local localhost test example internal lan home corp"
  ).split(" "),
);

export function isInternal(url: string | undefined | null): boolean {
  return !!url && url.startsWith("zenith://");
}

export function internalPage(url: string): InternalPage | null {
  if (!isInternal(url)) return null;
  const name = url.slice("zenith://".length).split(/[/?#]/)[0] as InternalPage;
  return (INTERNAL_PAGES as readonly string[]).includes(name) ? name : null;
}

export function isNewTab(url: string | undefined | null): boolean {
  return !url || url === NEW_TAB_URL || url === "about:blank";
}

function looksLikeHost(host: string): boolean {
  if (!host || host.length > 253) return false;
  const lower = host.toLowerCase();
  if (lower === "localhost") return true;
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(lower)) return true;
  if (/^\[[0-9a-f:]+\]$/.test(lower)) return true;
  if (!/^[a-z0-9\u00a1-\uffff-]+(\.[a-z0-9\u00a1-\uffff-]+)+$/.test(lower))
    return false;
  const tld = lower.slice(lower.lastIndexOf(".") + 1);
  if (tld.startsWith("xn--")) return true;
  if (/^[a-z]{2}$/.test(tld)) return true;
  return LONG_TLDS.has(tld);
}

/** Turns address-bar input into a URL: a URL as typed, a bare host, a local path or a search. */
export function normalizeInput(raw: string, engine: string): string {
  const input = raw.trim();
  if (!input) return NEW_TAB_URL;
  if (input.startsWith("?")) return searchUrl(input.slice(1).trim(), engine);

  if (/^zenith:\/\//i.test(input)) {
    return internalPage(input.toLowerCase())
      ? input.toLowerCase()
      : NEW_TAB_URL;
  }
  if (
    /^(https?|file|about|view-source|chrome-extension):/i.test(input) &&
    !/\s/.test(input)
  ) {
    try {
      return new URL(input).href;
    } catch {
      /* fall through to search */
    }
  }
  if (/^[a-zA-Z]:[\\/]/.test(input) || input.startsWith("\\\\")) {
    return "file:///" + input.replace(/\\/g, "/").replace(/^\/+/, "");
  }
  if (!/\s/.test(input)) {
    const hostPart = input.split(/[/?#]/)[0];
    const [host, port] = hostPart.split(/:(?=\d+$)/);
    if (looksLikeHost(host) && (port === undefined || /^\d{1,5}$/.test(port))) {
      const scheme =
        host === "localhost" || /^\d{1,3}(\.\d{1,3}){3}$/.test(host)
          ? "http"
          : "https";
      try {
        return new URL(`${scheme}://${input}`).href;
      } catch {
        /* search */
      }
    }
  }
  return searchUrl(input, engine);
}

export function searchUrl(query: string, engine: string): string {
  const e = SEARCH_ENGINES[engine] ?? SEARCH_ENGINES.google;
  return e.url.replace("%s", encodeURIComponent(query));
}

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

/** "www.example.com" -> "example.com" */
export function displayHost(url: string): string {
  if (isInternal(url)) return "Zenith";
  const host = hostOf(url);
  return host.replace(/^www\./, "") || url;
}

/** URL without scheme and trailing slash, for compact display. */
export function prettyUrl(url: string): string {
  if (isInternal(url)) return url;
  return url
    .replace(/^https?:\/\//, "")
    .replace(/^www\./, "")
    .replace(/\/$/, "");
}

export function isSecure(url: string): boolean {
  return url.startsWith("https://");
}

export function isWebUrl(url: string | undefined | null): boolean {
  return !!url && /^https?:\/\//.test(url);
}

/** Approximate registrable domain, mirroring the backend's `site_of`. */
export function siteOf(host: string): string {
  const parts = host.toLowerCase().replace(/\.$/, "").split(".");
  if (parts.length <= 2 || /^\d+$/.test(parts[parts.length - 1]))
    return parts.join(".");
  const second = [
    "co",
    "com",
    "net",
    "org",
    "gov",
    "edu",
    "ac",
    "or",
    "ne",
    "go",
    "gob",
    "mil",
    "nic",
  ];
  const n =
    parts[parts.length - 1].length === 2 &&
    second.includes(parts[parts.length - 2])
      ? 3
      : 2;
  return parts.slice(-n).join(".");
}

export function titleFor(url: string, title: string): string {
  if (title) return title;
  const page = internalPage(url);
  if (page) {
    return {
      newtab: "New Tab",
      settings: "Settings",
      history: "History",
      bookmarks: "Bookmarks",
      downloads: "Downloads",
      extensions: "Extensions",
      notes: "Notes",
      passwords: "Passwords",
    }[page];
  }
  if (isNewTab(url)) return "New Tab";
  return prettyUrl(url) || "Untitled";
}
