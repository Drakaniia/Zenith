<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import { browser, needsWebview, tabTitle } from '$features/browser/browser.svelte';
  import { dnd } from './sidebar-dnd.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { tabMenu } from './menus';
  import type { Tab } from '$features/browser/types';

  // One tab of the horizontal tab strip (TabStrip): a full tab, or an icon for Essentials, pinned
  // tabs and every tab in the collapsed layout.
  let { tab, iconOnly, activeId, splitIds }: { tab: Tab; iconOnly: boolean; activeId: string | undefined; splitIds: string[] } = $props();

  const mark = $derived(dnd.mark(tab.id));

  const sleeping = (t: Tab) => !t.live && needsWebview(t.url);
  const picked = (t: Tab) => ui.selectedTabs.length > 1 && ui.selectedTabs.includes(t.id);

  /** Same as a sidebar tab: Ctrl adds the tab to the picked ones, Shift picks a range. */
  function click(e: MouseEvent, tab: Tab) {
    if (e.ctrlKey || e.metaKey) ui.toggleTabSelected(tab.id, browser.activeTab?.id);
    else if (e.shiftKey) ui.selectTabRange(browser.order(), ui.selectedTabs[0] ?? browser.activeTab?.id, tab.id);
    else {
      ui.clearTabSelection();
      browser.activate(tab.id);
    }
  }

  /** Pinned tabs unload instead of closing; an unloaded pinned tab's X removes it, as in Zen. */
  function close(e: MouseEvent, tab: Tab) {
    e.stopPropagation();
    if (tab.pinned && sleeping(tab)) browser.removeTab(tab.id, true);
    else browser.closeTab(tab.id);
  }

  function tabContext(e: MouseEvent, tab: Tab) {
    e.preventDefault();
    ui.openMenu(e.clientX, e.clientY, tabMenu(tab));
  }
</script>

<div
  class="tab"
  class:icon-only={iconOnly}
  class:active={activeId === tab.id}
  class:picked={picked(tab)}
  class:sleeping={sleeping(tab)}
  class:in-split={splitIds.includes(tab.id)}
  class:dragging={dnd.drag?.started && dnd.drag.id === tab.id}
  role="tab"
  tabindex="-1"
  aria-selected={activeId === tab.id}
  data-tab-id={tab.id}
  title={tabTitle(tab)}
  onclick={(e) => click(e, tab)}
  onauxclick={(e) => e.button === 1 && browser.closeTab(tab.id)}
  onkeydown={(e) => e.key === 'Enter' && browser.activate(tab.id)}
  oncontextmenu={(e) => tabContext(e, tab)}
  onpointerdown={(e) => dnd.start(e, tab.id)}
>
  <span class="bg"></span>
  <span class="icon">
    {#if tab.crashed}
      <span class="warn"><Icon name="alert" size={15} /></span>
    {:else}
      <Favicon url={tab.url} favicon={tab.favicon} />
    {/if}
    {#if iconOnly && (tab.audible || tab.muted)}
      <span class="audio-badge"><Icon name={tab.muted ? 'volumeOff' : 'volume'} size={9} stroke={2.6} /></span>
    {/if}
  </span>
  {#if !iconOnly}
    <span class="title ellipsis">{tabTitle(tab)}</span>
    {#if tab.audible || tab.muted}
      <button
        class="mini"
        title={tab.muted ? 'Unmute tab' : 'Mute tab'}
        onclick={(e) => {
          e.stopPropagation();
          browser.setMuted(tab.id, !tab.muted);
        }}
      >
        <Icon name={tab.muted ? 'volumeOff' : 'volume'} size={13} />
      </button>
    {/if}
    {#if !tab.essential}
      <button class="mini close" title="Close tab" aria-label="Close tab" onclick={(e) => close(e, tab)}>
        <Icon name="close" size={13} />
      </button>
    {/if}
  {/if}
  {#if mark}<span class="mark" class:after={mark === 'after'}></span>{/if}
</div>

<style>
  .tab {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 6px 0 10px;
    font-size: 13px;
    transition:
      scale 0.1s ease,
      opacity var(--med) ease;
  }
  .tab.icon-only {
    flex: none;
    width: 36px;
    padding: 0;
    justify-content: center;
  }
  .tab:active:not(:has(button:active)) {
    scale: 0.98;
  }
  .bg {
    position: absolute;
    inset: 0;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    pointer-events: none;
    transition: background-color 0.1s ease;
  }
  .tab:hover .bg {
    background: var(--hover);
  }
  .tab.active .bg {
    background: var(--tab-selected);
    box-shadow: var(--tab-shadow);
  }
  .tab.picked .bg {
    background: color-mix(in srgb, var(--accent) 24%, var(--hover));
  }
  .tab.dragging {
    opacity: 0.4;
  }
  /* Tabs shown in split view get Zen's split colour as a bar along the strip's inner edge. */
  .tab.in-split::after {
    content: '';
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 1px;
    height: 2px;
    border-radius: 2px;
    background: var(--split-outline);
  }
  /* The strip (TabStrip's root) at the bottom of the window. */
  :global(.strip.bottom) .tab.in-split::after {
    bottom: auto;
    top: 1px;
  }
  .icon {
    position: relative;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    flex: none;
  }
  .tab.sleeping .icon {
    opacity: 0.5;
  }
  .warn {
    color: var(--warn);
  }
  .title {
    position: relative;
    flex: 1;
    min-width: 0;
    line-height: 1.2;
  }
  .mini {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    opacity: 0.8;
  }
  .mini:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
    opacity: 1;
  }
  .close {
    visibility: hidden;
  }
  .tab:hover .close,
  .tab.active .close {
    visibility: visible;
  }
  .audio-badge {
    position: absolute;
    right: -7px;
    top: -6px;
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--panel-bg);
    box-shadow: 0 0 0 1px var(--line);
  }
  /* Drop indicator: a 2px bar before or after the tab. */
  .mark {
    position: absolute;
    z-index: 2;
    top: 6px;
    bottom: 6px;
    left: -2px;
    width: 2px;
    border-radius: 2px;
    background: var(--drag-indicator);
    pointer-events: none;
  }
  .mark.after {
    left: auto;
    right: -2px;
  }
</style>
