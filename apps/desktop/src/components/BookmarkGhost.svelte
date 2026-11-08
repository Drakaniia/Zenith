<script lang="ts">
  import Icon from './Icon.svelte';
  import Favicon from './Favicon.svelte';
  import { bmDnd } from '../lib/bookmark-dnd.svelte';
  import { faviconFor } from '../lib/favicons';

  // What follows the pointer while bookmarks are dragged (like the tab ghost in the sidebar).
  const drag = $derived(bmDnd.drag?.started ? bmDnd.drag : null);
</script>

{#if drag}
  <div class="ghost" style:transform="translate({drag.x + 12}px, {drag.y + 8}px)">
    {#if drag.folder}
      <Icon name="folder" size={16} />
    {:else if drag.url}
      <Favicon url={drag.url} favicon={faviconFor(drag.url)} size={16} />
    {/if}
    <span class="ellipsis">{drag.label}</span>
    {#if drag.ids.length > 1}<span class="count">{drag.ids.length}</span>{/if}
  </div>
{/if}

<style>
  .ghost {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 240px;
    height: 30px;
    padding: 0 12px;
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    background: var(--tab-selected);
    backdrop-filter: blur(20px);
    box-shadow: var(--zen-big-shadow);
    pointer-events: none;
    font-size: 12.5px;
  }
  .count {
    flex: none;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 600;
    color: #fff;
    background: var(--accent-button);
  }
</style>
