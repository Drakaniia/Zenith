<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '../Icon.svelte';
  import Favicon from '../Favicon.svelte';
  import { browser } from '../../lib/browser.svelte';
  import { settings } from '../../lib/settings.svelte';
  import { ui } from '../../lib/ui.svelte';
  import { api } from '../../lib/ipc';
  import { incognito } from '../../lib/app.svelte';
  import { SEARCH_ENGINES, displayHost, hostOf, titleFor } from '../../lib/url';
  import type { HistoryEntry, Tab } from '../../lib/types';

  // The new-tab page is drawn by the chrome UI (no webview, no network requests).
  let { tab }: { tab?: Tab } = $props();

  let top = $state<HistoryEntry[]>([]);
  const essentials = $derived(browser.essentials.map((id) => browser.tabs[id]).filter(Boolean));
  const engine = $derived(SEARCH_ENGINES[settings.value.searchEngine] ?? SEARCH_ENGINES.google);

  const hour = new Date().getHours();
  const greeting = hour < 5 ? 'Good night' : hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';

  onMount(async () => {
    // Incognito windows record no history, so there are no top sites to show.
    if (!settings.value.showTopSites || incognito) return;
    const rows = await api.historyTop(60).catch(() => []);
    const seen = new Set<string>();
    const engines = Object.values(SEARCH_ENGINES).map((e) => hostOf(e.home));
    top = rows
      .filter((r) => {
        const h = hostOf(r.url).replace(/^www\./, '');
        if (!h || seen.has(h) || engines.some((e) => e.endsWith(h))) return false;
        seen.add(h);
        return true;
      })
      .slice(0, 8);
  });

  function search(initial = '') {
    ui.openPalette(tab ? 'navigate' : 'newTab', initial);
  }

  function open(url: string) {
    if (tab) browser.navigate(tab, url);
    else browser.openTab(url);
  }

  function hue(text: string) {
    let h = 0;
    for (const c of text) h = (h * 31 + c.charCodeAt(0)) % 360;
    return h;
  }
</script>

<div class="ntp">
  <div class="center">
    {#if incognito}
      <span class="incognito-mark"><Icon name="incognito" size={34} /></span>
      <h1>You're incognito</h1>
      <p class="incognito-note">
        Pages you open in this window aren't saved to your history, and their cookies and site data are deleted when
        you close it. Downloads and bookmarks you create are kept. Websites, your employer and your internet provider
        can still see what you visit.
      </p>
    {:else}
    <svg class="logo" viewBox="0 0 1024 1024" aria-hidden="true">
      <defs>
        <linearGradient id="ntp-bg" x1="0.1" y1="0" x2="0.9" y2="1">
          <stop offset="0" stop-color="#5B6CFF" />
          <stop offset="0.55" stop-color="#8B5CF6" />
          <stop offset="1" stop-color="#E5489E" />
        </linearGradient>
      </defs>
      <rect x="64" y="64" width="896" height="896" rx="224" fill="url(#ntp-bg)" />
      <circle cx="512" cy="356" r="96" fill="#fff" />
      <path d="M236 726 L512 486 L788 726" fill="none" stroke="#fff" stroke-width="100" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
    <h1>{greeting}</h1>
    {/if}

    <button class="search" onclick={() => search()}>
      <Icon name="search" size={18} />
      <span>Search {engine.name} or enter an address</span>
      <span class="kbd">Ctrl L</span>
    </button>

    {#if essentials.length || top.length}
      <div class="tiles">
        {#each essentials as e (e.id)}
          <button class="tile" onclick={() => browser.activate(e.id)} title={titleFor(e.url, e.title)}>
            <span class="tile-icon"><Favicon url={e.url} favicon={e.favicon} size={22} /></span>
            <span class="tile-label ellipsis">{titleFor(e.url, e.title)}</span>
          </button>
        {/each}
        {#each top as t (t.id)}
          <button class="tile" onclick={() => open(t.url)} title={t.title || t.url}>
            <span class="tile-icon letter" style:--h={hue(hostOf(t.url))}>{displayHost(t.url)[0]?.toUpperCase()}</span>
            <span class="tile-label ellipsis">{displayHost(t.url)}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
  <div class="foot">
    <span><span class="kbd">Alt</span> + click a link to Glance</span>
    <span><span class="kbd">Ctrl K</span> commands</span>
    <span><span class="kbd">Ctrl B</span> compact mode</span>
  </div>
</div>

<style>
  .ntp {
    flex: 1;
    min-width: 0;
    height: 100%;
    min-height: 420px;
    display: flex;
    flex-direction: column;
    background:
      radial-gradient(1200px 600px at 50% -10%, color-mix(in srgb, var(--accent) 16%, transparent), transparent 70%),
      var(--page-bg);
    user-select: none;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
    padding: 40px 24px;
  }
  .logo {
    width: 64px;
    height: 64px;
    filter: drop-shadow(0 10px 24px color-mix(in srgb, var(--accent) 35%, transparent));
  }
  .incognito-mark {
    width: 64px;
    height: 64px;
    display: grid;
    place-items: center;
    border-radius: 20px;
    color: #fff;
    background: linear-gradient(160deg, color-mix(in srgb, var(--accent) 85%, #fff), color-mix(in srgb, var(--accent) 70%, #000));
    box-shadow: 0 10px 24px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .incognito-note {
    max-width: 520px;
    margin: -8px 0 6px;
    text-align: center;
    font-size: 13px;
    line-height: 1.55;
    color: var(--text-2);
  }
  h1 {
    margin: 0 0 6px;
    font: 600 28px/1.2 var(--font-display);
    letter-spacing: -0.01em;
  }
  .search {
    width: min(620px, 100%);
    height: 52px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 16px 0 18px;
    border-radius: 26px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    color: var(--text-3);
    font-size: 15px;
    text-align: left;
    transition: transform var(--fast) var(--ease), box-shadow var(--fast);
  }
  .search span:nth-child(2) {
    flex: 1;
  }
  .search:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow-lg);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, 92px);
    justify-content: center;
    gap: 10px;
    width: min(720px, 100%);
    margin-top: 10px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 6px 10px;
    border-radius: var(--radius-lg);
    transition: background var(--fast);
  }
  .tile:hover {
    background: var(--hover);
  }
  .tile-icon {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 14px;
    background: var(--field);
    border: 1px solid var(--line);
  }
  .tile-icon.letter {
    font-weight: 700;
    font-size: 19px;
    color: #fff;
    background: hsl(var(--h) 55% 48%);
    border: 0;
  }
  .tile-label {
    max-width: 84px;
    font-size: 12px;
    color: var(--text-2);
  }
  .foot {
    display: flex;
    justify-content: center;
    gap: 22px;
    padding: 14px;
    font-size: 12px;
    color: var(--text-3);
  }
  .foot .kbd {
    margin-right: 2px;
  }
</style>
