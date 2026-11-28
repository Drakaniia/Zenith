<script lang="ts">
  import Pane from './Pane.svelte';
  import FindBar from '$features/page/FindBar.svelte';
  import DevToolsPanel from '$features/page/DevToolsPanel.svelte';
  import { devtools } from '$features/page/devtools.svelte';
  import { clampDevToolsSize } from '$features/page/devtools';
  import NewTab from '$features/startpage/NewTab.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import BookmarksPanel from '$features/bookmarks/BookmarksPanel.svelte';
  import { browser, tabTitle } from './browser.svelte';
  import { effectiveRatios, resizeAdjacent, resizeGridAxis } from './split';
  import { settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { dnd, type SplitSide } from '$features/sidebar/sidebar-dnd.svelte';

  let { panelAnimating = false }: { panelAnimating?: boolean } = $props();
  const showPanel = $derived((settings.value.bookmarksPanel || panelAnimating) && !browser.fullscreenTabId);
  const split = $derived(browser.activeSplit);
  const ids = $derived(browser.visibleIds);
  const layout = $derived(split?.layout ?? 'vsep');
  const isGrid = $derived(!!split && layout === 'grid');
  const ratios = $derived(split ? effectiveRatios(split) : []);
  const devtoolsTabId = $derived(
    !browser.fullscreenTabId &&
      browser.activeTab?.live &&
      !browser.activeTab.crashed &&
      devtools.isOpen(browser.activeTab.id)
      ? browser.activeTab.id
      : null,
  );
  const dtBottom = $derived(devtools.dock === 'bottom');
  let stageEl: HTMLDivElement | undefined = $state();
  let panesEl: HTMLDivElement | undefined = $state();
  let hoverSide = $state<SplitSide | null>(null);
  let dragPane = $state<number | null>(null);

  function startLinearResize(e: PointerEvent, dividerIndex: number) {
    const s = browser.activeSplit;
    if (!s || !panesEl) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect = panesEl.getBoundingClientRect();
    const stacked = (s.layout ?? 'vsep') === 'hsep';
    const move = (ev: PointerEvent) => {
      const pos = stacked ? (ev.clientY - rect.top) / rect.height : (ev.clientX - rect.left) / rect.width;
      const next = resizeAdjacent(effectiveRatios(s), dividerIndex, pos);
      s.ratios = next;
      s.ratio = next[0] ?? 0.5;
    };
    const up = () => el.removeEventListener('pointermove', move);
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up, { once: true });
  }

  function startGridResize(e: PointerEvent, axis: 0 | 1) {
    const s = browser.activeSplit;
    if (!s || !panesEl) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect = panesEl.getBoundingClientRect();
    const move = (ev: PointerEvent) => {
      const pos = axis === 1 ? (ev.clientY - rect.top) / rect.height : (ev.clientX - rect.left) / rect.width;
      const next = resizeGridAxis(effectiveRatios(s), axis, pos);
      s.ratios = next;
      s.ratio = next[0];
    };
    const up = () => el.removeEventListener('pointermove', move);
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up, { once: true });
  }

  function startDevToolsResize(e: PointerEvent) {
    if (!stageEl) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect = stageEl.getBoundingClientRect();
    const side = devtools.dock;
    const move = (ev: PointerEvent) => {
      const desired = side === 'bottom' ? rect.bottom - ev.clientY : rect.right - ev.clientX;
      const total = side === 'bottom' ? rect.height : rect.width;
      devtools.setSize(side, clampDevToolsSize(desired, total));
    };
    const up = () => el.removeEventListener('pointermove', move);
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up, { once: true });
  }
</script>

