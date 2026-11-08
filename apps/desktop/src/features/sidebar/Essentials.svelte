<script lang="ts">
  import { flip } from 'svelte/animate';
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import { browser, needsWebview, tabTitle } from '$features/browser/browser.svelte';
  import { dnd } from './sidebar-dnd.svelte';
  import { tabMenu } from '../../app/menus';
  import { ui } from '$shared/ui/ui.svelte';

  // Zen's Essentials: a grid of favicon tiles above the spaces, shared by every space.
  let { collapsed = false }: { collapsed?: boolean } = $props();

  const essentials = $derived(browser.essentials.map((id) => browser.tabs[id]).filter(Boolean));
  const active = $derived(browser.activeTab);
</script>

{#if essentials.length}
  <div class="essentials" class:collapsed data-zone="essentials">
    {#each essentials as tab (tab.id)}
      {@const mark = dnd.mark(tab.id)}
      <button
        class="essential"
        class:active={active?.id === tab.id}
        class:sleeping={!tab.live && needsWebview(tab.url)}
        class:dragging={dnd.drag?.started && dnd.drag.id === tab.id}
        data-tab-id={tab.id}
        title={tabTitle(tab)}
        animate:flip={{ duration: 160 }}
        onclick={() => browser.activate(tab.id)}
        onauxclick={(e) => e.button === 1 && browser.closeTab(tab.id)}
        oncontextmenu={(e) => {
          e.preventDefault();
          ui.openMenu(e.clientX, e.clientY, tabMenu(tab));
        }}
        onpointerdown={(e) => dnd.start(e, tab.id)}
      >
        <span class="bg">
          {#if active?.id === tab.id && tab.favicon}
            <img class="glow" src={tab.favicon} alt="" draggable="false" />
          {/if}
        </span>
        <span class="icon"><Favicon url={tab.url} favicon={tab.favicon} size={16} /></span>
        {#if tab.audible || tab.muted}
          <span class="audio"><Icon name={tab.muted ? 'volumeOff' : 'volume'} size={10} stroke={2.4} /></span>
        {/if}
        {#if mark}<span class="mark" class:after={mark === 'after'}></span>{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .essentials {
    display: grid;
    /* zen-essentials-container: up to four per row, wrapping at a tile's minimum width */
    grid-template-columns: repeat(auto-fill, minmax(max(calc(25% - 4px), 48px), 1fr));
    gap: 4px;
    padding: 6px var(--zen-pad) 2px;
  }
  .essentials.collapsed {
    grid-template-columns: 1fr;
    justify-items: center;
    padding-inline: 0;
  }
  .essential {
    position: relative;
    height: calc(var(--essential-h) + var(--tab-mb) * 2);
    min-width: 0;
    padding: var(--tab-mb);
    display: grid;
    place-items: center;
    transition: scale 0.1s ease;
  }
  .essentials.collapsed .essential {
    width: calc(var(--collapsed-tab-w) + var(--tab-mb) * 2);
    height: calc(var(--tab-h) + var(--tab-mb) * 2);
  }
  .essential:active {
    scale: 0.98;
  }
  .essential.dragging {
    opacity: 0.4;
  }
  .bg {
    position: absolute;
    inset: var(--tab-mb);
    overflow: hidden;
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    background: var(--element-bg);
    transition: background-color 0.1s ease;
  }
  .essential:hover .bg {
    background: var(--tab-selected);
  }
  :global(:root[data-theme='light']) .essential:hover .bg {
    background: rgba(0, 0, 0, 0.1);
  }
  /* Selected: the favicon, blurred, glows behind an inset tile (zen.theme.essentials-favicon-bg). */
  .essential.active .bg {
    background: transparent;
  }
  .essential.active .bg::before {
    content: '';
    position: absolute;
    inset: 2px;
    z-index: 1;
    border-radius: calc(var(--r-tab) - 2px);
    corner-shape: var(--sq);
    background: color-mix(in srgb, var(--accent) 10%, var(--essential-selected));
    transition: background 0.1s ease-in-out;
  }
  .essential.active:hover .bg::before {
    background: color-mix(in srgb, var(--accent) 10%, var(--essential-selected-hover));
  }
  .essential.active .bg:not(:has(.glow)) {
    background: var(--element-bg);
  }
  .glow {
    position: absolute;
    inset: -50%;
    width: 200%;
    height: 200%;
    object-fit: contain;
    filter: blur(20px) saturate(1.4);
    opacity: 0.9;
    pointer-events: none;
  }
  .icon {
    position: relative;
    z-index: 2;
    display: grid;
  }
  .essential.sleeping .icon {
    opacity: 0.5;
  }
  .audio {
    position: absolute;
    z-index: 2;
    right: 7px;
    top: 6px;
    color: var(--text-2);
  }
  .mark {
    position: absolute;
    z-index: 3;
    left: -3px;
    top: 10px;
    bottom: 10px;
    width: 2px;
    border-radius: 2px;
    background: var(--drag-indicator);
  }
  .mark.after {
    left: auto;
    right: -3px;
  }
  .essentials.collapsed .mark {
    left: 10px;
    right: 10px;
    top: -1px;
    bottom: auto;
    width: auto;
    height: 2px;
  }
  .essentials.collapsed .mark.after {
    top: auto;
    bottom: -1px;
  }
</style>
