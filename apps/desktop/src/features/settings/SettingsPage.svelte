<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import Segmented from '$shared/ui/Segmented.svelte';
  import ShortcutEditor from '$features/shortcuts/ShortcutEditor.svelte';
  import { settings, MATERIAL_FALLBACKS, SIDEBAR_MAX, SIDEBAR_MIN, type Settings } from './settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { formatBytes } from '$shared/format';
  import { api } from '$platform/ipc';
  import { makeDefaultBrowser } from '$features/default-browser/default-browser';
  import { openInternal, openNotes, shortcutHint } from '../../app/actions';
  import { bookmarks } from '$features/bookmarks/bookmarks.svelte';
  import { backupBookmarksFile, exportBookmarksFile, importBookmarksFile, restoreBookmarks } from '$features/bookmarks/bookmark-actions';
  import { displayChord } from '$features/shortcuts/shortcuts';
  import { SEARCH_ENGINES } from '$shared/url';
  import { SIDEBAR_POSITIONS, isStrip, positionHint } from '$features/sidebar/sidebar-position';
  import { appInfo, incognito } from '$features/window/window.svelte';
  import { onboarding } from '$features/onboarding/onboarding.svelte';
  import { ACCENT_NAMES, normalizeHomepage } from '$features/onboarding/onboarding-flow';
  import { browser, SPACE_COLORS } from '$features/browser/browser.svelte';
  import { updates, updateLine } from '$features/updates/updates.svelte';
  import { timeAgo } from '$shared/format';
  import type { BlockerStats } from '$features/blocker/types';
  import type { MemoryStats } from './types';
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

  const s = $derived(settings.value);
  let current = $state('general');
  let scroller: HTMLDivElement | undefined = $state();
  const info = $derived(appInfo.info);
  let stats = $state<BlockerStats | null>(null);
  let memory = $state<MemoryStats | null>(null);
  let updating = $state(false);
  let customList = $state('');
  let defaultStatus = $state<{ registered: boolean; isDefault: boolean } | null>(null);
  let clearKinds = $state({ history: true, cookies: false, cache: true, downloads: false, permissions: false });
  let clearRange = $state('hour');
  let clearing = $state(false);
  /** FFmpeg the media downloader would use (undefined while checking). */
  let ffmpeg = $state<string | null | undefined>(undefined);

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }

  const notesKeys = $derived(s.shortcuts.openNotes ?? []);
  const notesConflicts = $derived(notesKeys.filter((k) => settings.globalShortcutConflicts.includes(k)));

  function setNotesGlobal(v: boolean) {
    set('notesGlobalShortcut', v);
    void settings.pushShortcuts();
  }

  let homepageText = $state(settings.value.homepage);
  let homepageError = $state('');
  function saveHomepage() {
    const r = normalizeHomepage(homepageText);
    if (!r.ok) {
      homepageError = r.error;
      return;
    }
    homepageError = '';
    homepageText = r.url;
    set('homepage', r.url);
  }

  async function refreshMemory() {
    memory = await api.memoryStats().catch(() => null);
  }

  async function refreshBlocker(update = false) {
    updating = update;
    try {
      stats = await api.blockerConfigure(settings.enabledLists(), update, s.blockerDisabledSites);
      if (update) settings.update({ filterListsUpdated: Date.now() });
    } catch (e) {
      ui.toast(`Could not update filter lists: ${e}`, { icon: 'alert' });
    } finally {
      updating = false;
    }
  }

  function addCustomList() {
    const url = customList.trim();
    if (!/^https?:\/\//.test(url)) return ui.toast('Enter a list URL starting with https://', { icon: 'alert' });
    const id = 'custom-' + Math.abs([...url].reduce((h, c) => (h * 31 + c.charCodeAt(0)) | 0, 7)).toString(36);
    if (s.filterLists.some((l) => l.url === url)) return;
    settings.value.filterLists.push({ id, name: new URL(url).hostname, url, enabled: true, custom: true });
    settings.save(false);
    customList = '';
    void refreshBlocker(true);
  }

  async function clearData() {
    clearing = true;
    const since =
      clearRange === 'all' ? null : Date.now() - ({ hour: 3600e3, day: 86400e3, week: 7 * 86400e3, month: 30 * 86400e3 }[clearRange] ?? 0);
    const kinds: string[] = [];
    if (clearKinds.cookies) kinds.push('siteData');
    if (clearKinds.cache) kinds.push('cache');
    if (clearKinds.downloads) kinds.push('downloads');
    if (clearKinds.permissions) kinds.push('permissions');
    try {
      await api.clearBrowsingData(kinds, since, clearKinds.history);
      ui.toast('Browsing data cleared', { icon: 'check' });
    } catch (e) {
      ui.toast(`Could not clear data: ${e}`, { icon: 'alert' });
    } finally {
      clearing = false;
    }
  }

  async function makeDefault() {
    try {
      await makeDefaultBrowser();
      setTimeout(async () => (defaultStatus = await api.defaultBrowserStatus()), 1500);
    } catch {
      /* makeDefaultBrowser already said why */
    }
  }

  async function pickDownloadDir() {
    const dir = await api.pickFolder('Choose a download folder');
    if (dir) set('downloadDir', dir);
  }

  async function refreshTools() {
    ffmpeg = (await api.mediaTools().catch(() => ({ ffmpeg: null }))).ffmpeg;
  }

  function setFfmpeg<K extends 'useFfmpeg' | 'ffmpegPath'>(key: K, value: Settings[K]) {
    set(key, value);
    // The backend sees the new value once the settings push lands.
    setTimeout(() => void refreshTools(), 400);
  }

  async function pickFfmpeg() {
    const path = await api.mediaPickFfmpeg();
    if (path) setFfmpeg('ffmpegPath', path);
  }

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
    void api.focusView(null);
    void tick().then(() => searchEl?.focus());
    void refreshBlocker(false);
    void refreshMemory();
    void api.defaultBrowserStatus().then((d) => (defaultStatus = d)).catch(() => {});
    void refreshTools();
    const t = setInterval(refreshMemory, 5000);
    if (section) void tick().then(() => jump(section));
    // Rows that appear or change while searching (an {#if} opening, live stats) are filtered too.
    // Only content changes are observed: the filter itself just flips `hidden` attributes.
    const observer = new MutationObserver(() => {
      if (query.trim()) applySearch();
    });
    if (scroller) observer.observe(scroller, { childList: true, characterData: true, subtree: true });
    return () => {
      clearInterval(t);
      observer.disconnect();
      if (canHighlight) CSS.highlights.delete(HIGHLIGHT);
    };
  });

  const sleepOptions = [
    { value: 0, label: 'Never' },
    { value: 1, label: '1 minute' },
    { value: 2, label: '2 minutes' },
    { value: 5, label: '5 minutes' },
    { value: 10, label: '10 minutes' },
    { value: 15, label: '15 minutes' },
    { value: 30, label: '30 minutes' },
    { value: 60, label: '1 hour' },
  ];
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
      <!-- General -->
      <section id="settings-general">
        <h2>General</h2>
        <div class="group">
          <div class="row">
            <div class="text"><b>Restore tabs from last time</b><span>Pinned tabs and Essentials are always kept.</span></div>
            <Toggle checked={s.restoreSession} onchange={(v) => set('restoreSession', v)} label="Restore tabs" />
          </div>
          <div class="row">
            <div class="text"><b>Open new tabs at the top</b><span>Otherwise new tabs go to the bottom of the list.</span></div>
            <Toggle checked={s.newTabsAtTop} onchange={(v) => set('newTabsAtTop', v)} label="New tabs at top" />
          </div>
          <div class="row">
            <div class="text">
              <b>Home page</b>
              <span class:err={!!homepageError}>{homepageError || 'Opens in a window with no tabs, and when you press Enter in an empty new tab. Leave empty for the Zenith start page.'}</span>
            </div>
            <input
              class="field homepage"
              type="url"
              spellcheck="false"
              placeholder="Zenith start page"
              aria-label="Home page"
              aria-invalid={!!homepageError}
              bind:value={homepageText}
              onchange={saveHomepage}
              onkeydown={(e) => e.key === 'Enter' && saveHomepage()}
            />
          </div>
          <div class="row">
            <div class="text"><b>Search engine</b><span>Used for searches from the address bar.</span></div>
            <select class="field select" aria-label="Search engine" value={s.searchEngine} onchange={(e) => set('searchEngine', (e.currentTarget as HTMLSelectElement).value)}>
              {#each Object.entries(SEARCH_ENGINES) as [id, e] (id)}<option value={id}>{e.name}</option>{/each}
            </select>
          </div>
          <div class="row">
            <div class="text">
              <b>Search suggestions</b>
              <span>Sends what you type in the address bar to {SEARCH_ENGINES[s.searchEngine]?.name}. Off by default.</span>
            </div>
            <Toggle checked={s.searchSuggestions} onchange={(v) => set('searchSuggestions', v)} label="Search suggestions" />
          </div>
          <div class="row">
            <div class="text"><b>Show most visited sites on new tabs</b><span>Taken from your local history only.</span></div>
            <Toggle checked={s.showTopSites} onchange={(v) => set('showTopSites', v)} label="Top sites" />
          </div>
          {#if !incognito}
            <div class="row">
              <div class="text">
                <b>Setup assistant</b>
                <span>{s.onboardedAt ? `Completed ${new Date(s.onboardedAt).toLocaleDateString()}. ` : ''}Walk through theme, search, home page, import and privacy again.</span>
              </div>
              <button class="btn" onclick={() => onboarding.restart()}>Run setup again</button>
            </div>
          {/if}
        </div>
      </section>

      <!-- Appearance -->
      <section id="settings-appearance">
        <h2>Appearance</h2>
        <div class="group">
          <div class="row">
            <div class="text"><b>Theme</b></div>
            <Segmented
              value={s.theme}
              options={[
                { value: 'system', label: 'Windows' },
                { value: 'light', label: 'Light' },
                { value: 'dark', label: 'Dark' },
              ]}
              onchange={(v) => set('theme', v)}
            />
          </div>
          {#if browser.activeSpace}
            <div class="row">
              <div class="text">
                <b>Accent colour</b>
                <span>Tints the window and highlights for the “{browser.activeSpace.name}” space. Each space can have its own.</span>
              </div>
              <div class="swatches" role="radiogroup" aria-label="Accent colour">
                {#each SPACE_COLORS as c (c)}
                  <label class="swatch" style:--c={c} title={ACCENT_NAMES[c] ?? c}>
                    <input
                      type="radio"
                      name="settings-accent"
                      value={c}
                      checked={onboarding.accent === c}
                      aria-label={ACCENT_NAMES[c] ?? c}
                      onchange={() => onboarding.setAccent(c)}
                    />
                  </label>
                {/each}
              </div>
            </div>
          {/if}
          <div class="row">
            <div class="text">
              <b>Translucent sidebar</b>
              <span>
                {s.translucentSidebar && settings.materialFallback
                  ? MATERIAL_FALLBACKS[settings.materialFallback]
                  : info && !info.windows11
                    ? 'Needs Windows 11; Windows 10 uses the solid theme.'
                    : 'Your desktop shows through the sidebar. Off uses the least GPU and memory.'}
              </span>
            </div>
            <Toggle
              checked={s.translucentSidebar && (!info || info.windows11)}
              disabled={!!info && !info.windows11}
              onchange={(v) => set('translucentSidebar', v)}
              label="Translucent sidebar"
            />
          </div>
          {#if s.translucentSidebar && (!info || info.windows11)}
            <div class="row">
              <div class="text"><b>Sidebar material</b><span>Mica tints with your wallpaper; Acrylic blurs what is behind the window.</span></div>
              <Segmented
                value={s.material}
                options={[
                  { value: 'mica', label: 'Mica' },
                  { value: 'mica-alt', label: 'Mica Alt' },
                  { value: 'acrylic', label: 'Acrylic' },
                ]}
                onchange={(v) => set('material', v)}
              />
            </div>
          {/if}
          <div class="row">
            <div class="text"><b>Website appearance</b><span>The color scheme websites are asked to use.</span></div>
            <Segmented
              value={s.contentScheme}
              options={[
                { value: 'system', label: 'Windows' },
                { value: 'light', label: 'Light' },
                { value: 'dark', label: 'Dark' },
              ]}
              onchange={(v) => set('contentScheme', v)}
            />
          </div>
          <div class="row">
            <div class="text">
              <b>Sidebar position</b>
              <span>{positionHint(s.sidebarPosition)} You can also right-click the sidebar to move it.</span>
            </div>
            <Segmented
              value={s.sidebarPosition}
              options={SIDEBAR_POSITIONS}
              onchange={(v) => {
                ui.flyout = false;
                set('sidebarPosition', v);
              }}
            />
          </div>
          <div class="row">
            <div class="text">
              <b>Layout</b>
              <span>
                {isStrip(s.sidebarPosition) && s.toolbarMode === 'single'
                  ? 'A tab strip keeps the toolbar above the page; the single toolbar applies with the sidebar on the left or right.'
                  : isStrip(s.sidebarPosition) && s.toolbarMode === 'collapsed'
                    ? 'Icon-only tabs in the strip, with a toolbar above the page.'
                    : s.toolbarMode === 'single'
                  ? 'Address bar in the sidebar; window controls appear at the top edge.'
                  : s.toolbarMode === 'multiple'
                    ? 'A toolbar with the address bar above the page.'
                    : 'Icon-only sidebar with a toolbar above the page.'}
              </span>
            </div>
            <Segmented
              value={s.toolbarMode}
              options={[
                { value: 'single', label: 'Single toolbar' },
                { value: 'multiple', label: 'Multiple toolbars' },
                { value: 'collapsed', label: 'Collapsed' },
              ]}
              onchange={(v) => set('toolbarMode', v)}
            />
          </div>
          <div class="row">
            <div class="text"><b>Clear button</b><span>Show "Clear" between pinned and regular tabs.</span></div>
            <Toggle checked={s.showClearButton} onchange={(v) => set('showClearButton', v)} label="Clear button" />
          </div>
          <div class="row">
            <div class="text"><b>Compact mode</b><span>Hide the sidebar and toolbar; reveal them from the window edges.{shortcutHint('toggleCompact') ? ` ${shortcutHint('toggleCompact')}.` : ''}</span></div>
            <Toggle checked={s.compact} onchange={(v) => set('compact', v)} label="Compact mode" />
          </div>
          {#if !isStrip(s.sidebarPosition)}
          <div class="row">
            <div class="text"><b>Sidebar width</b><span>{s.sidebarWidth}px — you can also drag the sidebar edge (double-click it to reset).</span></div>
            <input
              type="range"
              min={SIDEBAR_MIN}
              max={SIDEBAR_MAX}
              step="1"
              value={s.sidebarWidth}
              oninput={(e) => (settings.value.sidebarWidth = Number((e.currentTarget as HTMLInputElement).value))}
              onchange={() => settings.save(false)}
            />
          </div>
          {/if}
        </div>
      </section>

      <!-- Privacy -->
      <section id="settings-privacy">
        <h2>Privacy & blocking</h2>
        <div class="group">
          <div class="row">
            <div class="text">
              <b>Block ads and trackers</b>
              <span>
                {#if stats}{stats.rules.toLocaleString()} rules · {stats.totalBlocked.toLocaleString()} requests blocked this session{:else}Network-level blocking in every tab.{/if}
              </span>
            </div>
            <Toggle checked={s.blockerEnabled} onchange={(v) => set('blockerEnabled', v)} label="Ad blocking" />
          </div>
          <div class="sub">
            <div class="sub-head">
              <span>Filter lists</span>
              <button class="btn ghost sm" disabled={updating} onclick={() => refreshBlocker(true)}>
                <Icon name="reload" size={14} />{updating ? 'Updating…' : 'Update now'}
              </button>
            </div>
            <div class="list-row builtin">
              <Icon name="shieldCheck" size={15} />
              <span class="grow">Zenith built-in list</span>
              <span class="faint">always on</span>
            </div>
            {#each s.filterLists as list (list.id)}
              {@const st = stats?.lists.find((l) => l.id === list.id)}
              <div class="list-row">
                <Toggle
                  checked={list.enabled}
                  label={list.name}
                  onchange={(v) => {
                    list.enabled = v;
                    settings.save(false);
                    void refreshBlocker(false);
                  }}
                />
                <span class="grow ellipsis" title={list.url}>{list.name}</span>
                <span class="faint">
                  {#if st?.error}<span class="err">{st.error}</span>{:else if list.enabled && st}{st.rules.toLocaleString()} rules{/if}
                </span>
                {#if list.custom}
                  <button
                    class="icon-btn sm"
                    title="Remove list"
                    onclick={() => {
                      settings.value.filterLists = s.filterLists.filter((l) => l.id !== list.id);
                      settings.save(false);
                      void refreshBlocker(false);
                    }}><Icon name="trash" size={14} /></button
                  >
                {/if}
              </div>
            {/each}
            <div class="add-row">
              <input class="field grow" placeholder="Add a list URL (hosts or Adblock Plus format)" bind:value={customList} onkeydown={(e) => e.key === 'Enter' && addCustomList()} />
              <button class="btn" onclick={addCustomList}>Add</button>
            </div>
          </div>
          {#if s.blockerDisabledSites.length}
            <div class="sub">
              <div class="sub-head"><span>Blocking turned off on</span></div>
              {#each s.blockerDisabledSites as site (site)}
                <div class="list-row">
                  <Icon name="shieldOff" size={15} />
                  <span class="grow">{site}</span>
                  <button
                    class="icon-btn sm"
                    title="Turn blocking back on"
                    onclick={() => {
                      settings.value.blockerDisabledSites = s.blockerDisabledSites.filter((x) => x !== site);
                      settings.save(false);
                      void api.blockerSetDisabledSites(settings.value.blockerDisabledSites);
                    }}><Icon name="close" size={14} /></button
                  >
                </div>
              {/each}
            </div>
          {/if}
          <div class="row">
            <div class="text"><b>Block pop-ups</b><span>Windows a page opens without you clicking anything.</span></div>
            <Toggle checked={s.blockPopups} onchange={(v) => set('blockPopups', v)} label="Block pop-ups" />
          </div>
          <div class="row">
            <div class="text"><b>Save browsing history</b></div>
            <Toggle checked={s.recordHistory} onchange={(v) => set('recordHistory', v)} label="Save history" />
          </div>
          <div class="row">
            <div class="text"><b>Keep history for</b></div>
            <select
              class="field select"
              value={s.historyDays}
              onchange={(e) => {
                const days = Number((e.currentTarget as HTMLSelectElement).value);
                set('historyDays', days);
                if (days > 0) void api.historyPrune(days);
              }}
            >
              <option value={7}>1 week</option>
              <option value={30}>1 month</option>
              <option value={90}>3 months</option>
              <option value={365}>1 year</option>
              <option value={0}>Forever</option>
            </select>
          </div>
        </div>

        <h3>Clear browsing data</h3>
        <div class="group" data-search-whole>
          <div class="checks">
            <label><input type="checkbox" bind:checked={clearKinds.history} /> Browsing history</label>
            <label><input type="checkbox" bind:checked={clearKinds.cookies} /> Cookies and site data (signs you out)</label>
            <label><input type="checkbox" bind:checked={clearKinds.cache} /> Cached images and files</label>
            <label><input type="checkbox" bind:checked={clearKinds.downloads} /> Download history</label>
            <label><input type="checkbox" bind:checked={clearKinds.permissions} /> Site permissions</label>
          </div>
          <div class="row">
            <select class="field select" bind:value={clearRange}>
              <option value="hour">Last hour</option>
              <option value="day">Last 24 hours</option>
              <option value="week">Last 7 days</option>
              <option value="month">Last 4 weeks</option>
              <option value="all">All time</option>
            </select>
            <button class="btn danger" disabled={clearing} onclick={clearData}><Icon name="eraser" size={15} />{clearing ? 'Clearing…' : 'Clear data'}</button>
          </div>
        </div>
      </section>

      <!-- Tabs & memory -->
      <section id="settings-tabs">
        <h2>Tabs & memory</h2>
        <div class="group">
          <div class="row">
            <div class="text">
              <b>Put background tabs to sleep after</b>
              <span>Sleeping tabs free their memory and reload when you open them. Tabs playing audio, downloading or in split view stay awake.</span>
            </div>
            <select class="field select" value={s.sleepMinutes} onchange={(e) => set('sleepMinutes', Number((e.currentTarget as HTMLSelectElement).value))}>
              {#each sleepOptions as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
            </select>
          </div>
          <div class="row">
            <div class="text"><b>Suspend background tabs after a minute</b><span>Pauses page scripts without unloading the page.</span></div>
            <Toggle checked={s.suspendHidden} onchange={(v) => set('suspendHidden', v)} label="Suspend background tabs" />
          </div>
          <div class="row">
            <div class="text"><b>Sleep tabs when switching spaces</b></div>
            <Toggle checked={s.sleepOnSpaceSwitch} onchange={(v) => set('sleepOnSpaceSwitch', v)} label="Sleep on space switch" />
          </div>
          <div class="row">
            <div class="text"><b>Unload tabs when Windows is low on memory</b></div>
            <Toggle checked={s.memoryPressure} onchange={(v) => set('memoryPressure', v)} label="Memory pressure" />
          </div>
          <p class="note">Unloading a tab discards anything you typed into it that the page has not saved.</p>
        </div>
        <h3>Memory in use</h3>
        <div class="group" data-search-whole>
          {#if memory}
            <div class="mem-total">
              <div><b>{formatBytes(memory.privateBytes)}</b><span>private memory, all Zenith processes</span></div>
              <div><b>{memory.systemLoad}%</b><span>of system memory in use</span></div>
            </div>
            <div class="procs">
              {#each memory.processes.slice(0, 8) as p (p.pid)}
                <div class="proc"><span class="ellipsis">{p.name}</span><span class="faint">{p.pid}</span><span>{formatBytes(p.privateBytes)}</span></div>
              {/each}
            </div>
          {:else}
            <p class="note">Measuring…</p>
          {/if}
        </div>
      </section>

      <!-- Bookmarks -->
      <section id="settings-bookmarks">
        <h2>Bookmarks</h2>
        <div class="group">
          <div class="row">
            <div class="text">
              <b>Bookmarks toolbar</b>
              <span>{shortcutHint('toggleBookmarksBar') ? `${shortcutHint('toggleBookmarksBar')} shows or hides it. ` : ''}With the single toolbar layout, a toolbar that isn't showing still appears while the pointer is at the top edge of the window.</span>
            </div>
            <Segmented
              value={s.bookmarksBar}
              options={[
                { value: 'always', label: 'Always' },
                { value: 'newtab', label: 'On a new tab' },
                { value: 'never', label: 'Never' },
              ]}
              onchange={(v) => set('bookmarksBar', v)}
            />
          </div>
          <div class="row">
            <div class="text"><b>Show Other Bookmarks</b><span>At the end of the toolbar, when something is in it.</span></div>
            <Toggle checked={s.bookmarksOther} onchange={(v) => set('bookmarksOther', v)} label="Show Other Bookmarks" />
          </div>
          <div class="row">
            <div class="text">
              <b>Bookmarks sidebar</b>
              <span>{shortcutHint('toggleBookmarksSidebar') ? `${shortcutHint('toggleBookmarksSidebar')}. ` : ''}All of your bookmarks, with search, beside the page.</span>
            </div>
            <button class="btn" onclick={() => set('bookmarksPanel', !s.bookmarksPanel)}>{s.bookmarksPanel ? 'Close' : 'Open'}</button>
          </div>
        </div>
        <h3>Manage</h3>
        <div class="group">
          <div class="row">
            <div class="text">
              <b>Bookmarks library</b>
              <span>{bookmarks.count} {bookmarks.count === 1 ? 'bookmark' : 'bookmarks'}. Organize folders, tags and keywords.</span>
            </div>
            <button class="btn" onclick={() => openInternal('bookmarks')}><Icon name="library" size={15} />Open library</button>
          </div>
          <div class="row">
            <div class="text"><b>Import</b><span>From another browser on this PC, or from an exported bookmarks file (.html).</span></div>
            <div class="inline">
              <button class="btn" onclick={() => (ui.bookmarkImport = true)}><Icon name="importBox" size={15} />From a browser…</button>
              <button class="btn" onclick={() => void importBookmarksFile()}>From a file…</button>
            </div>
          </div>
          <div class="row">
            <div class="text">
              <b>Export and back up</b>
              <span>An HTML file other browsers can read, or a backup of everything: folders, tags, keywords and workspaces. Zenith also keeps a backup for each day you use it.</span>
            </div>
            <div class="inline">
              <button class="btn" onclick={() => void exportBookmarksFile()}>Export…</button>
              <button class="btn" onclick={() => void backupBookmarksFile()}>Back up…</button>
              <button class="btn" onclick={() => void restoreBookmarks()}>Restore…</button>
            </div>
          </div>
        </div>
      </section>

      <!-- Downloads -->
      <section id="settings-downloads">
        <h2>Downloads</h2>
        <div class="group">
          <div class="row">
            <div class="text"><b>Save files to</b><span class="ellipsis">{s.downloadDir ?? info?.defaultDownloadDir ?? 'Downloads'}</span></div>
            <div class="inline">
              {#if s.downloadDir}<button class="btn ghost" onclick={() => set('downloadDir', null)}>Reset</button>{/if}
              <button class="btn" onclick={pickDownloadDir}><Icon name="folder" size={15} />Change</button>
            </div>
          </div>
        </div>
        <h3>Video downloader</h3>
        <div class="group">
          <div class="row">
            <div class="text">
              <b>Show a download button on videos</b>
              <span>Appears when you point at a video in a page. Applies to pages loaded after you change it. Videos can also be saved from the address bar, the page menu or by right-clicking a video.</span>
            </div>
            <Toggle checked={s.mediaButton} onchange={(v) => set('mediaButton', v)} label="Download button on videos" />
          </div>
          <div class="row">
            <div class="text">
              <b>Use FFmpeg when it is installed</b>
              <span
                >{#if ffmpeg === undefined}Checking…{:else if ffmpeg}Found: {ffmpeg}{:else}Not found. Streams are still saved; FFmpeg only adds .ts to .mp4 conversion and merging of audio and video that Zenith can't merge itself.{/if}</span
              >
            </div>
            <Toggle checked={s.useFfmpeg} onchange={(v) => setFfmpeg('useFfmpeg', v)} label="Use FFmpeg" />
          </div>
          <div class="row">
            <div class="text"><b>FFmpeg location</b><span class="ellipsis">{s.ffmpegPath ?? 'Found automatically (PATH)'}</span></div>
            <div class="inline">
              {#if s.ffmpegPath}<button class="btn ghost" onclick={() => setFfmpeg('ffmpegPath', null)}>Reset</button>{/if}
              <button class="btn" onclick={pickFfmpeg}><Icon name="folder" size={15} />Choose ffmpeg.exe</button>
            </div>
          </div>
          <p class="note">DRM-protected video (Netflix, Disney+ and similar) is never downloaded.</p>
        </div>
      </section>

      <!-- Notes -->
      <section id="settings-notes">
        <h2>Notes</h2>
        <div class="group">
          <div class="row">
            <div class="text">
              <b>Notes shortcut</b>
              <span>
                {#if notesKeys.length}
                  {#each notesKeys as chord, ci (chord)}{#if ci > 0}{' or '}{/if}{#each displayChord(chord) as part, i (i)}<span class="kbd">{part}</span>{/each}{/each}
                  opens your notes.
                {:else}
                  No shortcut is set.
                {/if}
              </span>
            </div>
            <div class="inline">
              <button class="btn ghost" onclick={() => jump('shortcuts')}>Change</button>
              <button class="btn" onclick={() => openNotes()}><Icon name="note" size={15} />Open notes</button>
            </div>
          </div>
          <div class="row">
            <div class="text">
              <b>Use the shortcut in every app</b>
              <span class:err={s.notesGlobalShortcut && notesConflicts.length > 0}>
                {#if s.notesGlobalShortcut && notesConflicts.length}
                  {notesConflicts.join(', ')} is already taken by another app, so it only works inside Zenith. Choose another shortcut below.
                {:else}
                  Brings Zenith to the front with your notes, even while you're in another app. While Zenith is running, other apps don't receive this key combination.
                {/if}
              </span>
            </div>
            <Toggle checked={s.notesGlobalShortcut} onchange={setNotesGlobal} label="Use the notes shortcut in every app" />
          </div>
        </div>
      </section>

      <!-- Shortcuts -->
      <section id="settings-shortcuts">
        <h2>Keyboard shortcuts</h2>
        <p class="lead">Shortcuts work while a page has focus. Editing keys such as Ctrl+C, Ctrl+V and Ctrl+Z always go to the page.</p>
        <!-- Searching for the section itself ("keyboard", "shortcuts") shows every shortcut. -->
        <ShortcutEditor query={'keyboard shortcuts'.includes(q) ? '' : q} />
      </section>

      <!-- Default browser -->
      <section id="settings-default">
        <h2>Default browser</h2>
        <div class="group">
          <div class="row">
            <div class="text">
              <b>{defaultStatus?.isDefault ? 'Zenith is your default browser' : 'Make Zenith your default browser'}</b>
              <span>Windows asks you to confirm the choice in Settings › Apps › Default apps. Links from other apps then open as new tabs here.</span>
            </div>
            {#if defaultStatus?.isDefault}
              <span class="ok"><Icon name="check" size={16} /> Default</span>
            {:else}
              <button class="btn primary" onclick={makeDefault}>Make default</button>
            {/if}
          </div>
          {#if defaultStatus?.registered}
            <div class="row">
              <div class="text"><b>Remove Zenith from the Default apps list</b></div>
              <button
                class="btn ghost"
                onclick={async () => {
                  await api.defaultBrowserUnregister();
                  defaultStatus = await api.defaultBrowserStatus();
                }}>Unregister</button
              >
            </div>
          {/if}
        </div>
      </section>

      <!-- About -->
      <section id="settings-about">
        <h2>About</h2>
        <div class="group about">
          <div class="row">
            <div class="text"><b>Zenith {info?.version ?? ''}</b><span>A lightweight Zen-style browser for Windows, built on Tauri and Microsoft Edge WebView2.</span></div>
          </div>
          <div class="row">
            <div class="text">
              <b>Updates</b>
              <span>{updateLine(updates.status, timeAgo)}</span>
            </div>
            {#if updates.ready}
              <button class="btn primary" disabled={updates.restarting} onclick={() => void updates.restart()}>Restart to update</button>
            {:else}
              <button
                class="btn"
                disabled={updates.status.state === 'checking' || updates.status.state === 'downloading'}
                onclick={() => void updates.check().catch((e) => ui.toast(String(e), { icon: 'alert' }))}>Check for updates</button
              >
            {/if}
          </div>
          <div class="row">
            <div class="text">
              <b>Update automatically</b>
              <span>Downloads new versions in the background and installs them silently when you close Zenith. Your tabs are restored.</span>
            </div>
            <Toggle checked={s.autoUpdate} onchange={(v) => set('autoUpdate', v)} label="Update automatically" />
          </div>
          <div class="kv"><span>WebView2 runtime</span><span>{info?.webviewVersion ?? '…'}</span></div>
          <div class="kv"><span>Profile folder</span><span class="ellipsis">{info?.dataDir ?? '…'}</span></div>
          <div class="kv"><span>Telemetry</span><span>None. Zenith sends no usage data.</span></div>
        </div>
      </section>
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
  .swatches {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
    padding: 4px;
  }
  .swatch {
    position: relative;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
    transition:
      scale 160ms ease-out,
      box-shadow 150ms ease;
  }
  .swatch:active {
    scale: 0.92;
  }
  .swatch input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }
  .swatch:has(input:checked) {
    outline: 2px solid var(--c);
    outline-offset: 2px;
  }
  .swatch:has(input:focus-visible) {
    outline: 2px solid var(--text);
    outline-offset: 2px;
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
  h3 {
    margin: 22px 0 10px;
    font: 600 14px/1.3 var(--font-display);
    color: var(--text-2);
  }
  .lead {
    margin: -4px 0 12px;
    color: var(--text-2);
  }
  .group {
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    padding: 4px 16px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    min-height: 58px;
    padding: 10px 0;
    border-bottom: 1px solid var(--line);
  }
  .row:last-child {
    border-bottom: 0;
  }
  /* Controls keep their natural width; the description column gives way instead. */
  .row > :not(.text) {
    flex: none;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .text b {
    font-weight: 500;
  }
  .text span {
    font-size: 12px;
    color: var(--text-3);
    line-height: 1.4;
  }
  .sub {
    padding: 10px 0 12px;
    border-bottom: 1px solid var(--line);
  }
  .sub-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
    margin-bottom: 6px;
  }
  .btn.sm {
    height: 26px;
    padding: 0 8px;
    font-size: 12px;
  }
  .list-row {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 38px;
  }
  .list-row.builtin {
    color: var(--ok);
  }
  .list-row.builtin .grow {
    color: var(--text);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .err {
    color: var(--danger);
  }
  .homepage {
    width: min(280px, 45%);
    flex: none;
  }
  .add-row {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .select {
    height: 32px;
    padding: 0 8px;
  }
  .checks {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 0 6px;
  }
  .checks label {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .note {
    margin: 10px 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .mem-total {
    display: flex;
    gap: 40px;
    padding: 14px 0 10px;
  }
  .mem-total div {
    display: flex;
    flex-direction: column;
  }
  .mem-total b {
    font: 600 20px/1.2 var(--font-display);
  }
  .mem-total span {
    font-size: 12px;
    color: var(--text-3);
  }
  .procs {
    display: flex;
    flex-direction: column;
    padding-bottom: 10px;
  }
  .proc {
    display: grid;
    grid-template-columns: 1fr 70px 90px;
    gap: 10px;
    font-size: 12px;
    padding: 3px 0;
    font-variant-numeric: tabular-nums;
  }
  .proc span:last-child {
    text-align: right;
  }
  .inline {
    display: flex;
    gap: 6px;
  }
  .ok {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ok);
    font-weight: 600;
  }
  .kv {
    display: flex;
    justify-content: space-between;
    gap: 20px;
    padding: 10px 0;
    border-top: 1px solid var(--line);
    font-size: 12.5px;
  }
  .kv span:first-child {
    color: var(--text-2);
  }
  input[type='range'] {
    width: 180px;
    accent-color: var(--accent);
  }
  input[type='checkbox'] {
    accent-color: var(--accent);
    width: 15px;
    height: 15px;
  }
</style>
