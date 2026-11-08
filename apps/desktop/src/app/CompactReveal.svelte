<script lang="ts">
  // Compact mode's hidden chrome: the window edges that reveal the floating sidebar (or tab
  // strip) and the floating toolbar.
  import { cubicOut } from 'svelte/easing';
  import { compactEase, slidePx, slideX } from '$shared/ui/motion';
  import { ui } from '$shared/ui/ui.svelte';
  import { overlayLease } from '$platform/layout.svelte';
  import type { ToolbarMode } from '$features/settings/settings.svelte';
  import type { SidebarPosition } from '$features/sidebar/sidebar-position';
  import { extensions } from '$features/extensions/extensions.svelte';
  import Sidebar from '$features/sidebar/Sidebar.svelte';
  import TabStrip from '$features/sidebar/TabStrip.svelte';
  import TopBar from '$features/toolbar/TopBar.svelte';

  let {
    compact,
    pageFullscreen,
    hideToolbar,
    side,
    strip,
    mode,
  }: {
    compact: boolean;
    pageFullscreen: boolean;
    hideToolbar: boolean;
    side: SidebarPosition;
    strip: boolean;
    mode: ToolbarMode;
  } = $props();

  // Compact mode: the sidebar slides in from its window edge and stays while hovered, plus
  // Zen's 150 ms keep-hover grace (zen.view.compact.sidebar-keep-hover.duration).
  let revealTimer: ReturnType<typeof setTimeout> | undefined;
  function revealSidebar() {
    clearTimeout(revealTimer);
    clearTimeout(hideTimer);
    revealTimer = setTimeout(() => (ui.flyout = true), 50);
  }
  function cancelReveal() {
    clearTimeout(revealTimer);
  }
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  function scheduleHideFlyout() {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      if (!ui.flyoutPinned && !ui.menu && !ui.downloadsOpen && !extensions.popover && !ui.media && !ui.spaceEditor && !ui.palette.open && !ui.renaming && !ui.dragging)
        ui.flyout = false;
    }, 150);
  }
  function keepFlyout() {
    clearTimeout(hideTimer);
  }
  // Compact mode's floating toolbar: the top edge reveals it, like the sidebar's left edge.
  let topTimer: ReturnType<typeof setTimeout> | undefined;
  function revealToolbar() {
    clearTimeout(topTimer);
    topTimer = setTimeout(() => (ui.topReveal = true), 50);
  }
  function scheduleHideToolbar(delay = 150) {
    clearTimeout(topTimer);
    topTimer = setTimeout(() => {
      if (!ui.menu && !ui.downloadsOpen && !extensions.popover && !ui.palette.open) ui.topReveal = false;
      else scheduleHideToolbar(400);
    }, delay);
  }
</script>

