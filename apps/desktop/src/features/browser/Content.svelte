<script lang="ts">
  import Pane from './Pane.svelte';
  import FindBar from '$features/page/FindBar.svelte';
  import DevToolsPanel from '$features/page/DevToolsPanel.svelte';
  import { devtools } from '$features/page/devtools.svelte';
  import { clampDevToolsSize } from '$features/page/devtools';
  import NewTab from '$features/startpage/NewTab.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import BookmarksPanel from '$features/bookmarks/BookmarksPanel.svelte';
  import { browser } from './browser.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { dnd } from '$features/sidebar/sidebar-dnd.svelte';

  let { panelAnimating = false }: { panelAnimating?: boolean } = $props();
  const showPanel = $derived((settings.value.bookmarksPanel || panelAnimating) && !browser.fullscreenTabId);
  const split = $derived(browser.activeSplit);
  const ids = $derived(browser.visibleIds);
  const layout = $derived(split?.layout ?? 'vsep');
  /** Two panes side by side or stacked share the space by the split ratio, with a draggable
   *  divider; more panes, and the grid, share it equally. */
  const resizable = $derived(!!split && ids.length === 2 && layout !== 'grid');
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
  let hoverSide = $state<'left' | 'right' | null>(null);

  function startResize(e: PointerEvent) {
    const s = browser.activeSpace?.split;
    if (!s || !panesEl) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect = panesEl.getBoundingClientRect();
    const stacked = s.layout === 'hsep';
    const move = (ev: PointerEvent) => {
      const at = stacked ? (ev.clientY - rect.top) / rect.height : (ev.clientX - rect.left) / rect.width;
      s.ratio = Math.min(0.8, Math.max(0.2, at));
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
    class:grid={!!split && layout === 'grid'}
    class:spaced={!!split && !resizable}
    class:three={!!split && layout === 'grid' && ids.length === 3}
    bind:this={panesEl}
  >
    {#if ids.length === 0}
      <div class="card solid"><NewTab /></div>
    {:else}
      {#each ids as id, i (id)}
        {#if i === 1 && resizable}
          <div
            class="divider"
            class:h={layout === 'hsep'}
            role="separator"
            aria-orientation={layout === 'hsep' ? 'horizontal' : 'vertical'}
            onpointerdown={startResize}
          ><span></span></div>
        {/if}
        <div
          class="card"
          class:focused={!!split && browser.activeTab?.id === id}
          style:flex={resizable && split ? `${i === 0 ? split.ratio : 1 - split.ratio} 1 0` : '1 1 0'}
          role="presentation"
          onpointerdown={() => split && browser.activeTab?.id !== id && browser.activate(id)}
        >
          {#if browser.tabs[id]}
            <Pane tab={browser.tabs[id]} />
          {/if}
        </div>
      {/each}
    {/if}

    {#if ui.dragging && browser.activeTab}
      <div class="zones">
        {#each ['left', 'right'] as side (side)}
          <div
            class="zone"
            class:hot={hoverSide === side}
            data-drop-split={side}
            role="presentation"
            onpointerenter={() => (hoverSide = side as 'left' | 'right')}
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
  /* Zen's split layouts: stacked (horizontal separator) and grid. */
  .panes.hsep {
    flex-direction: column;
  }
  .panes.spaced {
    gap: var(--zen-sep);
  }
  .panes.grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-auto-rows: minmax(0, 1fr);
  }
  /* Three tabs: the first takes the whole left column. */
  .panes.grid.three > .card:first-of-type {
    grid-row: span 2;
  }
  /* The page card (Zen: .browserSidebarContainer): native inner radius, Zen's big shadow. The
   * page itself is a native view drawn over it, so the radius shows on internal pages. */
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
  /* Split view: the focused pane gets Zen's 2px outline. */
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
  .divider span {
    width: 2px;
    height: 50px;
    border-radius: 2px;
    background: var(--accent-button);
    opacity: 0;
    transition: opacity 0.1s ease-in-out;
  }
  .divider:hover span {
    opacity: 1;
  }
  .divider.h {
    width: auto;
    height: var(--zen-sep);
    cursor: ns-resize;
  }
  .divider.h span {
    width: 50px;
    height: 2px;
  }
  .zones {
    position: absolute;
    inset: 0;
    display: flex;
    gap: var(--zen-sep);
    z-index: 20;
  }
  /* Drop targets look like Zen's fake split browser: an accent-bordered card. */
  .zone {
    flex: 1;
    display: grid;
    place-items: center;
    border-radius: var(--r-content);
    border: 2px solid transparent;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    transition:
      background 0.08s ease-out,
      border-color 0.08s ease-out,
      box-shadow 0.08s ease-out;
  }
  .zone.hot {
    border-color: var(--accent-button);
    background: rgba(255, 255, 255, 0.1);
    box-shadow: var(--zen-big-shadow);
  }
  .zone-label {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 20px 28px;
    border-radius: 16px;
    corner-shape: var(--sq);
    background: var(--dialog-bg);
    box-shadow: var(--zen-big-shadow);
    font-size: 14px;
    font-weight: 700;
    text-transform: capitalize;
  }
</style>
