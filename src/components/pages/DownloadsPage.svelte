<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '../Icon.svelte';
  import DownloadRow from '../DownloadRow.svelte';
  import { downloads } from '../../lib/ui.svelte';
  import { settings } from '../../lib/settings.svelte';
  import { appInfo } from '../../lib/app.svelte';
  import { api } from '../../lib/ipc';

  let query = $state('');
  onMount(() => void downloads.load());

  const folder = $derived(settings.value.downloadDir ?? appInfo.info?.defaultDownloadDir ?? '');
  const visible = $derived(
    downloads.items.filter(
      (d) => !query || (d.fileName ?? d.path).toLowerCase().includes(query.toLowerCase()) || d.url.toLowerCase().includes(query.toLowerCase()),
    ),
  );
</script>

<div class="page">
  <div class="inner">
    <header>
      <h1>Downloads</h1>
      <div class="tools">
        {#if folder}<button class="btn" onclick={() => void api.openPath(folder)}><Icon name="folder" size={15} />Open folder</button>{/if}
        <button class="btn" onclick={() => downloads.remove(null)} disabled={!downloads.items.some((d) => d.state !== 'progress')}>
          <Icon name="eraser" size={15} />Clear list
        </button>
      </div>
    </header>
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
</div>

<style>
  .page {
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
</style>
