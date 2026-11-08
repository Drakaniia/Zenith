<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import ShortcutEditor from '$features/shortcuts/ShortcutEditor.svelte';
  import GeneralSection from './sections/GeneralSection.svelte';
  import AppearanceSection from './sections/AppearanceSection.svelte';
  import PrivacySection from './sections/PrivacySection.svelte';
  import TabsSection from './sections/TabsSection.svelte';
  import DefaultBrowserSection from './sections/DefaultBrowserSection.svelte';
  import AboutSection from './sections/AboutSection.svelte';
  import BookmarksSettings from '$features/bookmarks/BookmarksSettings.svelte';
  import DownloadsSettings from '$features/downloads/DownloadsSettings.svelte';
  import NotesSettings from '$features/notes/NotesSettings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { webviewApi } from '$platform/webview-api';
  import { openInternal } from '$features/browser/target';
  import type { IconName } from '$shared/ui/icons';

  let { section = '' }: { section?: string } = $props();

  const SECTIONS: { id: string; label: string; icon: IconName }[] = [
    { id: 'general', label: 'General', icon: 'settings' },
    { id: 'appearance', label: 'Appearance', icon: 'sun' },
    { id: 'privacy', label: 'Privacy & blocking', icon: 'shield' },
    { id: 'tabs', label: 'Tabs & memory', icon: 'zap' },
    { id: 'bookmarks', label: 'Bookmarks', icon: 'bookmark' },
    { id: 'downloads', label: 'Downloads', icon: 'download' },
    { id: 'notes', label: 'Notes', icon: 'note' },
    { id: 'shortcuts', label: 'Keyboard shortcuts', icon: 'keyboard' },
    { id: 'default', label: 'Default browser', icon: 'globe' },
    { id: 'about', label: 'About', icon: 'info' },
  ];

  let current = $state('general');
  let scroller: HTMLDivElement | undefined = $state();

  // Search: filters the rendered page in place, so every setting is searchable without a
  // separate index that could drift from the markup.
  let query = $state('');
  let searchEl: HTMLInputElement | undefined = $state();
  /** Sections with at least one match; null while not searching. */
  let matched = $state<Set<string> | null>(null);
  const HIGHLIGHT = 'settings-search';
  const canHighlight = typeof Highlight !== 'undefined' && typeof CSS !== 'undefined' && 'highlights' in CSS;
  const EXTERNAL = { passwords: 'Passwords & autofill', extensions: 'Extensions' };
  const q = $derived(query.trim().toLowerCase());
  const externalDim = (label: string) => !!q && !label.toLowerCase().includes(q);

  function applySearch() {
    if (!scroller) return;
    const needle = query.trim().toLowerCase();
    const has = (el: Element) => (el.textContent ?? '').toLowerCase().includes(needle);
    const show = (el: Element, on: boolean) => ((el as HTMLElement).hidden = !on);
    const hits = new Set<string>();
    for (const sec of scroller.querySelectorAll<HTMLElement>('section[id^="settings-"]')) {
      const kids = [...sec.children];
      if (!needle) {
        sec.hidden = false;
        for (const k of kids) show(k, true);
        for (const g of sec.querySelectorAll(':scope > .group > *')) show(g, true);
        continue;
      }
      const h2 = kids.find((k) => k.tagName === 'H2');
      // The section's own name matches: show all of it.
      const whole = !!h2 && has(h2);
      let any = whole;
      // An h3 starts a block that runs to the next h3; the first block has no heading.
      let heading: Element | null = null;
      let block: Element[] = [];
      const flush = () => {
        const headHit = whole || (!!heading && has(heading));
        let blockHit = false;
        for (const el of block) {
          let on: boolean;
          if (headHit) {
            on = true;
            if (el.classList.contains('group')) for (const it of el.children) show(it, true);
          } else if (el.classList.contains('group') && !el.hasAttribute('data-search-whole')) {
            // A settings group: keep only its matching rows.
            on = false;
            for (const it of el.children) {
              const hit = has(it);
              show(it, hit);
              on ||= hit;
            }
          } else on = has(el);
          show(el, on);
          blockHit ||= on;
        }
        if (heading) show(heading, blockHit);
        any ||= blockHit;
      };
      for (const k of kids) {
        if (k === h2) continue;
        if (k.tagName === 'H3') {
          flush();
          heading = k;
          block = [];
        } else block.push(k);
      }
      flush();
      if (h2) show(h2, any);
      sec.hidden = !any;
      if (any) hits.add(sec.id.slice('settings-'.length));
    }
    matched = needle ? hits : null;
    highlight(needle);
  }

  function highlight(needle: string) {
    if (!canHighlight || !scroller) return;
    if (!needle) return void CSS.highlights.delete(HIGHLIGHT);
    const ranges: Range[] = [];
    const walk = document.createTreeWalker(scroller, NodeFilter.SHOW_TEXT);
    for (let n = walk.nextNode(); n; n = walk.nextNode()) {
      const text = (n.textContent ?? '').toLowerCase();
      if (!text.includes(needle) || n.parentElement?.closest('[hidden], select, .empty')) continue;
      for (let i = text.indexOf(needle); i !== -1; i = text.indexOf(needle, i + needle.length)) {
        const r = new Range();
        r.setStart(n, i);
        r.setEnd(n, i + needle.length);
        ranges.push(r);
      }
    }
    CSS.highlights.set(HIGHLIGHT, new Highlight(...ranges));
  }

  $effect(() => {
    void query;
    applySearch();
    scroller?.scrollTo({ top: 0, behavior: 'instant' });
  });

  // Ctrl+F on this page (see actions.ts "find"); ignore the count it had before this page opened.
  const searchRequestsAtOpen = ui.settingsSearch;
  $effect(() => {
    if (ui.settingsSearch === searchRequestsAtOpen) return;
    void tick().then(() => searchEl?.select());
  });

  function onSearchKey(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    e.stopPropagation();
    if (query) query = '';
    else searchEl?.blur();
  }

  function jump(id: string) {
    current = id;
    scroller?.querySelector(`#settings-${id}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  function onScroll() {
    if (!scroller) return;
    const top = scroller.getBoundingClientRect().top;
    const visible = SECTIONS.filter((sec) => !matched || matched.has(sec.id));
    let best = visible[0]?.id ?? current;
    for (const sec of visible) {
      const el = scroller.querySelector(`#settings-${sec.id}`);
      if (el && el.getBoundingClientRect().top - top < 120) best = sec.id;
    }
    current = best;
  }

  onMount(() => {
    // The search box is ready to type in: the keyboard moves from the page webviews to the chrome.
    void webviewApi.focusView(null);
    void tick().then(() => searchEl?.focus());
    if (section) void tick().then(() => jump(section));
    // Rows that appear or change while searching (an {#if} opening, live stats) are filtered too.
    // Only content changes are observed: the filter itself just flips `hidden` attributes.
    const observer = new MutationObserver(() => {
      if (query.trim()) applySearch();
    });
    if (scroller) observer.observe(scroller, { childList: true, characterData: true, subtree: true });
    return () => {
      observer.disconnect();
      if (canHighlight) CSS.highlights.delete(HIGHLIGHT);
    };
  });
