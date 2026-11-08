<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  // Windows caption buttons (min / max-restore / close), drawn like the native ones Zen keeps.
  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    void win.isMaximized().then((m) => (maximized = m));
    const un = win.onResized(async () => {
      maximized = await win.isMaximized();
    });
    return () => void un.then((f) => f());
  });
</script>

<div class="controls">
  <button class="ctl" title="Minimize" aria-label="Minimize" onclick={() => void win.minimize()}>
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
  </button>
  <button class="ctl" title={maximized ? 'Restore' : 'Maximize'} aria-label={maximized ? 'Restore' : 'Maximize'} onclick={() => void win.toggleMaximize()}>
    {#if maximized}
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
        <path d="M2.5 2.5V1.4c0-.5.4-.9.9-.9h5.2c.5 0 .9.4.9.9v5.2c0 .5-.4.9-.9.9H7.5" />
        <rect x="0.5" y="2.5" width="7" height="7" rx="0.9" />
      </svg>
    {:else}
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x="0.5" y="0.5" width="9" height="9" rx="1" /></svg>
    {/if}
  </button>
  <button class="ctl close" title="Close" aria-label="Close" onclick={() => void win.close()}>
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M.5.5l9 9M9.5.5l-9 9" /></svg>
  </button>
</div>

<style>
  .controls {
    display: flex;
    align-self: stretch;
    height: 100%;
    flex: none;
  }
  .ctl {
    width: 46px;
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--text);
    transition: background-color 0.1s;
  }
  .ctl svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1;
    shape-rendering: geometricPrecision;
  }
  .ctl:hover {
    background: var(--hover);
  }
  .ctl:active {
    background: var(--press);
  }
  .ctl.close:hover {
    background: #c42b1c;
    color: #fff;
  }
  .ctl.close:active {
    background: #c42b1c;
    opacity: 0.9;
  }
</style>
