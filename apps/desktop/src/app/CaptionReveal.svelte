<script lang="ts">
  // The single-toolbar layout's strip above the page, which reveals the window controls.
  import { fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { slidePx } from '$shared/ui/motion';
  import WindowControls from '$shared/ui/WindowControls.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { overlayLease } from '$platform/layout.svelte';
  import BookmarksBar from '$features/bookmarks/BookmarksBar.svelte';

  let {
    single,
    pageFullscreen,
    barOn,
    revealBar,
  }: { single: boolean; pageFullscreen: boolean; barOn: boolean; revealBar: boolean } = $props();

  // Single toolbar: hovering the strip above the page reveals the window controls (Zen's
  // zen.view.hide-window-controls), after a short delay, and hides them once the pointer leaves.
  let captionTimer: ReturnType<typeof setTimeout> | undefined;
  function revealCaptions() {
    clearTimeout(captionTimer);
    captionTimer = setTimeout(() => (ui.captionReveal = true), 120);
  }
  function hideCaptions(delay = 250) {
    clearTimeout(captionTimer);
    captionTimer = setTimeout(() => (ui.captionReveal = false), delay);
  }
</script>

{#if single && !pageFullscreen}
  <!-- The strip above the page belongs to the chrome; hovering it shows the window controls. -->
  <div
    class="hot-caption"
    role="presentation"
    onmouseenter={revealCaptions}
    onmouseleave={() => hideCaptions(400)}
    data-tauri-drag-region
  ></div>
  {#if barOn}
    <!-- The bookmarks toolbar in the strip above the page; hovering it shows the window controls too. -->
    <div
      class="single-bar"
      role="presentation"
      onmouseenter={revealCaptions}
      onmouseleave={() => hideCaptions(400)}
      transition:fade={{ duration: 150 }}
    >
      <BookmarksBar />
    </div>
  {/if}
  {#if ui.captionReveal}
    {#if revealBar}
      <div
        class="reveal-bar"
        role="presentation"
        use:overlayLease
        onmouseenter={() => clearTimeout(captionTimer)}
        onmouseleave={() => hideCaptions()}
        transition:slidePx={{ y: -12, duration: 150, easing: cubicOut }}
      >
        <BookmarksBar floating />
      </div>
    {/if}
    <div
      class="captions"
      role="presentation"
      use:overlayLease
      onmouseenter={() => clearTimeout(captionTimer)}
      onmouseleave={() => hideCaptions()}
      transition:slidePx={{ y: -12, duration: 150, easing: cubicOut }}
    >
      <WindowControls />
    </div>
  {/if}
{/if}

<style>
  .single-bar {
    position: absolute;
    top: 0;
    left: var(--lay-left);
    right: 0;
    height: var(--bm-bar-h);
    z-index: 6;
  }
  /* The hidden toolbar of the single layout, revealed with the window controls. */
  .reveal-bar {
    position: absolute;
    top: 0;
    left: var(--lay-left);
    right: 138px;
    height: var(--bm-bar-h);
    z-index: 44;
    overflow: hidden;
    border-bottom-left-radius: var(--r-panel);
    border-bottom-right-radius: var(--r-panel);
    corner-shape: var(--sq);
    background: var(--compact-bg);
    box-shadow: var(--zen-big-shadow);
  }
  .hot-caption {
    position: absolute;
    top: 0;
    right: 0;
    left: var(--sidebar-w);
    height: var(--zen-sep);
    z-index: 5;
  }
  :global(.app.compact) .hot-caption {
    left: var(--zen-sep);
  }
  /* Sidebar on the right: the strip above the page ends at the sidebar, and the window controls
   * appear at the page's top right corner rather than over the sidebar's buttons. */
  :global(.app.side-right) .hot-caption {
    left: 0;
    right: var(--lay-right);
  }
  :global(.app.side-right.compact) .hot-caption {
    left: var(--zen-sep);
    right: 0;
  }
  :global(.app.side-right) .single-bar {
    right: var(--lay-right);
  }
  :global(.app.side-right) .reveal-bar {
    right: calc(var(--lay-right) + 138px);
  }
  .captions {
    position: absolute;
    top: 0;
    right: 0;
    height: 34px;
    z-index: 45;
    display: flex;
    overflow: hidden;
    border-bottom-left-radius: var(--r-panel);
    corner-shape: var(--sq);
    background: var(--compact-bg);
    box-shadow: var(--zen-big-shadow);
  }
  :global(.app.side-right:not(.compact)) .captions {
    right: var(--lay-right);
    border-bottom-right-radius: var(--r-panel);
  }
</style>