<!-- With the tab strip at the top, the top edge reveals the strip and the toolbar together (below). -->
{#if hideToolbar && side !== 'top'}
  <div
    class="hot-top"
    role="presentation"
    onmouseenter={revealToolbar}
    onmouseleave={() => !ui.topReveal && clearTimeout(topTimer)}
    data-tauri-drag-region
  ></div>
  {#if ui.topReveal}
    <div
      class="top-flyout"
      role="presentation"
      use:overlayLease
      onmouseenter={() => clearTimeout(topTimer)}
      onmouseleave={() => scheduleHideToolbar()}
      in:slidePx={{ y: -16, duration: 200, easing: compactEase }}
      out:slidePx={{ y: -16, duration: 150, easing: cubicOut }}
    >
      <TopBar floating />
    </div>
  {/if}
{/if}

{#if compact && !pageFullscreen}
  <!-- The hidden sidebar's window edge reveals it. -->
  <div
    class="hot-edge {side}"
    role="presentation"
    onmouseenter={revealSidebar}
    onmouseleave={cancelReveal}
    data-tauri-drag-region
  ></div>
  {#if ui.flyout}
    {#if strip}
      <div
        class="flyout strip {side}"
        role="presentation"
        use:overlayLease
        onmouseenter={keepFlyout}
        onmouseleave={scheduleHideFlyout}
        in:slidePx={{ y: side === 'top' ? -16 : 16, duration: 200, easing: compactEase }}
        out:slidePx={{ y: side === 'top' ? -16 : 16, duration: 150, easing: cubicOut }}
      >
        <TabStrip floating edge={side === 'top' ? 'top' : 'bottom'} collapsed={mode === 'collapsed'} />
        {#if side === 'top' && hideToolbar}
          <TopBar floating />
        {/if}
      </div>
    {:else}
      <div
        class="flyout {side}"
        role="presentation"
        use:overlayLease
        onmouseenter={keepFlyout}
        onmouseleave={scheduleHideFlyout}
        in:slideX={{ from: side === 'right' ? 1.08 : -1.08, duration: 250, easing: compactEase }}
        out:slideX={{ from: side === 'right' ? 1.08 : -1.08, duration: 150, easing: cubicOut }}
      >
        <Sidebar floating />
      </div>
    {/if}
  {/if}
{/if}

<style>
  /* Compact mode: the window edge that brings the hidden sidebar back. */
  .hot-edge {
    position: absolute;
    z-index: 5;
  }
  .hot-edge.left,
  .hot-edge.right {
    top: 0;
    bottom: 0;
    width: var(--margin);
  }
  .hot-edge.left {
    left: 0;
  }
  .hot-edge.right {
    right: 0;
  }
  .hot-edge.bottom {
    left: 0;
    right: 0;
    bottom: 0;
    height: var(--margin);
  }
  /* At the top it is a thin band over the toolbar, clear of the window controls; with the toolbar
   * hidden too it is the strip above the page, like the floating toolbar's. */
  .hot-edge.top {
    top: 0;
    left: 0;
    right: 140px;
    height: 4px;
  }
  :global(.app.no-toolbar) .hot-edge.top {
    left: var(--margin);
    right: 0;
    height: var(--zen-sep);
  }
  /* Compact mode's floating sidebar (Zen: .zen-toolbar-background over #navigator-toolbox). */
  .flyout {
    position: absolute;
    left: calc(var(--zen-sep) / 2);
    top: calc(var(--zen-sep) / 2);
    bottom: calc(var(--zen-sep) / 2);
    width: var(--sidebar-w);
    z-index: 40;
    padding-top: 2px;
    border-radius: var(--r-panel);
    corner-shape: var(--sq);
    background: color-mix(in srgb, var(--accent) 7%, var(--compact-bg));
    outline: 1px solid rgba(255, 255, 255, 0.15);
    outline-offset: -1px;
    box-shadow: var(--zen-big-shadow);
    overflow: hidden;
  }
  :global(.app:not(.single)) .flyout {
    top: var(--zen-toolbar-h);
  }
  :global(.app.no-toolbar) .flyout {
    top: calc(var(--zen-sep) / 2);
  }
  .flyout.right {
    left: auto;
    right: calc(var(--zen-sep) / 2);
  }
  /* The floating tab strip, over the toolbar at the top or above the page's bottom edge. */
  :global(.app) .flyout.strip {
    left: calc(var(--zen-sep) / 2);
    right: calc(var(--zen-sep) / 2);
    top: auto;
    bottom: auto;
    width: auto;
    padding-top: 0;
  }
  :global(.app) .flyout.strip.top {
    top: calc(var(--zen-sep) / 2);
  }
  :global(.app) .flyout.strip.bottom {
    bottom: calc(var(--zen-sep) / 2);
  }
  .flyout.strip > :global(.strip) {
    height: var(--strip-h);
  }
  .hot-top {
    position: absolute;
    top: 0;
    left: var(--margin);
    right: 0;
    height: var(--zen-sep);
    z-index: 5;
  }
  /* Compact mode's floating toolbar (Zen: #zen-appcontent-navbar-container in compact mode). */
  .top-flyout {
    position: absolute;
    top: calc(var(--zen-sep) / 2);
    left: calc(var(--zen-sep) / 2);
    right: calc(var(--zen-sep) / 2);
    z-index: 41;
    border-radius: var(--r-panel);
    corner-shape: var(--sq);
    background: color-mix(in srgb, var(--accent) 7%, var(--compact-bg));
    outline: 1px solid rgba(255, 255, 255, 0.15);
    outline-offset: -1px;
    box-shadow: var(--zen-big-shadow);
    overflow: hidden;
  }
</style>
