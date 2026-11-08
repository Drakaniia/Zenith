// The palette's result list: gathers rows from commands, open tabs, bookmarks, history and
// search suggestions for what was typed. Plain logic, no markup; Palette.svelte owns the state.
import { browser } from '../browser/browser.svelte';
import { bookmarks } from '../bookmarks/bookmarks.svelte';
import { settings } from '../settings/settings.svelte';
import { keywordMatch, nodeLabel, parseRestriction, searchBookmarks, searchTagged } from '../bookmarks/bookmarks';
import { commands } from './commands';
import { faviconFor } from '../../shared/favicons';
import { scoreAny } from './fuzzy';
import { normalizeInput, prettyUrl, searchUrl, titleFor } from '../../shared/url';
import { popularCandidates, type Autofill, type AutofillCandidate } from './autocomplete';
import type { PaletteMode } from '../../shared/ui/ui.svelte';
import type { HistoryEntry } from '../history/types';
import type { IconName } from '../../shared/ui/icons';

export type How = 'enter' | 'newTab' | 'glance';
export interface Item {
  key: string;
  kind: 'go' | 'search' | 'tab' | 'bookmark' | 'history' | 'suggest' | 'command';
  title: string;
  subtitle?: string;
  url?: string;
  icon?: IconName;
  favicon?: string;
  run: (how: How) => void;
}

/** The palette state the result list is built from. */
export interface SourceContext {
  commandMode: boolean;
  mode: PaletteMode;
  q: string;
  restrict: ReturnType<typeof parseRestriction>;
  fill: Autofill | null;
  history: HistoryEntry[];
  suggestions: string[];
  engine: { name: string };
  go: (url: string, how: How) => void;
  close: (restoreFocus?: boolean) => void;
}

const POPULAR = popularCandidates();

