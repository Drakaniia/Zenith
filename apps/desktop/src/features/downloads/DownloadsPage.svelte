<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import DownloadRow from './DownloadRow.svelte';
  import AddTorrent from './AddTorrent.svelte';
  import { downloads } from './downloads.svelte';
  import { torrents } from './torrents.svelte';
  import { findMagnet } from './torrent';
  import { settings } from '$features/settings/settings.svelte';
  import { appInfo } from '$features/window/window.svelte';
  import { shell } from '$platform/shell';

  let query = $state('');
  let adding = $state(false);
  /** A file or link is being dragged over the page. */
  let dropping = $state(false);
  let dragDepth = 0;
  onMount(() => void downloads.load());

  const torrentsOn = $derived(settings.value.torrentsEnabled);
  const folder = $derived(settings.value.downloadDir ?? appInfo.info?.defaultDownloadDir ?? '');
  const visible = $derived(
    downloads.items.filter(
      (d) => !query || (d.fileName ?? d.path).toLowerCase().includes(query.toLowerCase()) || d.url.toLowerCase().includes(query.toLowerCase()),
    ),
  );

  /** Dragged files or links (not text selected on this page). */
  function droppable(e: DragEvent) {
    const types = e.dataTransfer?.types ?? [];
    return torrentsOn && (types.includes('Files') || types.includes('text/uri-list'));
  }

  function onDragEnter(e: DragEvent) {
    if (!droppable(e)) return;
    e.preventDefault();
    dragDepth++;
    dropping = true;
  }

  function onDragLeave() {
    dragDepth = Math.max(0, dragDepth - 1);
    if (!dragDepth) dropping = false;
  }

  async function onDrop(e: DragEvent) {
    if (!droppable(e)) return;
    e.preventDefault();
    dragDepth = 0;
    dropping = false;
    const dt = e.dataTransfer;
    if (!dt) return;
    const files = [...dt.files];
    if (files.length) return torrents.addFiles(files);
    const text = dt.getData('text/uri-list') || dt.getData('text/plain');
    if (findMagnet(text)) await torrents.addMagnet(text);
  }
</script>

<div
  class="page"
  role="region"
  aria-label="Downloads"
  ondragenter={onDragEnter}
  ondragover={(e) => {
    if (droppable(e)) e.preventDefault();
  }}
  ondragleave={onDragLeave}
  ondrop={(e) => void onDrop(e)}
>
  <div class="inner">
    <header>
      <h1>Downloads</h1>
      <div class="tools">
        {#if torrentsOn}<button class="btn" class:on={adding} onclick={() => (adding = !adding)}><Icon name="magnet" size={15} />Add torrent</button>{/if}
        {#if folder}<button class="btn" onclick={() => void shell.openPath(folder)}><Icon name="folder" size={15} />Open folder</button>{/if}
        <button class="btn" onclick={() => downloads.remove(null)} disabled={!downloads.items.some((d) => d.state !== 'progress')}>
          <Icon name="eraser" size={15} />Clear list
        </button>
      </div>
    </header>
    {#if adding && torrentsOn}
      <AddTorrent onclose={() => (adding = false)} />
    {/if}
    <div class="search">
      <Icon name="search" size={16} />
      <input placeholder="Search downloads" bind:value={query} spellcheck="false" />
    </div>
    <div class="list">
      {#each visible as d (d.id)}
        <DownloadRow {d} />
      {:else}
        <div class="empty"><Icon name="download" size={28} /><p>{query ? 'No matching downloads' : 'Files you download will appear here'}</p></div>
      {/each}
    </div>
  </div>
  {#if dropping}
    <div class="drop"><Icon name="magnet" size={28} /><p>Drop a .torrent file or magnet link to download it</p></div>
  {/if}
</div>

<style>
  .page {
    position: relative;
    height: 100%;
    overflow-y: auto;
  }
  .inner {
    max-width: 860px;
    margin: 0 auto;
    padding: 28px 32px 60px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h1 {
    margin: 0;
    font: 600 22px/1.2 var(--font-display);
  }
  .tools {
    display: flex;
    gap: 8px;
  }
  .tools .btn.on {
    background: var(--hover);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    margin: 18px 0 10px;
    padding: 0 14px;
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-3);
  }
  .search input {
    flex: 1;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 14px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 80px 0;
    color: var(--text-3);
  }
  .drop {
    position: absolute;
    inset: 12px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    border: 2px dashed var(--accent);
    border-radius: var(--radius-xl);
    background: color-mix(in srgb, var(--accent) 8%, var(--bg, transparent));
    color: var(--text-2);
    pointer-events: none;
  }
  .drop p {
    margin: 0;
  }
</style>