<section class="content" class:split={!!split}>
  {#if ui.find.open && browser.tabs[ui.find.tabId]}
    <FindBar />
  {/if}
  {#if split && !browser.fullscreenTabId}
    <div class="split-bar" role="toolbar" aria-label="Split view controls">
      <div class="split-chips">
        {#each ids as id, i (id)}
          {@const t = browser.tabs[id]}
          {#if t}
            <button
              type="button"
              class="split-chip"
              class:active={browser.activeTab?.id === id}
              class:drag-src={dragPane === i}
              draggable="true"
              title="Click to focus, drag to rearrange panes"
              onclick={() => browser.activate(id)}
              ondragstart={() => (dragPane = i)}
              ondragover={(e) => e.preventDefault()}
              ondrop={() => {
                if (dragPane !== null && dragPane !== i) browser.swapSplitPanes(dragPane, i);
                dragPane = null;
              }}
              ondragend={() => (dragPane = null)}
            >
              <Favicon url={t.url} favicon={t.favicon} size={13} />
              <span class="ellipsis">{tabTitle(t)}</span>
            </button>
          {/if}
        {/each}
      </div>
      <div class="split-tools">
        <button type="button" class="tool" class:on={layout === 'vsep'} title="Side by side" onclick={() => browser.setSplitLayout('vsep')}>Columns</button>
        <button type="button" class="tool" class:on={layout === 'hsep'} title="Stacked" onclick={() => browser.setSplitLayout('hsep')}>Rows</button>
        {#if ids.length >= 3}
          <button type="button" class="tool" class:on={layout === 'grid'} title="Grid" onclick={() => browser.setSplitLayout('grid')}>Grid</button>
        {/if}
        <button type="button" class="tool" title="Swap first two panes" onclick={() => browser.swapSplitPanes(0, 1)}>Swap</button>
        <button type="button" class="tool" title="Reset pane sizes" onclick={() => browser.resetSplitRatios()}>Reset</button>
        <button type="button" class="tool close-split" title="Exit split view" onclick={() => browser.unsplit()}><Icon name="unsplit" size={13} /></button>
      </div>
    </div>
  {/if}
  <!-- Zen's bookmarks sidebar sits beside the page card; its slot slides on --lay-panel while
       the page webview clips at its endpoint width without mid-slide Blink reflows. -->
  <div class="row">
  <div class="panel-slot">
    {#if showPanel}
      <BookmarksPanel />
    {/if}
  </div>
  <div class="stage" class:dt-bottom={dtBottom} bind:this={stageEl}>
  <div
    class="panes"
    class:hsep={!!split && layout === 'hsep'}
    class:grid={isGrid}
    class:three={isGrid && ids.length === 3}
    style:grid-template-columns={isGrid ? `${ratios[0] ?? 0.5}fr ${1 - (ratios[0] ?? 0.5)}fr` : undefined}
    style:grid-template-rows={isGrid ? `${ratios[1] ?? 0.5}fr ${1 - (ratios[1] ?? 0.5)}fr` : undefined}
    bind:this={panesEl}
  >
    {#if ids.length === 0}
      <div class="card solid"><NewTab /></div>
    {:else}
      {#each ids as id, i (id)}
        {#if i > 0 && split && !isGrid}
          <div
            class="divider"
            class:h={layout === 'hsep'}
            role="separator"
            aria-orientation={layout === 'hsep' ? 'horizontal' : 'vertical'}
            ondblclick={() => browser.resetSplitRatios()}
            onpointerdown={(e) => startLinearResize(e, i - 1)}
          ><span></span></div>
        {/if}
        <div
          class="card"
          class:focused={!!split && browser.activeTab?.id === id}
          style:flex={split && !isGrid ? `${ratios[i] ?? 1 / ids.length} 1 0` : '1 1 0'}
          role="presentation"
          onpointerdown={() => split && browser.activeTab?.id !== id && browser.activate(id)}
        >
          {#if browser.tabs[id]}
            <Pane tab={browser.tabs[id]} />
          {/if}
        </div>
      {/each}
      {#if isGrid}
        <div
          class="grid-handle v"
          style:left="calc({(ratios[0] ?? 0.5) * 100}% - var(--zen-sep) / 2)"
          role="separator"
          aria-orientation="vertical"
          ondblclick={() => browser.resetSplitRatios()}
          onpointerdown={(e) => startGridResize(e, 0)}
        ><span></span></div>
        <div
          class="grid-handle h"
          style:top="calc({(ratios[1] ?? 0.5) * 100}% - var(--zen-sep) / 2)"
          role="separator"
          aria-orientation="horizontal"
          ondblclick={() => browser.resetSplitRatios()}
          onpointerdown={(e) => startGridResize(e, 1)}
        ><span></span></div>
      {/if}
    {/if}

    {#if ui.dragging && dnd.drag?.kind === 'tab' && browser.activeTab}
      <div class="zones">
        {#each (['top', 'left', 'right', 'bottom'] as SplitSide[]) as side (side)}
          <div
            class="zone {side}"
            class:hot={hoverSide === side}
            data-drop-split={side}
            role="presentation"
            onpointerenter={() => (hoverSide = side)}
            onpointerleave={() => (hoverSide = null)}
          >
            {#if hoverSide === side && dnd.drag && browser.tabs[dnd.drag.id]}
              {@const t = browser.tabs[dnd.drag.id]}
              <div class="zone-label">
                <Favicon url={t.url} favicon={t.favicon} size={20} />
                <span>Split {side}</span>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
  {#if devtoolsTabId}
    <div
      class="divider"
      class:h={dtBottom}
      role="separator"
      aria-orientation={dtBottom ? 'horizontal' : 'vertical'}
      aria-label="Resize developer tools"
      onpointerdown={startDevToolsResize}
    ><span></span></div>
    <DevToolsPanel
      tabId={devtoolsTabId}
      style={dtBottom
        ? `height:${devtools.height}px;max-height:calc(100% - 240px)`
        : `width:${devtools.width}px;max-width:calc(100% - 240px)`}
    />
  {/if}
  </div>
  </div>
</section>

<style>
  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--zen-sep);
  }
  .split-bar {
    flex: none;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 4px;
    font-size: 11.5px;
  }
  .split-chips {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }
  .split-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 180px;
    height: 22px;
    padding: 0 8px;
    border-radius: 7px;
    background: var(--element-bg);
    color: var(--text-2);
    cursor: grab;
  }
  .split-chip.active {
    background: var(--tab-selected);
    color: var(--text);
  }
  .split-chip.drag-src {
    opacity: 0.45;
  }
  .split-tools {
    display: flex;
    align-items: center;
    gap: 3px;
    flex: none;
  }
  .tool {
    height: 22px;
    padding: 0 7px;
    border-radius: 6px;
    font-size: 11px;
    color: var(--text-2);
  }
  .tool:hover,
  .tool.on {
    background: var(--element-bg);
    color: var(--text);
  }
  .close-split {
    display: grid;
    place-items: center;
    width: 24px;
    padding: 0;
  }
  .row {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: flex;
  }
  .panel-slot {
    position: relative;
    flex: none;
    width: var(--lay-panel);
    min-height: 0;
    overflow: clip;
  }
  .panel-slot > :global(aside.panel) {
    position: absolute;
    top: 0;
    bottom: 0;
    right: var(--zen-sep);
  }
  .stage {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }
  .stage.dt-bottom {
    flex-direction: column;
  }
  .panes {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }
  .panes.hsep {
    flex-direction: column;
  }
  .panes.grid {
    display: grid;
    gap: var(--zen-sep);
  }
  .panes.grid.three > .card:first-of-type {
    grid-row: span 2;
  }
  .card {
    position: relative;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-content);
    box-shadow: var(--zen-big-shadow);
    overflow: hidden;
    transition: flex-grow 0.09s ease-out;
  }
  .card.solid {
    flex: 1;
    background: var(--page-bg);
  }
  .split .card.focused {
    outline: 2px solid var(--split-outline);
    outline-offset: 0;
  }
  .divider {
    flex: none;
    width: var(--zen-sep);
    cursor: ew-resize;
    display: grid;
    place-items: center;
  }
  .divider span,
  .grid-handle span {
    width: 2px;
    height: 50px;
    border-radius: 2px;
    background: var(--accent-button);
    opacity: 0;
    transition: opacity 0.1s ease-in-out;
  }
  .divider:hover span,
  .grid-handle:hover span {
    opacity: 1;
  }
  .divider.h {
    width: auto;
    height: var(--zen-sep);
    cursor: ns-resize;
  }
  .divider.h span,
  .grid-handle.h span {
    width: 50px;
    height: 2px;
  }
  .grid-handle {
    position: absolute;
    z-index: 6;
    display: grid;
    place-items: center;
  }
  .grid-handle.v {
    top: 0;
    bottom: 0;
    width: var(--zen-sep);
    cursor: ew-resize;
  }
  .grid-handle.h {
    left: 0;
    right: 0;
    height: var(--zen-sep);
    cursor: ns-resize;
  }
  .zones {
    position: absolute;
    inset: 0;
    z-index: 20;
  }
  .zone {
    position: absolute;
    display: grid;
    place-items: center;
    border-radius: var(--r-content);
    border: 2px solid transparent;
    background: color-mix(in srgb, var(--accent) 5%, transparent);
    transition: background 0.08s ease-out, border-color 0.08s ease-out, box-shadow 0.08s ease-out;
  }
  .zone.top { left: 18%; right: 18%; top: 0; height: 24%; }
  .zone.bottom { left: 18%; right: 18%; bottom: 0; height: 24%; }
  .zone.left { left: 0; width: 48%; top: 24%; bottom: 24%; }
  .zone.right { right: 0; width: 48%; top: 24%; bottom: 24%; }
  .zone.hot {
    border-color: var(--accent-button);
    background: rgba(255, 255, 255, 0.1);
    box-shadow: var(--zen-big-shadow);
    z-index: 2;
  }
  .zone-label {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 16px 24px;
    border-radius: 16px;
    corner-shape: var(--sq);
    background: var(--dialog-bg);
    box-shadow: var(--zen-big-shadow);
    font-size: 13px;
    font-weight: 700;
    text-transform: capitalize;
  }
</style>
