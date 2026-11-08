<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import DownloadRow from './DownloadRow.svelte';
  import { downloads } from './downloads.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { overlayLease } from '$platform/layout.svelte';
  import { openInternal } from '$features/browser/target';

  const recent = $derived(downloads.items.slice(0, 6));
</script>

{#if ui.downloadsOpen}
  <div class="root" use:overlayLease>
    <div class="catcher" role="presentation" transition:fade={{ duration: 100 }} onpointerdown={() => (ui.downloadsOpen = false)}></div>
    <div class="pop glass" transition:fly={{ y: 10, duration: 160 }} role="dialog" aria-label="Downloads">
      <div class="head">
        <h3>Downloads</h3>
        <button
          class="btn ghost"
          onclick={() => {
            ui.downloadsOpen = false;
            openInternal('downloads');
          }}>See all</button
        >
      </div>
      {#if recent.length}
        <div class="list">
          {#each recent as d (d.id)}
            <DownloadRow {d} compact />
          {/each}
        </div>
      {:else}
        <div class="empty">
          <Icon name="download" size={22} />
          <p>Files you download appear here</p>
        </div>
      {/if}
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
    left: 10px;
    bottom: 52px;
    width: 360px;
    border-radius: var(--radius-xl);
    padding: 10px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 4px 8px 8px;
  }
  h3 {
    margin: 0;
    font: 600 14px/1 var(--font-display);
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
    gap: 6px;
    padding: 26px 0 30px;
    color: var(--text-3);
  }
  .empty p {
    margin: 0;
  }
</style>
