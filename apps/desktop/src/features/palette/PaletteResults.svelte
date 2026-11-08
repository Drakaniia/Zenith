<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import { bookmarks } from '$features/bookmarks/bookmarks.svelte';
  import type { Item } from './palette-sources';

  let {
    items,
    selected,
    list = $bindable(),
    onhover,
  }: {
    items: Item[];
    selected: number;
    list?: HTMLDivElement;
    /** The pointer moved onto another row: it becomes the selection. */
    onhover: (i: number) => void;
  } = $props();
</script>

<div class="results" role="listbox" bind:this={list}>
  {#each items as it, i (it.key)}
    <button
      class="item"
      class:sel={i === selected}
      role="option"
      aria-selected={i === selected}
      onmousemove={() => {
        if (selected === i) return;
        onhover(i);
      }}
      onclick={(e) => it.run(e.altKey ? 'newTab' : e.shiftKey ? 'glance' : 'enter')}
    >
      <span class="item-icon">
        {#if it.kind === 'tab' || it.favicon}
          <Favicon url={it.url ?? ''} favicon={it.favicon} />
        {:else}
          <Icon name={it.icon ?? 'globe'} size={16} />
        {/if}
        {#if it.kind === 'bookmark' || (it.url && bookmarks.has(it.url))}
          <span class="badge" title="Bookmarked"><Icon name="star" size={9} fill stroke={1.5} /></span>
        {/if}
      </span>
      <span class="item-text">
        <span class="item-title ellipsis">{it.title}</span>
        {#if it.subtitle}<span class="item-sub ellipsis">{it.subtitle}</span>{/if}
      </span>
      {#if it.kind === 'tab'}
        <span class="tag">Switch to tab</span>
      {:else if i === selected && it.kind !== 'command'}
        <span class="tag">↵</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  .badge {
    position: absolute;
    right: -3px;
    bottom: -3px;
    display: grid;
    color: var(--accent-button);
    pointer-events: none;
  }
  .results {
    max-height: min(440px, 55vh);
    overflow-y: auto;
    padding: 6px 4px;
  }
  .item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 45px;
    padding: 0 12px;
    border-radius: 10px;
    corner-shape: var(--sq);
    text-align: left;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.sel {
    background: color-mix(in srgb, var(--accent) 20%, var(--element-bg));
  }
  .item-icon {
    position: relative;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    background: var(--element-bg);
    color: var(--text-2);
    flex: none;
  }
  .item-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .item-title {
    font-size: 13.5px;
  }
  .item-sub {
    font-size: 12px;
    color: var(--text-3);
  }
  .tag {
    font-size: 11.5px;
    color: var(--text-3);
    white-space: nowrap;
  }
</style>
