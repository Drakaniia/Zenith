<script lang="ts">
  import { flip } from 'svelte/animate';
  import { cubicOut } from 'svelte/easing';
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { startPage } from './startpage.svelte';
  import { MAX_SAVED_SITES, TILE_METRICS, composeTiles, type SavedSite } from './startpage';
  import { prefersReducedMotion, revealTransition } from './reveal';
  import { historyApi } from '$features/history/api';
  import { incognito } from '$features/window/window.svelte';
  import { SEARCH_ENGINES, displayHost, hostOf, titleFor } from '$shared/url';
  import type { HistoryEntry } from '$features/history/types';
  import type { Tab } from '$features/browser/types';

  // The new-tab page's Tableau: the user's saved sites, then Essentials and most visited sites.
  // `open` opens a site in the page's tab (or a new one).
  let { open }: { open: (url: string) => void } = $props();

  const p = $derived(startPage.prefs);
  let top = $state<HistoryEntry[]>([]);
  let topLoaded = false;
  const essentials = $derived(browser.essentials.map((id) => browser.tabs[id]).filter(Boolean));
  const metrics = $derived(TILE_METRICS[p.tileSize]);

  // The Tableau: the user's saved sites, then Essentials and most visited sites.
  type Tile =
    | { key: string; kind: 'saved'; site: SavedSite }
    | { key: string; kind: 'essential'; tab: Tab }
    | { key: string; kind: 'top'; entry: HistoryEntry };
  const tiles = $derived.by(() => {
    const c = composeTiles(
      p.showSaved ? p.sites : [],
      p.showEssentials ? essentials : [],
      // Incognito windows record no history, so there are no most visited sites to show.
      settings.value.showTopSites && !incognito ? top : [],
      p.hiddenSites,
      p.tileCount,
    );
    return [
      ...c.saved.map((site): Tile => ({ key: `s-${site.id}`, kind: 'saved', site })),
      ...c.essentials.map((tab): Tile => ({ key: `e-${tab.id}`, kind: 'essential', tab })),
      ...c.top.map((entry): Tile => ({ key: `h-${entry.id}`, kind: 'top', entry })),
    ];
  });
  // Incognito windows show the saved sites but don't change them.
  const canEdit = !incognito;
  const canAdd = $derived(canEdit && p.showSaved && p.sites.length < MAX_SAVED_SITES);

  // Saved sites are reordered by dragging one onto another.
  let dragId = $state<string | null>(null);
  let dropId = $state<string | null>(null);
  function onDragStart(e: DragEvent, site: SavedSite) {
    dragId = site.id;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/uri-list', site.url);
      e.dataTransfer.setData('text/plain', site.url);
    }
  }
  function onDragOver(e: DragEvent, site: SavedSite) {
    if (!dragId) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    dropId = site.id;
  }
  function onDrop(e: DragEvent, index: number) {
    if (!dragId) return;
    e.preventDefault();
    startPage.moveSite(dragId, index);
    dragId = dropId = null;
  }
  function onDragEnd() {
    dragId = dropId = null;
  }

  $effect(() => {
    if (!settings.value.showTopSites || incognito || topLoaded) return;
    topLoaded = true;
    void historyApi
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
          .slice(0, 40);
      })
      .catch(() => {});
  });

  function hue(text: string) {
    let h = 0;
    for (const c of text) h = (h * 31 + c.charCodeAt(0)) % 360;
    return h;
  }

  const reduced = prefersReducedMotion();
  const reveal = revealTransition(reduced);
</script>

