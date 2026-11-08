<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import Segmented from '$shared/ui/Segmented.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { startPage } from './startpage.svelte';
  import { SEARCH_ENGINES } from '$shared/url';
  import { MAX_SAVED_SITES, TILE_COUNTS, type StartPageSettings, type TileSize } from './startpage';

  // The Layout tab of the start page's customization panel (StartPagePanel): which blocks the page
  // shows, the search bar, the Tableau and the tile size.
  let { onclose }: { onclose: () => void } = $props();

  const p = $derived(startPage.prefs);

  function set<K extends keyof StartPageSettings>(key: K, value: StartPageSettings[K]) {
    startPage.set({ [key]: value } as Partial<StartPageSettings>);
  }

  const sizes: { value: TileSize; label: string; px: number }[] = [
    { value: 'small', label: 'Small', px: 12 },
    { value: 'medium', label: 'Medium', px: 16 },
    { value: 'large', label: 'Large', px: 21 },
  ];
</script>

<section>
  <h3>Main page display</h3>
  <div class="row"><span>Greeting</span><Toggle checked={p.showGreeting} label="Greeting" onchange={(v) => set('showGreeting', v)} /></div>
  <div class="row"><span>Search bar</span><Toggle checked={p.showSearch} label="Search bar" onchange={(v) => set('showSearch', v)} /></div>
  <div class="row"><span>Tableau</span><Toggle checked={p.showTiles} label="Tableau" onchange={(v) => set('showTiles', v)} /></div>
  <div class="row"><span>Keyboard hints</span><Toggle checked={p.showHints} label="Keyboard hints" onchange={(v) => set('showHints', v)} /></div>
  <div class="row">
    <span>Position</span>
    <Segmented
      value={p.align}
      options={[
        { value: 'center', label: 'Center' },
        { value: 'top', label: 'Top' },
      ]}
      onchange={(v) => set('align', v as StartPageSettings['align'])}
    />
  </div>
  <div class="row">
    <span class="text"><b>Clear on hover</b><small>Hover over the settings button for 1 second to hide everything. Move away to bring it back.</small></span>
    <Toggle checked={p.hoverToHide} label="Clear on hover" onchange={(v) => set('hoverToHide', v)} />
  </div>
</section>

<section class:off={!p.showSearch}>
  <h3>Search bar</h3>
  <div class="row">
    <span>Width</span>
    <Segmented
      value={p.searchWidth}
      options={[
        { value: 'compact', label: 'Compact' },
        { value: 'wide', label: 'Wide' },
      ]}
      onchange={(v) => set('searchWidth', v as StartPageSettings['searchWidth'])}
    />
  </div>
  <div class="row">
    <span>Search with</span>
    <select
      class="field select"
      aria-label="Search engine"
      value={settings.value.searchEngine}
      onchange={(e) => settings.update({ searchEngine: e.currentTarget.value })}
    >
      {#each Object.entries(SEARCH_ENGINES) as [id, e] (id)}<option value={id}>{e.name}</option>{/each}
    </select>
  </div>
</section>

<section class:off={!p.showTiles}>
  <h3>Tableau · saved websites</h3>
  <div class="row">
    <span class="text"><b>Saved sites</b><small>{p.sites.length} of {MAX_SAVED_SITES} · drag tiles to reorder</small></span>
    <Toggle checked={p.showSaved} label="Saved sites" onchange={(v) => set('showSaved', v)} />
  </div>
  <div class="row"><span>Essentials</span><Toggle checked={p.showEssentials} label="Essentials" onchange={(v) => set('showEssentials', v)} /></div>
  <div class="row">
    <span>Most visited</span>
    <Toggle checked={settings.value.showTopSites} label="Most visited" onchange={(v) => settings.update({ showTopSites: v })} />
  </div>
  <div class="row"><span>Site names</span><Toggle checked={p.tileLabels} label="Site names" onchange={(v) => set('tileLabels', v)} /></div>
  <div class="row">
    <span>Show up to</span>
    <Segmented value={p.tileCount} options={TILE_COUNTS.map((n) => ({ value: n, label: String(n) }))} onchange={(v) => set('tileCount', v)} />
  </div>
  {#if p.hiddenSites.length}
    <div class="row">
      <span class="text"><b>Hidden sites</b><small>{p.hiddenSites.length} removed from most visited</small></span>
      <button type="button" class="btn ghost sm" onclick={() => startPage.restoreHidden()}>Restore</button>
    </div>
  {/if}
  <button
    type="button"
    class="btn ghost add-site"
    disabled={p.sites.length >= MAX_SAVED_SITES}
    onclick={() => {
      onclose();
      startPage.editing = { url: '', title: '' };
    }}
  >
    <Icon name="plus" size={14} /> Add a site
  </button>
</section>

<section class:off={!p.showTiles}>
  <h3>Tile size</h3>
  <div class="sizes" role="radiogroup" aria-label="Tile size">
    {#each sizes as s (s.value)}
      <button type="button" role="radio" aria-checked={p.tileSize === s.value} class:on={p.tileSize === s.value} onclick={() => set('tileSize', s.value)}>
        <span class="size-box" style:--px="{s.px}px"></span>
        {s.label}
      </button>
    {/each}
  </div>
</section>

<button type="button" class="btn ghost reset" onclick={() => startPage.resetLayout()}>Reset layout</button>

<style>
  section {
    padding: 10px 0 4px;
    transition: opacity 150ms ease;
  }
  section + section {
    border-top: 1px solid var(--line);
  }
  section.off {
    opacity: 0.5;
  }
  h3 {
    margin: 2px 0 4px;
    font: 600 12px/1.3 var(--font);
    color: var(--text-3);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    min-height: 36px;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .text b {
    font-weight: 500;
  }
  .text small {
    font-size: 11.5px;
    line-height: 1.35;
    color: var(--text-3);
  }
  .select {
    height: 30px;
    max-width: 150px;
    padding: 0 8px;
    font-size: 12.5px;
  }
  .sizes {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
    padding: 4px 0 6px;
  }
  .sizes button {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    gap: 7px;
    height: 66px;
    padding-bottom: 8px;
    border-radius: 10px;
    font-size: 12px;
    color: var(--text-2);
    border: 1px solid var(--line);
    transition:
      background 120ms ease,
      border-color 120ms ease,
      color 120ms ease,
      transform 120ms var(--ease-out);
  }
  .sizes button:hover {
    background: var(--hover);
  }
  .sizes button:active {
    transform: scale(0.97);
  }
  .sizes button.on {
    color: var(--text);
    border-color: color-mix(in srgb, var(--accent-button) 70%, transparent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .size-box {
    width: var(--px);
    height: var(--px);
    border-radius: calc(var(--px) / 3.4);
    background: var(--text-3);
  }
  .sizes button.on .size-box {
    background: var(--accent-button);
  }
  .add-site {
    width: 100%;
    height: 30px;
    margin: 4px 0 6px;
    justify-content: center;
    gap: 6px;
    color: var(--text-2);
    border: 1px dashed var(--line-strong);
  }
  .add-site:hover:not(:disabled) {
    color: var(--text);
  }
  .reset {
    margin-top: 6px;
    width: 100%;
    justify-content: center;
    height: 30px;
    color: var(--text-2);
  }
  .btn.sm {
    height: 28px;
    padding: 0 11px;
    font-size: 12.5px;
  }
</style>