</script>

<div class="settings">
  <nav class="nav">
    <h1>Settings</h1>
    <div class="search">
      <Icon name="search" size={15} />
      <input
        class="field"
        type="text"
        placeholder="Search settings"
        aria-label="Search settings"
        spellcheck="false"
        bind:this={searchEl}
        bind:value={query}
        onkeydown={onSearchKey}
      />
      {#if query}
        <button class="clear" title="Clear search (Esc)" onclick={() => ((query = ''), searchEl?.focus())}><Icon name="close" size={13} /></button>
      {/if}
    </div>
    {#each SECTIONS as sec (sec.id)}
      {@const dim = !!matched && !matched.has(sec.id)}
      <button class="nav-item" class:on={current === sec.id && !dim} class:dim disabled={dim} onclick={() => jump(sec.id)}>
        <Icon name={sec.icon} size={16} />
        {sec.label}
      </button>
    {/each}
    <button class="nav-item" class:dim={externalDim(EXTERNAL.passwords)} onclick={() => openInternal('passwords', 'settings')}>
      <Icon name="key" size={16} />
      Passwords & autofill
      <Icon name="external" size={13} class="faint-icon" />
    </button>
    <button class="nav-item" class:dim={externalDim(EXTERNAL.extensions)} onclick={() => openInternal('extensions')}>
      <Icon name="puzzle" size={16} />
      Extensions
      <Icon name="external" size={13} class="faint-icon" />
    </button>
  </nav>

  <div class="scroller" bind:this={scroller} onscroll={onScroll}>
    <div class="body">
      {#if matched && matched.size === 0}
        <div class="empty">
          <Icon name="search" size={22} />
          <b>No settings match “{query.trim()}”</b>
          <span>Try a different word, or check the spelling.</span>
        </div>
      {/if}
      <GeneralSection />

      <AppearanceSection />

      <PrivacySection />

      <TabsSection />

      <BookmarksSettings />

      <DownloadsSettings />

      <NotesSettings {jump} />

      <!-- Shortcuts -->
      <section id="settings-shortcuts">
        <h2>Keyboard shortcuts</h2>
        <p class="lead">Shortcuts work while a page has focus. Editing keys such as Ctrl+C, Ctrl+V and Ctrl+Z always go to the page.</p>
        <!-- Searching for the section itself ("keyboard", "shortcuts") shows every shortcut. -->
        <ShortcutEditor query={'keyboard shortcuts'.includes(q) ? '' : q} />
      </section>

      <DefaultBrowserSection />

      <AboutSection />
    </div>
  </div>
</div>

<style>
  .settings {
    display: flex;
    height: 100%;
    user-select: none;
  }
  .nav {
    width: 230px;
    flex: none;
    padding: 28px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-right: 1px solid var(--line);
  }
  h1 {
    margin: 0 0 14px 10px;
    font: 600 22px/1.2 var(--font-display);
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--radius);
    color: var(--text-2);
    text-align: left;
  }
  .nav-item:hover {
    background: var(--hover);
    color: var(--text);
  }
  .nav-item.on {
    background: var(--selected);
    color: var(--text);
    font-weight: 600;
  }
  .nav-item.dim {
    opacity: 0.4;
  }
  .nav-item:disabled {
    pointer-events: none;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    margin: 0 0 12px;
    color: var(--text-3);
  }
  .search > :global(svg) {
    position: absolute;
    left: 10px;
    pointer-events: none;
  }
  .search input {
    width: 100%;
    height: 34px;
    padding: 0 30px 0 32px;
  }
  .search .clear {
    position: absolute;
    right: 6px;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: var(--radius);
    color: var(--text-3);
  }
  .search .clear:hover {
    background: var(--hover);
    color: var(--text);
  }
  /* The search filter hides rows with `hidden`; rows' own display rules must not override it. */
  .settings :global([hidden]) {
    display: none !important;
  }
  :global(::highlight(settings-search)) {
    background-color: color-mix(in srgb, var(--accent) 35%, transparent);
    color: inherit;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 80px 0;
    color: var(--text-3);
    text-align: center;
  }
  .empty b {
    color: var(--text);
    font-weight: 600;
  }
  .empty span {
    font-size: 12px;
  }
  .nav-item :global(.faint-icon) {
    margin-left: auto;
    opacity: 0.5;
  }
  .scroller {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    scroll-behavior: smooth;
  }
  .body {
    max-width: 760px;
    padding: 28px 36px 80px;
  }
  section {
    padding-top: 8px;
    margin-bottom: 28px;
  }
  h2 {
    margin: 0 0 12px;
    font: 600 18px/1.3 var(--font-display);
  }
  .lead {
    margin: -4px 0 12px;
    color: var(--text-2);
  }
</style>
