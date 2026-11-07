<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import Favicon from './Favicon.svelte';
  import { browser } from '../lib/browser.svelte';
  import { bookmarks } from '../lib/bookmarks.svelte';
  import { settings } from '../lib/settings.svelte';
  import { rectOf, ui, type PaletteMode } from '../lib/ui.svelte';
  import { bookmarkPage } from '../lib/bookmark-actions';
  import { keywordMatch, nodeLabel, parseRestriction, searchBookmarks, searchTagged } from '../lib/bookmarks';
  import { api } from '../lib/ipc';
  import { focusAfterLayout, overlayLease } from '../lib/layout.svelte';
  import { commands } from '../lib/actions';
  import { faviconFor, rememberFavicon } from '../lib/favicons';
  import { scoreAny } from '../lib/fuzzy';
  import { NEW_TAB_URL, SEARCH_ENGINES, isWebUrl, normalizeInput, prettyUrl, searchUrl, titleFor } from '../lib/url';
  import { autofill, popularCandidates, type Autofill, type AutofillCandidate } from '../lib/autocomplete';
  import type { HistoryEntry } from '../lib/types';
  import type { IconName } from '../lib/icons';

  type How = 'enter' | 'newTab' | 'glance';
  interface Item {
    key: string;
    kind: 'go' | 'search' | 'tab' | 'bookmark' | 'history' | 'suggest' | 'command';
    title: string;
    subtitle?: string;
    url?: string;
    icon?: IconName;
    favicon?: string;
    run: (how: How) => void;
  }

  let input: HTMLInputElement | undefined = $state();
  let list: HTMLDivElement | undefined = $state();
  let query = $state('');
  let selected = $state(0);
  let history = $state<HistoryEntry[]>([]);
  let suggestions = $state<string[]>([]);
  let mode = $state<PaletteMode>('navigate');
  // `query` is what the user typed and drives the result list. The input can also show an
  // inline completion after it (`fill`, the added part selected) or the address of a result
  // picked with Tab or the arrow keys (`preview`); neither changes the list.
  let fill = $state<Autofill | null>(null);
  let preview = $state<string | null>(null);
  /** The last edit typed a character at the end, so an inline completion may be offered. */
  let armed = false;
  const POPULAR = popularCandidates();

  const open = $derived(ui.palette.open);

  $effect(() => {
    if (!open) return;
    query = ui.palette.query;
    mode = ui.palette.mode;
    // Read only the palette's initial state here: reading `q` would make this effect re-run
    // (and reset the input) on every keystroke.
    selected = ui.palette.query.trim() || ui.palette.mode === 'command' ? 0 : -1;
    history = [];
    suggestions = [];
    fill = null;
    preview = null;
    armed = false;
    for (const t of browser.closed) rememberFavicon(t.url, t.favicon);
    for (const t of Object.values(browser.tabs)) rememberFavicon(t.url, t.favicon);
    void tick().then(() => {
      input?.focus();
      if (ui.palette.selectAll) input?.select();
    });
  });

  const commandMode = $derived(mode === 'command' || query.startsWith('>'));
  const q = $derived(commandMode ? query.replace(/^>\s*/, '').trim() : query.trim());
  const engine = $derived(SEARCH_ENGINES[settings.value.searchEngine] ?? SEARCH_ENGINES.google);
  /** A lone *, +, ^ or % limits the results to bookmarks, tagged bookmarks, history or open tabs. */
  const restrict = $derived(commandMode ? { restriction: null, rest: q } : parseRestriction(q));

  let seq = 0;
  let historyTimer: ReturnType<typeof setTimeout> | undefined;
  let suggestTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const restricted = restrict.restriction;
    const text = restricted ? restrict.rest : q;
    const cmd = commandMode;
    clearTimeout(historyTimer);
    clearTimeout(suggestTimer);
    if (!open || cmd) {
      history = [];
      suggestions = [];
      return;
    }
    const mine = ++seq;
    historyTimer = setTimeout(async () => {
      const r = await api.historySearch(text, restricted === 'history' ? 14 : 7).catch(() => []);
      if (mine !== seq) return;
      history = r;
      refreshFill();
    }, text ? 40 : 0);
    const looksLikeUrl = !/\s/.test(text) && text.includes('.');
    if (settings.value.searchSuggestions && text.length > 1 && !looksLikeUrl && !restricted) {
      suggestTimer = setTimeout(async () => {
        const r = await api.searchSuggest(settings.value.searchEngine, text).catch(() => []);
        if (mine === seq) suggestions = r;
      }, 150);
    } else suggestions = [];
  });

  // With nothing typed, the list only offers recent tabs: nothing is preselected, so Enter
  // opens a new tab (new-tab mode) or just closes the palette.
  function initialSelection() {
    return q || commandMode ? 0 : -1;
  }
  $effect(() => {
    selected = initialSelection();
  });

  function go(url: string, how: How) {
    const m = mode;
    close(false);
    const tab = browser.activeTab;
    if (how === 'glance') browser.openGlance(url, tab?.id);
    else if (how === 'newTab' || m === 'newTab' || !tab) browser.openTab(url);
    else browser.navigate(tab, url);
  }

  function close(restoreFocus = true) {
    ui.closePalette();
    if (restoreFocus) focusAfterLayout(browser.activeTab?.live ? browser.activeTab.id : null);
  }

  const items = $derived.by<Item[]>(() => {
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
  });

  async function scrollSelected() {
    await tick();
    list?.querySelector('.item.sel')?.scrollIntoView({ block: 'nearest' });
  }

  /** Everything that can complete an address, the user's own browsing weighted above the bundled sites. */
  function candidates(): AutofillCandidate[] {
    const out: AutofillCandidate[] = [];
    for (const h of history) out.push({ url: h.url, weight: 10 + Math.min(h.visitCount, 50) });
    for (const url of bookmarks.urls) out.push({ url, weight: 8 });
    for (const t of Object.values(browser.tabs)) out.push({ url: t.url, weight: 4 });
    return out.concat(POPULAR);
  }

  /** Offers (or updates) the inline completion, but only while the caret is where typing left it. */
  function refreshFill() {
    const el = input;
    if (!el || !armed || preview !== null || commandMode || document.activeElement !== el) return;
    const caret = fill ? query.length : el.value.length;
    if (el.selectionStart !== caret || el.selectionEnd !== el.value.length) return;
    fill = autofill(query, candidates());
  }

  function oninput(e: Event) {
    const el = e.currentTarget as HTMLInputElement;
    const ev = e as InputEvent;
    query = el.value;
    preview = null;
    fill = null;
    // Deleting, pasting or editing mid-text never completes (Backspace removes a completion).
    armed = ev.inputType === 'insertText' && !ev.isComposing && el.selectionEnd === el.value.length;
    refreshFill();
  }

  // Mirror the typed text, completion or preview into the input. The value is only rewritten
  // when it differs, so ordinary typing keeps the caret where the user put it.
  const display = $derived(preview ?? fill?.text ?? query);
  $effect(() => {
    const el = input;
    const text = display;
    const from = preview === null && fill ? query.length : text.length;
    if (!el || el.value === text) return;
    el.value = text;
    el.setSelectionRange(from, text.length);
  });

  /** What the input shows while a result is picked from the keyboard; null keeps the typed text. */
  function previewOf(it: Item): string | null {
    if (it.key === 'go' || it.kind === 'command') return null;
    if (it.key === 'typed') return query;
    return it.url ?? it.title;
  }

  function pick(i: number) {
    selected = i;
    const it = items[i];
    preview = it ? previewOf(it) : null;
    armed = false;
    void scrollSelected();
  }

  function onkeydown(e: KeyboardEvent) {
    const n = items.length;
    if (e.key === 'ArrowDown' && n) {
      e.preventDefault();
      pick((selected + 1) % n);
    } else if (e.key === 'ArrowUp' && n) {
      e.preventDefault();
      pick(selected < 0 ? n - 1 : (selected - 1 + n) % n);
    } else if (e.key === 'Tab' && n) {
      // Tab walks the results (Shift+Tab back), writing each one's address into the bar.
      e.preventDefault();
      pick(e.shiftKey ? (selected <= 0 ? n - 1 : selected - 1) : (selected + 1) % n);
    } else if (
      (e.key === 'ArrowRight' || e.key === 'End') &&
      fill &&
      preview === null &&
      input &&
      input.selectionStart !== input.selectionEnd &&
      input.selectionEnd === input.value.length
    ) {
      // Accept the completion as typed text; the default action moves the caret to the end.
      query = fill.text;
      fill = null;
      armed = false;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const it = items[selected];
      if (it) it.run(e.altKey ? 'newTab' : e.shiftKey ? 'glance' : 'enter');
      else if (!q && mode === 'newTab') {
        close(false);
        browser.openTab(settings.value.homepage || NEW_TAB_URL);
      } else if (!q) close();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      // First Escape drops a completion or preview and goes back to what was typed.
      if (fill || preview !== null) {
        fill = null;
        preview = null;
        armed = false;
        selected = initialSelection();
      } else close();
    } else if (e.key === 'Backspace' && commandMode && query === '' && mode === 'command') {
      mode = 'navigate';
    }
  }

  // Zen's bookmark button at the end of the URL bar: only while editing the current page's address.
  const pageTab = $derived(mode === 'navigate' && !commandMode ? browser.activeTab : undefined);
  const pageUrl = $derived(pageTab && isWebUrl(pageTab.url) ? pageTab.url : '');
  const starred = $derived(bookmarks.has(pageUrl));

  /** The button bookmarks the page and hangs the "Edit bookmark" panel from itself; the bar closes. */
  function bookmarkFromBar(e: MouseEvent) {
    if (!pageTab || !pageUrl) return;
    const anchor = rectOf(e.currentTarget as HTMLElement);
    close(false);
    void bookmarkPage(pageTab, anchor);
  }

  const placeholder = $derived(
    commandMode ? 'Type a command…' : mode === 'newTab' ? 'Search or enter address' : 'Search or enter address',
  );