export function paletteItems(ctx: SourceContext): Item[] {
  const { commandMode, mode, q, restrict, fill, history, suggestions, engine, go, close } = ctx;
  if (commandMode) {
    return commands()
      .map((c) => ({ c, s: q ? scoreAny(q, c.label, c.keywords) : 1 }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 14)
      .map(({ c }) => ({
        key: `c:${c.id}`,
        kind: 'command' as const,
        title: c.label,
        icon: c.icon,
        run: () => {
          close(false);
          c.run();
        },
      }));
  }
  // Zen's "Focus Search" (Ctrl+K): whatever is typed is searched with the default engine.
  if (mode === 'search') {
    if (!q) return [];
    const url = searchUrl(q, settings.value.searchEngine);
    const out: Item[] = [{ key: 'go', kind: 'search', title: q, subtitle: `Search ${engine.name}`, icon: 'search', run: (h) => go(url, h) }];
    for (const s of suggestions.slice(0, 8)) {
      if (s.toLowerCase() === q.toLowerCase()) continue;
      const su = searchUrl(s, settings.value.searchEngine);
      out.push({ key: `s:${s}`, kind: 'suggest', title: s, icon: 'search', run: (h) => go(su, h) });
    }
    return out;
  }
  // Firefox's restriction tokens: only bookmarks (*), tagged bookmarks (+), history (^) or tabs (%).
  const only = restrict.restriction;
  if (only === 'bookmarks' || only === 'tagged') {
    const hits = only === 'tagged' ? searchTagged(bookmarks.index, restrict.rest) : searchBookmarks(bookmarks.index, restrict.rest, { limit: 12 });
    return hits.map(
      (b): Item => ({
        key: `b:${b.id}`,
        kind: 'bookmark',
        title: nodeLabel(b),
        subtitle: only === 'tagged' ? b.tags.join(', ') : prettyUrl(b.url),
        url: b.url,
        icon: 'bookmarkStar',
        favicon: faviconFor(b.url),
        run: (h) => go(b.url, h),
      }),
    );
  }
  if (only === 'history') {
    return history.map(
      (h): Item => ({ key: `h:${h.id}`, kind: 'history', title: h.title || prettyUrl(h.url), subtitle: prettyUrl(h.url), url: h.url, icon: 'history', favicon: faviconFor(h.url), run: (how) => go(h.url, how) }),
    );
  }
  if (only === 'tabs') {
    const open = Object.values(browser.tabs).filter((t) => t.id !== browser.glance?.tabId && t.url && !t.url.startsWith('zenith://newtab'));
    const hits = restrict.rest ? open.map((t) => ({ t, s: scoreAny(restrict.rest, t.title, t.url) })).filter((x) => x.s > 0).sort((a, b) => b.s - a.s).map((x) => x.t) : [...open].sort((a, b) => b.lastActive - a.lastActive);
    return hits.slice(0, 12).map(
      (t): Item => ({
        key: `t:${t.id}`,
        kind: 'tab',
        title: titleFor(t.url, t.title),
        subtitle: prettyUrl(t.url),
        favicon: t.favicon,
        url: t.url,
        run: () => {
          close(false);
          browser.activate(t.id);
        },
      }),
    );
  }
  const out: Item[] = [];
  const seen = new Set<string>();
  // A bookmark's keyword typed alone (or with the words for its %s address) opens it first.
  const kw = q ? keywordMatch(bookmarks.index, q) : null;
  if (kw) {
    seen.add(kw.url);
    out.push({
      key: `k:${kw.node.id}`,
      kind: 'bookmark',
      title: nodeLabel(kw.node),
      subtitle: prettyUrl(kw.url),
      url: kw.url,
      icon: 'bookmarkStar',
      favicon: faviconFor(kw.url),
      run: (h) => go(kw.url, h),
    });
  }
  if (q) {
    // An inline completion leads; what was typed, taken literally, comes right after it.
    const auto = fill;
    if (auto) {
      out.push({ key: 'go', kind: 'go', title: prettyUrl(auto.url), subtitle: 'Open address', icon: 'globe', url: auto.url, favicon: faviconFor(auto.url), run: (h) => go(auto.url, h) });
      seen.add(auto.url);
    }
    const key = auto ? 'typed' : 'go';
    const target = normalizeInput(q, settings.value.searchEngine);
    const isSearch = target === searchUrl(q, settings.value.searchEngine);
    out.push(
      isSearch
        ? { key, kind: 'search', title: q, subtitle: `Search ${engine.name}`, icon: 'search', run: (h) => go(target, h) }
        : { key, kind: 'go', title: prettyUrl(target), subtitle: 'Open address', icon: 'globe', url: target, favicon: faviconFor(target), run: (h) => go(target, h) },
    );
    seen.add(target);
  }
  const currentId = mode === 'navigate' ? browser.activeTab?.id : undefined;
  const tabs = Object.values(browser.tabs).filter((t) => t.id !== browser.glance?.tabId && t.id !== currentId);
  const tabHits = q
    ? tabs
        .map((t) => ({ t, s: scoreAny(q, t.title, t.url) }))
        .filter((x) => x.s > 0)
        .sort((a, b) => b.s - a.s)
        .slice(0, 4)
        .map((x) => x.t)
    : [...tabs].sort((a, b) => b.lastActive - a.lastActive).slice(0, 5);
  for (const t of tabHits) {
    seen.add(t.url);
    out.push({
      key: `t:${t.id}`,
      kind: 'tab',
      title: titleFor(t.url, t.title),
      subtitle: prettyUrl(t.url),
      favicon: t.favicon,
      url: t.url,
      run: () => {
        close(false);
        browser.activate(t.id);
      },
    });
  }
  if (q) {
    // Bookmarks match on their title, address, tags and keyword.
    let shown = 0;
    for (const b of searchBookmarks(bookmarks.index, q)) {
      if (shown >= 4) break;
      if (seen.has(b.url)) continue;
      seen.add(b.url);
      shown++;
      out.push({ key: `b:${b.id}`, kind: 'bookmark', title: nodeLabel(b), subtitle: prettyUrl(b.url), url: b.url, icon: 'bookmarkStar', favicon: faviconFor(b.url), run: (h) => go(b.url, h) });
    }
  }
  for (const s of suggestions.slice(0, 4)) {
    if (s.toLowerCase() === q.toLowerCase()) continue;
    const url = searchUrl(s, settings.value.searchEngine);
    out.push({ key: `s:${s}`, kind: 'suggest', title: s, icon: 'search', run: (h) => go(url, h) });
  }
  for (const h of history) {
    if (seen.has(h.url)) continue;
    seen.add(h.url);
    out.push({ key: `h:${h.id}`, kind: 'history', title: h.title || prettyUrl(h.url), subtitle: prettyUrl(h.url), url: h.url, icon: 'history', favicon: faviconFor(h.url), run: (how) => go(h.url, how) });
  }
  if (q.length >= 2) {
    for (const c of commands()
      .map((c) => ({ c, s: scoreAny(q, c.label) }))
      .filter((x) => x.s >= 900)
      .slice(0, 2)) {
      out.push({ key: `c:${c.c.id}`, kind: 'command', title: c.c.label, icon: c.c.icon, subtitle: 'Command', run: () => { close(false); c.c.run(); } });
    }
  }
  return out;
}

/** Everything that can complete an address, the user's own browsing weighted above the bundled sites. */
export function autofillCandidates(history: HistoryEntry[]): AutofillCandidate[] {
  const out: AutofillCandidate[] = [];
  for (const h of history) out.push({ url: h.url, weight: 10 + Math.min(h.visitCount, 50) });
  for (const url of bookmarks.urls) out.push({ url, weight: 8 });
  for (const t of Object.values(browser.tabs)) out.push({ url: t.url, weight: 4 });
  return out.concat(POPULAR);
}
