<!-- The downloads popup that opens from the downloads button, modelled on Zen's (Firefox's)
     downloads panel: the latest files with progress, speed and time left, a right-click menu on
     each, and "Show all downloads" at the bottom. -->
<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { formatBytes } from '$shared/format';
  import { overlayLease } from '$platform/layout.svelte';
  import { openInternal } from '$features/browser/target';
  import DownloadRow from './DownloadRow.svelte';
  import PopoverRow from './PopoverRow.svelte';
  import { downloads } from './downloads.svelte';

  const SHOWN = 8;
  const WIDTH = 380;
  const GAP = 8;

  const recent = $derived(downloads.items.slice(0, SHOWN));
  const finished = $derived(downloads.items.some((d) => d.state !== 'progress'));
  const summary = $derived.by(() => {
    const n = downloads.active.length;
    if (!n) return '';
    const rate = downloads.totalRate > 0 ? ` · ${formatBytes(downloads.totalRate)}/s` : '';
    return `${n === 1 ? '1 file' : `${n} files`} downloading${rate}`;
  });

  /** Next to the downloads button, wherever the sidebar or toolbar put it. */
  let place = $state<{ left?: string; right?: string; top?: string; bottom?: string }>({ left: '10px', bottom: '52px' });
  $effect(() => {
    if (!ui.downloadsOpen) return;
    const anchor = [...document.querySelectorAll<HTMLElement>('[data-downloads-anchor]')].find((el) => el.offsetParent);
    if (!anchor) return;
    const r = anchor.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const left = Math.max(GAP, Math.min(r.left, vw - WIDTH - GAP));
    const x = r.left + r.width / 2 > vw / 2 ? { right: `${Math.max(GAP, vw - r.right)}px` } : { left: `${left}px` };
    const y = r.top > vh / 2 ? { bottom: `${vh - r.top + GAP}px` } : { top: `${r.bottom + GAP}px` };
    place = { ...x, ...y };
  });

  function close() {
    ui.downloadsOpen = false;
  }

  function clear() {
    downloads.remove(null).catch((e) => ui.toast(String(e), { icon: 'alert' }));
  }

  function showAll() {
    close();
    openInternal('downloads');
  }

  function onKey(e: KeyboardEvent) {
    if (ui.downloadsOpen && e.key === 'Escape' && !ui.menu) {
      e.preventDefault();
      close();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if ui.downloadsOpen}
  <div class="root" use:overlayLease>
    <div class="catcher" role="presentation" transition:fade={{ duration: 100 }} onpointerdown={close}></div>
    <div
      class="pop glass"
      style:left={place.left}
      style:right={place.right}
      style:top={place.top}
      style:bottom={place.bottom}
      style:width="{WIDTH}px"
      transition:fly={{ y: place.top ? -8 : 8, duration: 160 }}
      role="dialog"
      aria-label="Downloads"
    >
      <div class="head">
        <div class="title">
          <h3>Downloads</h3>
          {#if summary}<span class="summary">{summary}</span>{/if}
        </div>
        {#if finished}
          <button class="btn ghost sm" title="Remove finished downloads from the list" onclick={clear}>Clear</button>
        {/if}
      </div>
      {#if recent.length}
        <div class="list" role="list">
          {#each recent as d (d.id)}
            {#if d.kind === 'torrent' || d.live}
              <DownloadRow {d} compact />
            {:else}
              <PopoverRow {d} />
            {/if}
          {/each}
        </div>
      {:else}
        <div class="empty">
          <span class="empty-icon"><Icon name="download" size={22} /></span>
          <p>No downloads yet</p>
          <span class="hint">Files you download appear here</span>
        </div>
      {/if}
      <button class="all" onclick={showAll}>
        Show all downloads
        {#if downloads.items.length > SHOWN}<span class="count">{downloads.items.length}</span>{/if}
      </button>
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 85;
  }
  .catcher {
    position: absolute;
    inset: 0;
  }
  .pop {
    position: absolute;
    max-width: calc(100vw - 16px);
    max-height: calc(100vh - 80px);
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-xl);
    padding: 6px;
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 6px 8px 10px;
  }
  .title {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font: 600 14px/1.2 var(--font-display);
  }
  .summary {
    font-size: 11.5px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .sm {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    min-height: 0;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 22px 0 26px;
    color: var(--text-3);
  }
  .empty-icon {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    margin-bottom: 6px;
    border-radius: 50%;
    background: var(--field);
    color: var(--text-2);
  }
  .empty p {
    margin: 0;
    color: var(--text-2);
    font-weight: 500;
  }
  .hint {
    font-size: 12px;
  }
  /* Firefox's footer button: the whole width, set off by a line. */
  .all {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin-top: 6px;
    padding: 9px;
    border-top: 1px solid var(--line);
    border-radius: 0 0 var(--radius) var(--radius);
    font-size: 12.5px;
    color: var(--text-2);
  }
  .all:hover {
    background: var(--hover);
    color: var(--text);
  }
  .count {
    padding: 0 6px;
    border-radius: 8px;
    background: var(--field);
    font-size: 11px;
    line-height: 16px;
    font-variant-numeric: tabular-nums;
  }
</style>
