<script lang="ts">
  import { fly } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { overlayLease, overlaySurface } from '$platform/layout.svelte';
  import { security } from './security.svelte';

  const item = $derived(security.queue.find((x) => browser.isVisible(x.data.tabId)) ?? security.queue[0] ?? null);
  let showAdvanced = $state(false);

  $effect(() => {
    void item;
    showAdvanced = false;
  });
</script>

{#if item}
  <div class="prompt glass" use:overlayLease use:overlaySurface={12} transition:fly={{ y: -10, duration: 160 }} role="alertdialog" aria-label="Security warning">
    <div class="icon danger"><Icon name="shield" size={20} /></div>
    <div class="body">
      {#if item.kind === 'cert'}
        <p class="title">Your connection to <strong>{item.data.host || item.data.url}</strong> is not private</p>
        <p class="sub">Certificate verification failed (<code>{item.data.error}</code>). Attackers could be trying to steal your information.</p>
      {:else if item.kind === 'safebrowsing'}
        <p class="title">Dangerous site blocked: <strong>{item.data.host}</strong></p>
        <p class="sub">{item.data.category}. Checked privately against Zenith's local threat database.</p>
      {:else if item.kind === 'protocol'}
        <p class="title">Open external application for <code>{item.data.scheme}:</code>?</p>
        <p class="sub url">{item.data.url}</p>
      {:else}
        <p class="title">Dangerous download: <strong>{item.data.fileName}</strong></p>
        <p class="sub">{item.data.reason}.</p>
      {/if}
    </div>
    <div class="actions">
      {#if item.kind === 'cert' || item.kind === 'safebrowsing'}
        {#if !showAdvanced}
          <button class="btn ghost" onclick={() => (showAdvanced = true)}>Advanced</button>
        {:else}
          <button class="btn danger-btn" onclick={() => security.answer(item, true)}>Proceed anyway (unsafe)</button>
        {/if}
        <button class="btn primary" onclick={() => security.answer(item, false)}>Back to safety</button>
      {:else if item.kind === 'protocol'}
        <button class="btn" onclick={() => security.answer(item, true)}>Open {item.data.scheme}</button>
        <button class="btn primary" onclick={() => security.answer(item, false)}>Cancel</button>
      {:else}
        <button class="btn danger-btn" onclick={() => security.answer(item, true)}>Keep anyway</button>
        <button class="btn primary" onclick={() => security.answer(item, false)}>Discard</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .prompt {
    position: fixed;
    top: calc(var(--topbar) + 14px);
    left: calc(var(--sidebar-w) + 16px);
    z-index: 82;
    width: 440px;
    max-width: calc(100vw - 32px);
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 12px 14px;
    padding: 16px;
    border-radius: var(--radius-xl);
    border: 1px solid color-mix(in srgb, var(--danger) 40%, var(--line));
  }
  .icon.danger {
    width: 40px;
    height: 40px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--danger) 18%, transparent);
    color: var(--danger);
  }
  .title {
    margin: 0 0 6px;
    line-height: 1.4;
  }
  .sub {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .url {
    word-break: break-all;
    font-family: var(--font-mono, monospace);
  }
  .actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .danger-btn {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 40%, var(--line));
  }
</style>
