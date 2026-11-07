<script lang="ts">
  import { fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import Icon from '../Icon.svelte';
  import Favicon from '../Favicon.svelte';
  import StartPageControls from '../startpage/StartPageControls.svelte';
  import { browser } from '../../lib/browser.svelte';
  import { settings } from '../../lib/settings.svelte';
  import { startPage } from '../../lib/startpage.svelte';
  import { TILE_METRICS, greetingFor, textTone } from '../../lib/startpage';
  import { ui } from '../../lib/ui.svelte';
  import { api } from '../../lib/ipc';
  import { incognito } from '../../lib/app.svelte';
  import { SEARCH_ENGINES, displayHost, hostOf, titleFor } from '../../lib/url';
  import type { HistoryEntry, Tab } from '../../lib/types';

  // The new-tab page is drawn by the chrome UI (no webview, no network requests).
  let { tab }: { tab?: Tab } = $props();

  const p = $derived(startPage.prefs);
  let top = $state<HistoryEntry[]>([]);
  let topLoaded = false;
  const essentials = $derived(browser.essentials.map((id) => browser.tabs[id]).filter(Boolean));
  const engine = $derived(SEARCH_ENGINES[settings.value.searchEngine] ?? SEARCH_ENGINES.google);
  const metrics = $derived(TILE_METRICS[p.tileSize]);
  const greeting = greetingFor(new Date().getHours());

  type Tile = { key: string; essential: Tab; site?: undefined } | { key: string; essential?: undefined; site: HistoryEntry };
  const tiles = $derived.by(() => {
    const list: Tile[] = [];
    if (p.showEssentials) for (const e of essentials) list.push({ key: `e-${e.id}`, essential: e });
    // Incognito windows record no history, so there are no most visited sites to show.
    if (settings.value.showTopSites && !incognito) for (const t of top) list.push({ key: `h-${t.id}`, site: t });
    return list.slice(0, p.tileCount);
  });

  const background = $derived(startPage.resolve(p.background));
  const text = $derived(background ? textTone(background.tone, p.dim) : null);

  startPage.rollDaily();

  $effect(() => {
    if (!settings.value.showTopSites || incognito || topLoaded) return;
    topLoaded = true;
    void api
      .historyTop(80)
      .then((rows) => {
        const seen = new Set<string>();
        const engines = Object.values(SEARCH_ENGINES).map((e) => hostOf(e.home));
        top = rows
          .filter((r) => {
            const h = hostOf(r.url).replace(/^www\./, '');
            if (!h || seen.has(h) || engines.some((e) => e.endsWith(h))) return false;
            seen.add(h);
            return true;
          })
          .slice(0, 16);
      })
      .catch(() => {});
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

  // Blocks shown or hidden from the settings collapse (height, spacing) while fading, so the
  // rest of the page glides into place instead of jumping.
  const reduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  function reveal(node: HTMLElement) {
    const h = node.offsetHeight;
    return {
      duration: reduced ? 120 : 240,
      easing: cubicOut,
      css: (t: number) =>
        reduced
          ? `opacity:${t}`
          : `opacity:${t};height:${t * h}px;margin-block:${t * 9}px;overflow:clip;` +
            `transform:translateY(${(1 - t) * -6}px) scale(${0.97 + 0.03 * t});filter:blur(${(1 - t) * 4}px)`,
    };
  }

  // A cleared page comes back with any click on it or Escape.
  function restore(e: PointerEvent) {
    if (!startPage.cleared || (e.target as Element).closest('.sp-controls')) return;
    startPage.cleared = false;
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape' && startPage.cleared) startPage.cleared = false;
  }}
/>

<div
  class="ntp"
  class:walled={!!background}
  class:cleared={startPage.cleared}
  data-text={text}
  data-align={p.align}
  role="presentation"
  style:--tile-w="{metrics.tile}px"
  style:--tile-icon="{metrics.icon}px"
  style:--tile-r="{metrics.radius}px"
  style:--tile-letter="{metrics.letter}px"
  onpointerdown={restore}
>
  {#if background}
    {#key background.url}
      <div
        class="wall"
        style:background-image={`url("${background.url}")`}
        style:--blur={p.blur}
        transition:fade={{ duration: 420 }}
      ></div>
    {/key}
    <div class="scrim" style:opacity={p.dim / 100}></div>
  {/if}

  <div class="center" inert={startPage.cleared}>
    {#if incognito}
      <div class="block greet">
        <span class="incognito-mark"><Icon name="incognito" size={34} /></span>
        <h1>You're incognito</h1>
        <p class="incognito-note">
          Pages you open in this window aren't saved to your history, and their cookies and site data are deleted when
          you close it. Downloads and bookmarks you create are kept. Websites, your employer and your internet provider
          can still see what you visit.
        </p>
      </div>
    {:else if p.showGreeting}
      <div class="block greet" transition:reveal>
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
      </div>
    {/if}

    {#if p.showSearch}
      <button class="block search" class:compact={p.searchWidth === 'compact'} onclick={() => search()} transition:reveal>
        <Icon name="search" size={18} />
        <span class="search-text">Search {engine.name} or enter an address</span>
        <span class="kbd">Ctrl L</span>
      </button>
    {/if}

    {#if p.showTiles && tiles.length}
      <div class="block tiles" class:no-labels={!p.tileLabels} transition:reveal>
        {#each tiles as t (t.key)}
          {#if t.essential}
            {@const e = t.essential}
            <button class="tile" onclick={() => browser.activate(e.id)} title={titleFor(e.url, e.title)}>
              <span class="tile-icon"><Favicon url={e.url} favicon={e.favicon} size={metrics.favicon} /></span>
              {#if p.tileLabels}<span class="tile-label ellipsis">{titleFor(e.url, e.title)}</span>{/if}
            </button>
          {:else}
            {@const s = t.site}
            <button class="tile" onclick={() => open(s.url)} title={s.title || s.url}>
              <span class="tile-icon letter" style:--h={hue(hostOf(s.url))}>{displayHost(s.url)[0]?.toUpperCase()}</span>
              {#if p.tileLabels}<span class="tile-label ellipsis">{displayHost(s.url)}</span>{/if}
            </button>
          {/if}
        {/each}
      </div>
    {/if}
  </div>

  {#if p.showHints}
    <div class="foot" inert={startPage.cleared} transition:fade={{ duration: 160 }}>
      <span><span class="kbd">Alt</span> + click a link to Glance</span>
      <span><span class="kbd">Ctrl K</span> commands</span>
      <span><span class="kbd">Ctrl B</span> compact mode</span>
    </div>
  {/if}

  {#if !incognito}
    <StartPageControls />
  {/if}
</div>

<style>
  .ntp {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 100%;
    min-height: 420px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background:
      radial-gradient(1200px 600px at 50% -10%, color-mix(in srgb, var(--accent) 16%, transparent), transparent 70%),
      var(--page-bg);
    user-select: none;
  }

  /* Background picture. Blur would show soft edges, so a blurred picture overhangs the page. */
  .wall {
    position: absolute;
    inset: calc(var(--blur) * -2px);
    background: center / cover no-repeat;
    filter: blur(calc(var(--blur) * 1px));
    transition: filter 150ms ease;
  }
  .scrim {
    position: absolute;
    inset: 0;
    background: #000;
    pointer-events: none;
    transition: opacity 150ms ease;
  }

  .center {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 40px 24px 64px;
  }
  .ntp[data-align='top'] .center {
    justify-content: flex-start;
    padding-top: max(48px, 12vh);
  }
  .block {
    margin-block: 9px;
  }

  /* Cleared view: search, sites and hints step back so only the background is left. Hiding is
   * slower (the user is watching it happen); coming back is quick. */
  .center,
  .foot {
    transition:
      opacity 200ms var(--ease-out),
      transform 200ms var(--ease-out),
      filter 200ms var(--ease-out);
  }
  .ntp.cleared .center,
  .ntp.cleared .foot {
    opacity: 0;
    transform: scale(0.985);
    filter: blur(6px);
    pointer-events: none;
    transition-duration: 380ms;
  }

  .greet {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 18px;
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
    margin: -8px 0 0;
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
    width: min(640px, 100%);
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
    transition:
      transform var(--fast) var(--ease),
      box-shadow var(--fast),
      width 240ms var(--ease-out);
  }
  .search.compact {
    width: min(480px, 100%);
    height: 46px;
    font-size: 14px;
  }
  .search-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .search:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow-lg);
  }
  .search:focus-visible {
    outline: 2px solid var(--accent-button);
    outline-offset: 2px;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, var(--tile-w));
    justify-content: center;
    gap: 10px;
    width: min(calc(var(--tile-w) * 8 + 70px), 100%);
    margin-top: 19px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 6px 10px;
    border-radius: var(--radius-lg);
    transition:
      background var(--fast),
      transform 140ms var(--ease-out);
  }
  .tiles.no-labels .tile {
    padding: 10px 6px;
  }
  .tile:hover {
    background: var(--hover);
  }
  .tile:active {
    transform: scale(0.96);
  }
  .tile:focus-visible {
    outline: 2px solid var(--accent-button);
    outline-offset: -2px;
  }
  .tile-icon {
    width: var(--tile-icon);
    height: var(--tile-icon);
    display: grid;
    place-items: center;
    border-radius: var(--tile-r);
    background: var(--field);
    border: 1px solid var(--line);
    transition:
      width 200ms var(--ease-out),
      height 200ms var(--ease-out);
  }
  .tile-icon.letter {
    font-weight: 700;
    font-size: var(--tile-letter);
    color: #fff;
    background: hsl(var(--h) 55% 48%);
    border: 0;
  }
  .tile-label {
    max-width: calc(var(--tile-w) - 8px);
    font-size: 12px;
    color: var(--text-2);
  }

  .foot {
    position: relative;
    display: flex;
    justify-content: center;
    flex-wrap: wrap;
    gap: 6px 22px;
    padding: 14px 64px;
    font-size: 12px;
    color: var(--text-3);
  }
  .foot .kbd {
    margin-right: 2px;
  }

  /* Over a background picture: the search bar becomes the one frosted surface, tile icons sit
   * on solid chips, and loose text switches to whichever colour reads on the picture. */
  .walled .search {
    background: color-mix(in srgb, var(--dialog-bg) 80%, transparent);
    backdrop-filter: blur(18px) saturate(1.4);
    border-color: color-mix(in srgb, var(--text) 12%, transparent);
  }
  .walled .tile-icon:not(.letter) {
    background: color-mix(in srgb, var(--dialog-bg) 86%, transparent);
    border-color: transparent;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  }
  .walled .tile-icon.letter {
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
  }
  .ntp[data-text='light'] :is(h1, .tile-label, .foot, .incognito-note) {
    color: rgba(255, 255, 255, 0.94);
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.5);
  }
  .ntp[data-text='dark'] :is(h1, .tile-label, .foot, .incognito-note) {
    color: rgba(16, 16, 22, 0.88);
    text-shadow: 0 1px 2px rgba(255, 255, 255, 0.45);
  }
  .ntp[data-text='light'] .tile:hover {
    background: rgba(255, 255, 255, 0.14);
  }
  .ntp[data-text='dark'] .tile:hover {
    background: rgba(0, 0, 0, 0.07);
  }

  @media (prefers-reduced-motion: reduce) {
    .ntp.cleared .center,
    .ntp.cleared .foot {
      transform: none;
      filter: none;
    }
  }
</style>