</script>

{#if open}
  <div class="root" use:overlayLease>
    <div class="backdrop" transition:fade={{ duration: 120 }} role="presentation" onpointerdown={() => close()}></div>
    <div
      class="palette"
      class:searchmode={commandMode}
      role="dialog"
      aria-label="Address bar and commands"
      transition:scale={{ start: 0.97, duration: 150, opacity: 0 }}
    >
      <div class="field">
        <span class="lead"><Icon name={commandMode ? 'command' : 'search'} size={18} /></span>
        <input
          bind:this={input}
          {oninput}
          {onkeydown}
          {placeholder}
          spellcheck="false"
          autocomplete="off"
          aria-label={placeholder}
        />
        {#if pageUrl}
          <button
            class="star"
            class:on={starred}
            data-action="bookmark"
            title={starred ? 'Edit bookmark' : 'Bookmark this page'}
            aria-label={starred ? 'Edit bookmark' : 'Bookmark this page'}
            aria-pressed={starred}
            tabindex="-1"
            onmousedown={(e) => e.preventDefault()}
            onclick={bookmarkFromBar}
          >
            <Icon name={starred ? 'bookmarkStar' : 'bookmark'} size={16} />
          </button>
        {:else if mode === 'newTab' && !commandMode}
          <span class="chip">New tab</span>
        {:else if commandMode}
          <span class="chip">Commands</span>
        {/if}
      </div>
      {#if items.length}
        <div class="results" role="listbox" bind:this={list}>
          {#each items as it, i (it.key)}
            <button
              class="item"
              class:sel={i === selected}
              role="option"
              aria-selected={i === selected}
              onmousemove={() => {
                if (selected === i) return;
                selected = i;
                preview = null;
              }}
              onclick={(e) => it.run(e.altKey ? 'newTab' : e.shiftKey ? 'glance' : 'enter')}
            >
              <span class="item-icon">
                {#if it.kind === 'tab' || it.favicon}
                  <Favicon url={it.url ?? ''} favicon={it.favicon} />
                {:else}
                  <Icon name={it.icon ?? 'globe'} size={16} />
                {/if}
                {#if it.kind === 'bookmark' || (it.url && bookmarks.has(it.url))}
                  <span class="badge" title="Bookmarked"><Icon name="star" size={9} fill stroke={1.5} /></span>
                {/if}
              </span>
              <span class="item-text">
                <span class="item-title ellipsis">{it.title}</span>
                {#if it.subtitle}<span class="item-sub ellipsis">{it.subtitle}</span>{/if}
              </span>
              {#if it.kind === 'tab'}
                <span class="tag">Switch to tab</span>
              {:else if i === selected && it.kind !== 'command'}
                <span class="tag">↵</span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
      <div class="hints">
        <span><span class="kbd">↵</span> open</span>
        <span><span class="kbd">Tab</span> next</span>
        <span><span class="kbd">Alt ↵</span> new tab</span>
        <span><span class="kbd">Shift ↵</span> Glance</span>
        <span><span class="kbd">*</span> bookmarks <span class="kbd">+</span> tags <span class="kbd">^</span> history <span class="kbd">%</span> tabs</span>
        <span><span class="kbd">&gt;</span> commands</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 80;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    background: var(--backdrop);
  }
  /* Zen's floating URL bar (#urlbar[zen-floating-urlbar]): centred near the top, 52px input,
   * 45px result rows, tinted by the space colour. */
  .palette {
    position: absolute;
    left: 50%;
    top: 18%;
    width: min(720px, calc(100vw - 48px));
    transform: translateX(-50%);
    padding: 3px;
    border-radius: 16px;
    corner-shape: var(--sq);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, var(--accent) 4%, rgb(24, 24, 24));
    border: 1px solid var(--line);
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.45), 0 0 0 1px rgba(0, 0, 0, 0.2);
  }
  :global(:root[data-theme='light']) .palette {
    background: color-mix(in srgb, var(--accent) 3%, #f4f4f4);
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.18);
  }
  .palette.searchmode {
    animation: searchmode 0.6s ease-out;
  }
  @keyframes searchmode {
    from {
      box-shadow: 0 0 20px color-mix(in srgb, color-mix(in srgb, var(--accent), var(--text) 20%), transparent 50%);
    }
    to {
      box-shadow: 0 0 250px color-mix(in srgb, var(--accent), transparent 100%);
    }
  }
  .field {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 52px;
    padding: 0 14px 0 16px;
    border-bottom: 1px solid var(--line);
  }
  .lead {
    color: var(--text-2);
    display: grid;
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: none;
    background: transparent;
    font: 500 15px/1 var(--font);
  }
  input::placeholder {
    color: var(--text-3);
  }
  .chip {
    padding: 3px 9px;
    border-radius: 10px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--accent-fg);
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .star {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    corner-shape: var(--sq);
    color: var(--text-2);
    flex: none;
  }
  .star:hover {
    background: var(--hover);
    color: var(--text);
  }
  .star.on {
    color: var(--accent-button);
  }
  .badge {
    position: absolute;
    right: -3px;
    bottom: -3px;
    display: grid;
    color: var(--accent-button);
    pointer-events: none;
  }
  .results {
    max-height: min(440px, 55vh);
    overflow-y: auto;
    padding: 6px 4px;
  }
  .item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 45px;
    padding: 0 12px;
    border-radius: 10px;
    corner-shape: var(--sq);
    text-align: left;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.sel {
    background: color-mix(in srgb, var(--accent) 20%, var(--element-bg));
  }
  .item-icon {
    position: relative;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    background: var(--element-bg);
    color: var(--text-2);
    flex: none;
  }
  .item-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .item-title {
    font-size: 13.5px;
  }
  .item-sub {
    font-size: 12px;
    color: var(--text-3);
  }
  .tag {
    font-size: 11.5px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .hints {
    display: flex;
    gap: 16px;
    padding: 9px 16px;
    border-top: 1px solid var(--line);
    font-size: 11.5px;
    color: var(--text-3);
  }
  .hints .kbd {
    margin-right: 4px;
  }
</style>
