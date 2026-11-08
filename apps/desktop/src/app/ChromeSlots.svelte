<script lang="ts">
  // The docked chrome: the sidebar (or tab strip) and the toolbar row above the page.
  import { fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { foldY, slidePx, slideX, springEase } from '$shared/ui/motion';
  import type { ToolbarMode } from '$features/settings/settings.svelte';
  import type { SidebarPosition } from '$features/sidebar/sidebar-position';
  import Sidebar from '$features/sidebar/Sidebar.svelte';
  import TabStrip from '$features/sidebar/TabStrip.svelte';
  import TopBar from '$features/toolbar/TopBar.svelte';
  import BookmarksBar from '$features/bookmarks/BookmarksBar.svelte';
  import type { LayoutMotion } from './layout-motion';

  let {
    showSidebar,
    strip,
    side,
    mode,
    stripHasControls,
    stripIsToolbar,
    showTopBar,
    barOn,
    motion,
  }: {
    showSidebar: boolean;
    strip: boolean;
    side: SidebarPosition;
    mode: ToolbarMode;
    stripHasControls: boolean;
    stripIsToolbar: boolean;
    showTopBar: boolean;
    barOn: boolean;
    /** The layout animation now (all 0 when nothing animates). */
    motion: () => LayoutMotion;
  } = $props();

  // The toolbar row: the toolbar compact mode hides folds into the top edge (Zen); other layout
  // changes slide it.
  function toolbarMove(node: Element, { out }: { out: boolean }) {
    const m = motion();
    if (m.fold) return foldY(node, { duration: m.bar });
    return slidePx(node, { y: -16, duration: out ? m.bar * 0.6 : m.bar, easing: out ? cubicOut : springEase });
  }
</script>

{#if showSidebar}
  {#if strip}
    <div
      class="strip-slot"
      in:slidePx={{ y: side === 'top' ? -16 : 16, duration: motion().side, easing: springEase }}
      out:slidePx={{ y: side === 'top' ? -16 : 16, duration: motion().side * 0.6, easing: cubicOut }}
    >
      <TabStrip edge={side === 'top' ? 'top' : 'bottom'} collapsed={mode === 'collapsed'} controls={stripHasControls} merged={stripIsToolbar} />
    </div>
  {:else}
    <div
      class="side-slot"
      in:slideX={{ from: side === 'right' ? 1 : -1, duration: motion().side, easing: springEase }}
      out:slideX={{ from: side === 'right' ? 1 : -1, duration: motion().side, easing: springEase }}
    >
      <Sidebar />
    </div>
  {/if}
{/if}
{#if showTopBar || (stripIsToolbar && barOn)}
  <div class="toolbar-slot" in:toolbarMove={{ out: false }} out:toolbarMove={{ out: true }}>
    {#if showTopBar}
      <TopBar controls={!stripHasControls} />
    {/if}
    {#if barOn}
      <div class="bar-row" transition:fade={{ duration: 150 }}>
        <BookmarksBar />
      </div>
    {/if}
  </div>
{/if}

<style>
  .side-slot {
    position: absolute;
    left: 0;
    top: var(--lay-side-top);
    bottom: 0;
    display: flex;
    z-index: 1;
  }
  :global(.app.side-right) .side-slot {
    left: auto;
    right: 0;
  }
  .side-slot > :global(aside.sidebar) {
    height: 100%;
  }
  .strip-slot {
    position: absolute;
    left: 0;
    right: 0;
    height: var(--strip-h);
    z-index: 1;
  }
  :global(.app.side-top) .strip-slot {
    top: 0;
  }
  :global(.app.side-bottom) .strip-slot {
    bottom: 0;
  }
  .toolbar-slot {
    position: absolute;
    top: var(--lay-bar-top);
    left: var(--lay-bar-left);
    right: 0;
    display: flex;
    flex-direction: column;
    min-width: 0;
    z-index: 1;
  }
  /* Under the toolbar the row spans the page area, not the collapsed layout's icon sidebar. */
  .bar-row {
    margin-left: calc(var(--lay-left) - var(--lay-bar-left));
  }
  :global(.app.side-right) .bar-row {
    margin-right: var(--lay-right);
  }
</style>
