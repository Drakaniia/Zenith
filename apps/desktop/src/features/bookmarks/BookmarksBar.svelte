<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { bmDnd } from './bookmark-dnd.svelte';
  import { dnd } from '$features/sidebar/sidebar-dnd.svelte';
  import { ROOT_OTHER, ROOT_TITLES, ROOT_TOOLBAR, nodeLabel } from './bookmarks';
  import { faviconFor } from '$shared/favicons';
  import { bookmarkFolderItems, bookmarkItem, bookmarkNodeMenu, bookmarksBarMenu, rootIcon } from './menus';
  import { mouseHow, openBookmark } from './bookmark-actions';
  import type { BookmarkNode } from './types';

  // Zen's bookmarks toolbar: the Bookmarks Toolbar folder as a row of buttons (a bookmark opens
  // on click, a folder drops down), a » menu for what doesn't fit, and Other Bookmarks at the end.
  let { floating = false }: { floating?: boolean } = $props();

  const items = $derived(bookmarks.visibleKids(ROOT_TOOLBAR));
  const empty = $derived(bookmarks.kids(ROOT_TOOLBAR).length === 0);
  const other = $derived(settings.value.bookmarksOther && bookmarks.kids(ROOT_OTHER).length > 0);
  const otherTitle = $derived(bookmarks.node(ROOT_OTHER)?.title || ROOT_TITLES[ROOT_OTHER]);

  /** The folder whose menu is open. */
  let openId = $state<number | null>(null);
  $effect(() => {
    if (!ui.menu) openId = null;
  });

  // ---------------------------------------------------------------- overflow
  // The buttons are laid out in one row that wraps; whatever wraps is hidden and goes into a »
  // menu instead.
  let itemsEl: HTMLDivElement | undefined = $state();
  let cut = $state(-1);

  async function measure() {
    if (!itemsEl) return;
    await tick();
    const els = [...itemsEl.querySelectorAll<HTMLElement>(':scope > [data-bm-node]')];
    cut = els.findIndex((el) => el.offsetTop > 6);
  }

  $effect(() => {
    if (!itemsEl) return;
    const ro = new ResizeObserver(() => void measure());
    ro.observe(itemsEl);
    return () => ro.disconnect();
  });
  // Titles or the list changing change what fits.
  $effect(() => {
    void items.map((n) => n.title + n.kind).join('\u0001');
    void other;
    void measure();
  });

  function overflowMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    ui.openMenu(Math.max(6, r.right - 240), r.bottom + 4, items.slice(Math.max(cut, 0)).map(bookmarkItem));
  }

  // ---------------------------------------------------------------- actions
  function openFolder(n: BookmarkNode, el: HTMLElement) {
    const r = el.getBoundingClientRect();
    openId = n.id;
    ui.openMenu(r.left, r.bottom + 2, bookmarkFolderItems(n.id));
  }

  function click(e: MouseEvent, n: BookmarkNode) {
    if (n.kind === 'bookmark') openBookmark(n.url, mouseHow(e));
    else if (n.kind === 'folder' && e.button === 0) openFolder(n, e.currentTarget as HTMLElement);
  }

  function nodeMenu(e: MouseEvent, n: BookmarkNode) {
    e.preventDefault();
    e.stopPropagation();
    ui.openMenu(e.clientX, e.clientY, bookmarkNodeMenu(n));
  }

  function barMenu(e: MouseEvent) {
    e.preventDefault();
    ui.openMenu(e.clientX, e.clientY, bookmarksBarMenu());
  }

  const grab = (e: PointerEvent, n: BookmarkNode) =>
    bmDnd.start(e, [n.id], { label: nodeLabel(n), url: n.url, folder: n.kind === 'folder' });
</script>

