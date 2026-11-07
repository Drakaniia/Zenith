<script lang="ts">
  import Pane from './Pane.svelte';
  import FindBar from './FindBar.svelte';
  import NewTab from './pages/NewTab.svelte';
  import Favicon from './Favicon.svelte';
  import BookmarksPanel from './BookmarksPanel.svelte';
  import { browser } from '../lib/browser.svelte';
  import { settings } from '../lib/settings.svelte';
  import { ui } from '../lib/ui.svelte';
  import { dnd } from '../lib/sidebar-dnd.svelte';

  const split = $derived(browser.activeSplit);
  const ids = $derived(browser.visibleIds);
  let panesEl: HTMLDivElement | undefined = $state();
  let hoverSide = $state<'left' | 'right' | null>(null);

  function startResize(e: PointerEvent) {
    const s = browser.activeSpace?.split;
    if (!s || !panesEl) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect = panesEl.getBoundingClientRect();
    const move = (ev: PointerEvent) => {
      s.ratio = Math.min(0.8, Math.max(0.2, (ev.clientX - rect.left) / rect.width));
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
  <!-- Zen's bookmarks sidebar sits beside the page card; the page view follows its width. -->
  <div class="row">
  {#if settings.value.bookmarksPanel && !browser.fullscreenTabId}
    <BookmarksPanel />
  {/if}
  <div class="panes" bind:this={panesEl}>
    {#if ids.length === 0}
      <div class="card solid"><NewTab /></div>
    {:else}
      {#each ids as id, i (id)}
        {#if i === 1}
          <div class="divider" role="separator" aria-orientation="vertical" onpointerdown={startResize}><span></span></div>
        {/if}
        <div
          class="card"
          class:focused={!!split && browser.activeTab?.id === id}
          style:flex={split ? `${i === 0 ? split.ratio : 1 - split.ratio} 1 0` : '1 1 0'}
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
    gap: var(--zen-sep);
  }
  .panes {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }
  /* The page card (Zen: .browserSidebarContainer): native inner radius, Zen's big shadow. The
   * page itself is a native view drawn over it, so the radius shows on internal pages. */
  .card {
    position: relative;
    min-width: 0;
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
