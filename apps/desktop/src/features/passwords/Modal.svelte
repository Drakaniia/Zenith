<script lang="ts">
  // A dialog inside the passwords page (not over the whole window: in split view the other
  // pane's page would draw over it).
  import type { Snippet } from 'svelte';
  import { fade, scale } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';

  let {
    title,
    onclose,
    width = 440,
    children,
  }: { title: string; onclose: () => void; width?: number; children: Snippet } = $props();

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<div class="scrim" transition:fade={{ duration: 120 }} role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div
    class="dialog"
    style:width="{width}px"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    onkeydown={onKey}
    transition:scale={{ start: 0.97, duration: 140 }}
  >
    <header>
      <h3>{title}</h3>
      <button class="icon-btn sm" title="Close" onclick={onclose}><Icon name="close" size={14} /></button>
    </header>
    <div class="content">{@render children()}</div>
  </div>
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 30;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--backdrop);
  }
  .dialog {
    max-width: 100%;
    max-height: 100%;
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-xl);
    background: var(--dialog-bg);
    border: 1px solid var(--line);
    box-shadow: var(--shadow-lg);
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 16px 16px 4px 20px;
  }
  h3 {
    margin: 0;
    font: 600 16px/1.3 var(--font-display);
  }
  .content {
    padding: 8px 20px 20px;
    overflow-y: auto;
  }
</style>