{#if p.showTiles && (tiles.length || canAdd)}
  <div class="block tiles" class:no-labels={!p.tileLabels} role="list" aria-label="Tableau" transition:reveal>
    {#each tiles as t, i (t.key)}
      <div
        class="tile-wrap"
        class:dragging={t.kind === 'saved' && dragId === t.site.id}
        class:drop={t.kind === 'saved' && dropId === t.site.id && dragId !== t.site.id}
        role="listitem"
        draggable={t.kind === 'saved' && canEdit}
        ondragstart={(e) => t.kind === 'saved' && canEdit && onDragStart(e, t.site)}
        ondragover={(e) => t.kind === 'saved' && onDragOver(e, t.site)}
        ondrop={(e) => t.kind === 'saved' && onDrop(e, i)}
        ondragend={onDragEnd}
        animate:flip={{ duration: reduced ? 0 : 220, easing: cubicOut }}
      >
        {#if t.kind === 'saved'}
          {@const s = t.site}
          {@const name = s.title || displayHost(s.url)}
          <button class="tile" onclick={() => open(s.url)} title={`${name}\n${s.url}`}>
            <span class="tile-icon letter" style:--h={hue(hostOf(s.url))}>{name[0]?.toUpperCase()}</span>
            {#if p.tileLabels}<span class="tile-label ellipsis">{name}</span>{/if}
          </button>
          {#if canEdit}
            <span class="tile-actions">
              <button type="button" aria-label="Edit {name}" title="Edit" onclick={() => (startPage.editing = { id: s.id, url: s.url, title: s.title })}>
                <Icon name="edit" size={11} stroke={2.4} />
              </button>
              <button type="button" aria-label="Remove {name}" title="Remove" onclick={() => startPage.removeSite(s.id)}>
                <Icon name="close" size={11} stroke={2.6} />
              </button>
            </span>
          {/if}
        {:else if t.kind === 'essential'}
          {@const e = t.tab}
          <button class="tile" onclick={() => browser.activate(e.id)} title={titleFor(e.url, e.title)}>
            <span class="tile-icon"><Favicon url={e.url} favicon={e.favicon} size={metrics.favicon} /></span>
            {#if p.tileLabels}<span class="tile-label ellipsis">{titleFor(e.url, e.title)}</span>{/if}
          </button>
        {:else}
          {@const s = t.entry}
          {@const name = displayHost(s.url)}
          <button class="tile" onclick={() => open(s.url)} title={s.title || s.url}>
            <span class="tile-icon letter" style:--h={hue(hostOf(s.url))}>{name[0]?.toUpperCase()}</span>
            {#if p.tileLabels}<span class="tile-label ellipsis">{name}</span>{/if}
          </button>
          {#if canEdit}
            <span class="tile-actions">
              {#if canAdd}
                <button type="button" aria-label="Save {name} to Tableau" title="Save to Tableau" onclick={() => (startPage.editing = { url: s.url, title: '' })}>
                  <Icon name="pin" size={11} stroke={2.4} />
                </button>
              {/if}
              <button type="button" aria-label="Hide {name}" title="Don't show this site" onclick={() => startPage.hideSite(s.url)}>
                <Icon name="close" size={11} stroke={2.6} />
              </button>
            </span>
          {/if}
        {/if}
      </div>
    {/each}
    {#if canAdd}
      <div class="tile-wrap" role="listitem">
        <button class="tile add" onclick={() => (startPage.editing = { url: '', title: '' })} title="Add a site">
          <span class="tile-icon add-icon"><Icon name="plus" size={Math.round(metrics.icon * 0.4)} /></span>
          {#if p.tileLabels}<span class="tile-label ellipsis">Add site</span>{/if}
        </button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .block {
    margin-block: 9px;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, var(--tile-w));
    justify-content: center;
    gap: 10px;
    width: min(calc(var(--tile-w) * 8 + 70px), 100%);
    margin-top: 19px;
  }
  .tile-wrap {
    position: relative;
    border-radius: var(--radius-lg);
    transition:
      opacity 140ms ease,
      box-shadow 140ms ease;
  }
  .tile-wrap.dragging {
    opacity: 0.4;
  }
  .tile-wrap.drop {
    box-shadow: inset 0 0 0 2px var(--accent-button);
  }
  .tile {
    width: 100%;
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
  .tile-icon.add-icon {
    color: var(--text-2);
    background: transparent;
    border: 1.5px dashed var(--line-strong);
    transition:
      color var(--fast),
      border-color var(--fast),
      width 200ms var(--ease-out),
      height 200ms var(--ease-out);
  }
  .tile.add:hover .add-icon {
    color: var(--text);
    border-color: var(--text-3);
  }

  /* Edit / remove buttons in the tile's corner, shown on hover or keyboard focus. */
  .tile-actions {
    position: absolute;
    top: 3px;
    right: 3px;
    display: flex;
    gap: 2px;
    opacity: 0;
    transform: scale(0.9);
    transition:
      opacity 120ms ease,
      transform 120ms var(--ease-out);
  }
  .tile-wrap:hover .tile-actions,
  .tile-actions:focus-within {
    opacity: 1;
    transform: none;
  }
  .tile-wrap.dragging .tile-actions {
    opacity: 0;
  }
  .tile-actions button {
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text-2);
    background: var(--dialog-bg);
    border: 1px solid var(--line-strong);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.18);
  }
  .tile-actions button:hover {
    color: var(--text);
  }
  .tile-actions button:focus-visible {
    outline: 2px solid var(--accent-button);
    outline-offset: 1px;
  }

  /* Over a background picture (NewTab's `.walled` / `[data-text]` root): tile icons sit on solid
   * chips, and labels switch to whichever colour reads on the picture. */
  :global(.walled) .tile-icon:not(.letter) {
    background: color-mix(in srgb, var(--dialog-bg) 86%, transparent);
    border-color: transparent;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  }
  :global(.walled) .tile-icon.letter {
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
  }
  :global(.ntp[data-text='light']) .tile-label {
    color: rgba(255, 255, 255, 0.94);
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.5);
  }
  :global(.ntp[data-text='dark']) .tile-label {
    color: rgba(16, 16, 22, 0.88);
    text-shadow: 0 1px 2px rgba(255, 255, 255, 0.45);
  }
  :global(.ntp[data-text='light']) .tile:hover {
    background: rgba(255, 255, 255, 0.14);
  }
  :global(.ntp[data-text='light']) .add-icon {
    color: rgba(255, 255, 255, 0.9);
    border-color: rgba(255, 255, 255, 0.55);
  }
  :global(.ntp[data-text='dark']) .add-icon {
    color: rgba(16, 16, 22, 0.75);
    border-color: rgba(16, 16, 22, 0.35);
  }
  :global(.ntp[data-text='dark']) .tile:hover {
    background: rgba(0, 0, 0, 0.07);
  }
</style>
