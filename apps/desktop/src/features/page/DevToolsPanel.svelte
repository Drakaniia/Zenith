<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { devtoolsPane } from '$platform/layout.svelte';
  import { devtools } from './devtools.svelte';

  let { tabId, style = '' }: { tabId: string; style?: string } = $props();
  const bottom = $derived(devtools.dock === 'bottom');
</script>

<aside class="dt-card" {style} aria-label="Developer tools">
  <header class="dt-bar">
    <span class="dt-title">
      <Icon name="code" size={13} />
      <span>DevTools</span>
    </span>
    <div class="dt-actions">
      <button
        type="button"
        class="dt-btn"
        title={bottom ? 'Dock to right' : 'Dock to bottom'}
        aria-label={bottom ? 'Dock to right' : 'Dock to bottom'}
        onclick={() => devtools.setDock(bottom ? 'right' : 'bottom')}
      >
        <Icon name={bottom ? 'dockRight' : 'dockBottom'} size={14} />
      </button>
      <button
        type="button"
        class="dt-btn"
        title="Close DevTools (F12)"
        aria-label="Close DevTools"
        onclick={() => void devtools.close(tabId)}
      >
        <Icon name="close" size={14} />
      </button>
    </div>
  </header>
  <!-- The native CEF DevTools view is positioned over this element; it stays transparent so
       DevTools shows through when floating overlays raise chrome above native views. -->
  <div class="dt-body" use:devtoolsPane={tabId}></div>
</aside>

<style>
  .dt-card {
    flex: none;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-content);
    box-shadow: var(--zen-big-shadow);
    overflow: hidden;
  }
  .dt-bar {
    flex: none;
    height: 28px;
    padding: 0 6px 0 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    background: var(--page-bg);
    border-bottom: 1px solid var(--line);
    color: var(--text-2);
    font-size: 12px;
    font-weight: 500;
    user-select: none;
  }
  .dt-title {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .dt-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .dt-btn {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .dt-btn:hover {
    background: var(--hover);
    color: var(--text);
  }
  .dt-body {
    flex: 1;
    min-width: 0;
    min-height: 0;
    position: relative;
  }
</style>
