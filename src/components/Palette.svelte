<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import Favicon from './Favicon.svelte';
  import { browser } from '../lib/browser.svelte';
  import { bookmarks } from '../lib/bookmarks.svelte';
  import { settings } from '../lib/settings.svelte';
  import { ui, type PaletteMode } from '../lib/ui.svelte';
  import { api } from '../lib/ipc';
  import { focusAfterLayout, overlayLease } from '../lib/layout.svelte';
  import { commands } from '../lib/actions';
  import { scoreAny } from '../lib/fuzzy';
  import { NEW_TAB_URL, SEARCH_ENGINES, normalizeInput, prettyUrl, searchUrl, titleFor } from '../lib/url';
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
    void tick().then(() => {
      input?.focus();
      if (ui.palette.selectAll) input?.select();
    });
  });

  const commandMode = $derived(mode === 'command' || query.startsWith('>'));
  const q = $derived(commandMode ? query.replace(/^>\s*/, '').trim() : query.trim());
  const engine = $derived(SEARCH_ENGINES[settings.value.searchEngine] ?? SEARCH_ENGINES.google);

  let seq = 0;
  let historyTimer: ReturnType<typeof setTimeout> | undefined;
  let suggestTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const text = q;
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
      const r = await api.historySearch(text, 7).catch(() => []);
      if (mine === seq) history = r;
    }, text ? 40 : 0);
    const looksLikeUrl = !/\s/.test(text) && text.includes('.');
    if (settings.value.searchSuggestions && text.length > 1 && !looksLikeUrl) {
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
    const out: Item[] = [];
    const seen = new Set<string>();
    if (q) {
      const target = normalizeInput(q, settings.value.searchEngine);
      const isSearch = target === searchUrl(q, settings.value.searchEngine);
      out.push(
        isSearch
          ? { key: 'go', kind: 'search', title: q, subtitle: `Search ${engine.name}`, icon: 'search', run: (h) => go(target, h) }
          : { key: 'go', kind: 'go', title: prettyUrl(target), subtitle: 'Open address', icon: 'globe', url: target, run: (h) => go(target, h) },
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
      const marks = bookmarks.items
        .map((b) => ({ b, s: scoreAny(q, b.title, b.url) }))
        .filter((x) => x.s > 0 && !seen.has(x.b.url))
        .sort((a, b) => b.s - a.s)
        .slice(0, 3);
      for (const { b } of marks) {
        seen.add(b.url);
        out.push({ key: `b:${b.id}`, kind: 'bookmark', title: b.title || prettyUrl(b.url), subtitle: prettyUrl(b.url), url: b.url, icon: 'star', run: (h) => go(b.url, h) });
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
      out.push({ key: `h:${h.id}`, kind: 'history', title: h.title || prettyUrl(h.url), subtitle: prettyUrl(h.url), url: h.url, icon: 'history', run: (how) => go(h.url, how) });
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

  function onkeydown(e: KeyboardEvent) {
    const n = items.length;
    if (e.key === 'ArrowDown' && n) {
      e.preventDefault();
      selected = (selected + 1) % n;
      void scrollSelected();
    } else if (e.key === 'ArrowUp' && n) {
      e.preventDefault();
      selected = selected < 0 ? n - 1 : (selected - 1 + n) % n;
      void scrollSelected();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const it = items[selected];
      if (it) it.run(e.altKey ? 'newTab' : e.shiftKey ? 'glance' : 'enter');
      else if (!q && mode === 'newTab') {
        close(false);
        browser.openTab(NEW_TAB_URL);
      } else if (!q) close();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === 'Tab' && !e.shiftKey) {
      const it = items[selected];
      if (it?.url && it.kind !== 'tab') {
        e.preventDefault();
        query = prettyUrl(it.url);
      }
    } else if (e.key === 'Backspace' && commandMode && query === '' && mode === 'command') {
      mode = 'navigate';
    }
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
          bind:value={query}
          {onkeydown}
          {placeholder}
          spellcheck="false"
          autocomplete="off"
          aria-label={placeholder}
        />
        {#if mode === 'newTab' && !commandMode}
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
              onmousemove={() => (selected = i)}
              onclick={(e) => it.run(e.altKey ? 'newTab' : e.shiftKey ? 'glance' : 'enter')}
            >
              <span class="item-icon">
                {#if it.kind === 'tab'}
                  <Favicon url={it.url ?? ''} favicon={it.favicon} />
                {:else}
                  <Icon name={it.icon ?? 'globe'} size={16} />
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
        <span><span class="kbd">Alt ↵</span> new tab</span>
        <span><span class="kbd">Shift ↵</span> Glance</span>
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