{#snippet entry(n: BookmarkNode)}
  {#if n.kind === 'separator'}
    <span class="sepr" data-bm-node={n.id} data-mark={bmDnd.markFor(n.id)} role="separator" onpointerdown={(e) => grab(e, n)}></span>
  {:else}
    <button
      class="bm"
      class:open={openId === n.id}
      class:dragging={bmDnd.dragging(n.id)}
      data-bm-node={n.id}
      data-mark={bmDnd.markFor(n.id)}
      title={n.kind === 'bookmark' ? `${nodeLabel(n)}\n${n.url}` : nodeLabel(n)}
      onclick={(e) => click(e, n)}
      onauxclick={(e) => e.button === 1 && click(e, n)}
      oncontextmenu={(e) => nodeMenu(e, n)}
      onpointerdown={(e) => grab(e, n)}
    >
      {#if n.kind === 'folder'}
        <Icon name="folder" size={16} />
      {:else}
        <Favicon url={n.url} favicon={faviconFor(n.url)} size={16} />
      {/if}
      <span class="t ellipsis">{nodeLabel(n)}</span>
    </button>
  {/if}
{/snippet}

<div
  class="bar"
  class:floating
  class:tabdrop={dnd.drag?.started && dnd.drag.target?.zone === 'bookmarks'}
  data-bm-bar
  role="toolbar"
  aria-label="Bookmarks"
  tabindex="-1"
  data-tauri-drag-region
  oncontextmenu={barMenu}
>
  <div class="items" bind:this={itemsEl} data-bm-list={ROOT_TOOLBAR} data-bm-axis="x" data-tauri-drag-region>
    {#each items as n (n.id)}
      {@render entry(n)}
    {/each}
    {#if empty}
      {#if bookmarks.count === 0}
        <button class="bm import" onclick={() => (ui.bookmarkImport = true)}>
          <Icon name="importBox" size={15} />
          <span class="t">Import bookmarks…</span>
        </button>
      {/if}
      <span class="hint ellipsis" data-tauri-drag-region>For quick access, put your bookmarks here on the bookmarks toolbar.</span>
    {/if}
  </div>
  {#if cut >= 0}
    <button class="bm more" title="More bookmarks" aria-label="More bookmarks" onclick={overflowMenu}>
      <Icon name="chevronsRight" size={15} />
    </button>
  {/if}
  {#if other}
    <button
      class="bm other"
      class:open={openId === ROOT_OTHER}
      data-bm-node={ROOT_OTHER}
      data-mark={bmDnd.markFor(ROOT_OTHER)}
      onclick={(e) => openFolder(bookmarks.node(ROOT_OTHER)!, e.currentTarget as HTMLElement)}
      oncontextmenu={(e) => bookmarks.node(ROOT_OTHER) && nodeMenu(e, bookmarks.node(ROOT_OTHER)!)}
    >
      <Icon name={rootIcon(ROOT_OTHER)} size={16} />
      <span class="t ellipsis">{otherTitle}</span>
    </button>
  {/if}
</div>

<style>
  .bar {
    height: var(--bm-bar-h);
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 var(--zen-sep);
    color: var(--toolbar-text);
    font-size: 12.5px;
  }
  /* A tab is being dragged over the toolbar: it would be bookmarked here. */
  .bar.tabdrop {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    outline: 1px dashed color-mix(in srgb, var(--accent) 60%, transparent);
    outline-offset: -2px;
  }
  .items {
    /* The offset parent of the buttons: a button on a second (wrapped) row has offsetTop > 0. */
    position: relative;
    flex: 1;
    min-width: 0;
    height: 26px;
    display: flex;
    flex-wrap: wrap;
    align-content: flex-start;
    align-items: center;
    gap: 2px;
    overflow: hidden;
  }
  .bm {
    position: relative;
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    max-width: 200px;
    padding: 0 8px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    color: inherit;
    transition: background-color var(--fast) ease;
  }
  .bm:hover,
  .bm.open {
    background: var(--hover);
  }
  .bm:active {
    background: var(--press);
  }
  .bm.dragging {
    opacity: 0.45;
  }
  .t {
    min-width: 0;
  }
  .bm.import {
    color: var(--toolbar-text);
  }
  .bm.more {
    padding: 0 6px;
  }
  .hint {
    min-width: 0;
    padding: 0 6px;
    color: var(--text-3);
  }
  .sepr {
    flex: none;
    width: 9px;
    height: 26px;
    position: relative;
  }
  .sepr::after {
    content: '';
    position: absolute;
    left: 4px;
    top: 5px;
    bottom: 5px;
    width: 1px;
    background: var(--separator);
  }

  /* Drop marks: a line where the dragged bookmark would go, or the folder lit up. */
  [data-mark='before']::before,
  [data-mark='after']::before {
    content: '';
    position: absolute;
    top: 3px;
    bottom: 3px;
    width: 2px;
    border-radius: 1px;
    background: var(--drag-indicator);
    pointer-events: none;
  }
  [data-mark='before']::before {
    left: -2px;
  }
  [data-mark='after']::before {
    right: -2px;
  }
  .bm[data-mark='into'] {
    background: color-mix(in srgb, var(--accent) 28%, transparent);
    outline: 1px solid color-mix(in srgb, var(--accent) 60%, transparent);
  }
</style>
