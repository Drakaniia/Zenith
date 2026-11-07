import { displayHost, isInternal } from "./url";

/**
 * Remembers the favicon each site last showed, keyed by host, so lists that only know an
 * address (history, bookmarks) can still show the site's logo. Kept in localStorage when
 * the runtime has one.
 */
const KEY = "zenith.favicons";
const MAX = 300;

const cache = new Map<string, string>();
let loaded = false;

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

function load() {
  if (loaded) return;
  loaded = true;
  try {
    const raw = storage()?.getItem(KEY);
    const list: unknown = raw ? JSON.parse(raw) : [];
    if (!Array.isArray(list)) return;
    for (const e of list) {
      if (Array.isArray(e) && typeof e[0] === "string" && typeof e[1] === "string") cache.set(e[0], e[1]);
    }
  } catch {
    /* A corrupt cache is just an empty one. */
  }
}

let timer: ReturnType<typeof setTimeout> | undefined;
function persist() {
  const store = storage();
  if (!store) return;
  clearTimeout(timer);
  timer = setTimeout(() => {
    try {
      store.setItem(KEY, JSON.stringify([...cache]));
    } catch {
      /* Quota exceeded: keep the in-memory copy. */
    }
  }, 1000);
}

function siteKey(url: string): string {
  if (!url || isInternal(url) || !/^https?:/i.test(url)) return "";
  const host = displayHost(url);
  return host === url ? "" : host;
}

export function rememberFavicon(url: string, favicon: string | undefined) {
  const key = siteKey(url);
  if (!key || !favicon) return;
  load();
  if (cache.get(key) === favicon) return;
  cache.delete(key);
  cache.set(key, favicon);
  if (cache.size > MAX) cache.delete(cache.keys().next().value as string);
  persist();
}

const asked = new Set<string>();

/**
 * Fetches the icons of sites that have none remembered yet (bookmarks that were imported, or whose
 * site was never visited here), a few at a time, each site once per run. Resolves with how many
 * icons arrived, so the caller can redraw.
 */
export async function fetchMissingFavicons(
  urls: Iterable<string>,
  fetchIcon: (url: string) => Promise<string | null>,
  limit = 80,
): Promise<number> {
  const todo: string[] = [];
  for (const url of urls) {
    const key = siteKey(url);
    if (!key || asked.has(key) || faviconFor(url)) continue;
    asked.add(key);
    todo.push(url);
    if (todo.length >= limit) break;
  }
  let got = 0;
  const worker = async () => {
    for (let url = todo.pop(); url; url = todo.pop()) {
      try {
        const icon = await fetchIcon(url);
        // Big files would crowd the small stored cache out.
        if (icon && icon.length <= 40_000) {
          rememberFavicon(url, icon);
          got++;
        }
      } catch {
        /* No icon: the letter tile stays. */
      }
    }
  };
  await Promise.all([worker(), worker(), worker(), worker()]);
  return got;
}

export function faviconFor(url: string | undefined): string | undefined {
  if (!url) return undefined;
  const key = siteKey(url);
  if (!key) return undefined;
  load();
  return cache.get(key);
}
