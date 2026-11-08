<script lang="ts">
  import { untrack } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Segmented from '$shared/ui/Segmented.svelte';
  import { startPage } from './startpage.svelte';
  import LayoutOptions from './LayoutOptions.svelte';
  import WallpaperPicker from './WallpaperPicker.svelte';

  // The start page's customization panel: a Layout tab (LayoutOptions) and a Background tab
  // (WallpaperPicker). The background being looked at and the upload live here, so they survive
  // switching tabs.
  let { onclose }: { onclose: () => void } = $props();

  const p = $derived(startPage.prefs);
  let tab = $state<'layout' | 'background'>('layout');

  // The background being looked at. It is only applied with "Use background".
  let candidate = $state(untrack(() => startPage.prefs.background));

  let fileInput: HTMLInputElement | undefined = $state();
  let uploading = $state(false);
  let error = $state('');

  $effect(() => {
    for (const c of p.customImages) startPage.ensureImage(c.id);
  });

  async function onFile() {
    const file = fileInput?.files?.[0];
    if (fileInput) fileInput.value = '';
    if (!file) return;
    uploading = true;
    error = '';
    try {
      candidate = await startPage.upload(file);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      uploading = false;
    }
  }

  async function remove(id: string) {
    if (candidate === `custom:${id}`) candidate = '';
    await startPage.removeImage(id);
  }
</script>

<div class="panel" role="dialog" aria-label="Customize start page">
  <header>
    <Segmented
      value={tab}
      options={[
        { value: 'layout', label: 'Layout' },
        { value: 'background', label: 'Background' },
      ]}
      onchange={(v) => (tab = v as typeof tab)}
    />
    <button type="button" class="icon-btn sm" aria-label="Close" title="Close" onclick={onclose}><Icon name="close" size={14} /></button>
  </header>

  <div class="body">
    {#if tab === 'layout'}
      <LayoutOptions {onclose} />
    {:else}
      <WallpaperPicker bind:candidate bind:fileInput {uploading} {error} onfile={onFile} onremove={remove} />
    {/if}
  </div>
</div>

<style>
  .panel {
    width: 340px;
    max-height: min(640px, calc(100vh - 140px));
    display: flex;
    flex-direction: column;
    border-radius: 14px;
    corner-shape: var(--sq);
    background: var(--dialog-bg);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow-lg);
    color: var(--text);
    font-size: 13px;
    overflow: hidden;
    user-select: none;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 10px 8px 12px;
    border-bottom: 1px solid var(--line);
  }
  .body {
    overflow-y: auto;
    padding: 2px 14px 14px;
    scrollbar-width: thin;
  }
</style>
