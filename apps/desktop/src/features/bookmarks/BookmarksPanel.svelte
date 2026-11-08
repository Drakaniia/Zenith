<script lang="ts">
  import { tick, onMount } from 'svelte';
  import { cubicOut } from 'svelte/easing';
  import { revealX } from '$shared/ui/motion';
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import BookmarksPanelHistory from './BookmarksPanelHistory.svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { settings, PANEL_DEFAULT, PANEL_MAX, PANEL_MIN } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { webviewApi } from '$platform/webview-api';
  import { bmDnd } from './bookmark-dnd.svelte';
  import { ROOTS, ROOT_TITLES, ancestors, kids, nodeLabel, searchBookmarks } from './bookmarks';
  import { faviconFor } from '$shared/favicons';
  import { bookmarkNodeMenu, rootIcon } from './menus';
  import { copyNodes, deleteNodes, mouseHow, openBookmark, openHow, pasteNodes } from './bookmark-actions';
  import { displayHost } from '$shared/url';
  import type { BookmarkNode } from './types';
  import type { HistoryEntry } from '$features/history/types';

  // Zen's bookmarks sidebar (Ctrl+B), between the tab sidebar and the page: the whole tree with
  // a search box on top. Its title switches to the history list, as Firefox's sidebar does.
  const view = $derived(settings.value.bookmarksPanelView);
  const ix = $derived(bookmarks.index);

  let query = $state('');
  let search: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();
  let cursor = $state<number | null>(null);

  // ---------------------------------------------------------------- folders open or closed
  const KEY = 'zenith.bookmarksPanel.open';
  function loadOpen(): Set<number> {
    try {
      const raw: unknown = JSON.parse(localStorage.getItem(KEY) ?? 'null');
      if (Array.isArray(raw)) return new Set(raw.filter((n): n is number => typeof n === 'number'));
    } catch {
      /* A missing or damaged list is the default one. */
    }
    return new Set([ROOTS[0], ROOTS[1]]);
  }
  let open = $state(loadOpen());
  $effect(() => {
    try {
      localStorage.setItem(KEY, JSON.stringify([...open]));
    } catch {
      /* Not persisted: fine. */
    }
  });

  function setOpen(id: number, value: boolean) {
    const next = new Set(open);
    if (value) next.add(id);
    else next.delete(id);
    open = next;
  }

  // ---------------------------------------------------------------- the tree
  interface Row {
    node: BookmarkNode;
    depth: number;
    folders: boolean;
    open: boolean;
  }
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const walk = (id: number, depth: number) => {
      for (const n of kids(ix, id)) {
        const isFolder = n.kind === 'folder';
        const isOpen = isFolder && open.has(n.id);
        out.push({ node: n, depth, folders: isFolder && kids(ix, n.id).length > 0, open: isOpen });
        if (isOpen) walk(n.id, depth + 1);
      }
    };
    for (const r of ROOTS) {
      const n = ix.byId.get(r);
      if (!n) continue;
      const isOpen = open.has(r);
      out.push({ node: n, depth: 0, folders: kids(ix, r).length > 0, open: isOpen });
      if (isOpen) walk(r, 1);
    }
    return out;
  });

  const found = $derived(query.trim() && view === 'bookmarks' ? searchBookmarks(ix, query, { limit: 200 }) : null);

  function label(n: BookmarkNode) {
    return n.parent === null ? bookmarks.node(n.id)?.title || ROOT_TITLES[n.id] : nodeLabel(n);
  }

  function activate(e: MouseEvent, n: BookmarkNode) {
    cursor = n.id;
    if (n.kind === 'bookmark') openBookmark(n.url, mouseHow(e));
    else if (n.kind === 'folder') setOpen(n.id, !open.has(n.id));
  }

  function menu(e: MouseEvent, n: BookmarkNode) {
    e.preventDefault();
    e.stopPropagation();
    cursor = n.id;
    ui.openMenu(e.clientX, e.clientY, bookmarkNodeMenu(n));
  }

  async function keep(id: number) {
    await tick();
    listEl?.querySelector(`[data-bm-node="${id}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  function onkeydown(e: KeyboardEvent) {
    if (view !== 'bookmarks' || found) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && ['z', 'y'].includes(e.key.toLowerCase())) {
      e.preventDefault();
      if (e.key.toLowerCase() === 'y' || e.shiftKey) void bookmarks.redo();
      else void bookmarks.undo();
      return;
    }
    const at = rows.findIndex((r) => r.node.id === cursor);
    const row = rows[at];
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && ['c', 'x', 'v'].includes(e.key.toLowerCase()) && row) {
      e.preventDefault();
      const k = e.key.toLowerCase();
      if (k === 'v') {
        const into = row.node.kind === 'folder' ? row.node.id : row.node.parent;
        if (into !== null) void pasteNodes(into);
      } else copyNodes([row.node.id], k === 'x');
      return;
    }
    const go = (i: number) => {
      const r = rows[Math.max(0, Math.min(rows.length - 1, i))];
      if (r) {
        cursor = r.node.id;
        void keep(r.node.id);
      }
    };
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      go(at + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      go(at < 0 ? rows.length - 1 : at - 1);
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      go(e.key === 'Home' ? 0 : rows.length - 1);
    } else if (e.key === 'ArrowRight' && row?.node.kind === 'folder') {
      e.preventDefault();
      if (!row.open) setOpen(row.node.id, true);
      else go(at + 1);
    } else if (e.key === 'ArrowLeft' && row) {
      e.preventDefault();
      if (row.open) setOpen(row.node.id, false);
      else if (row.node.parent !== null) {
        cursor = row.node.parent;
        void keep(row.node.parent);
      }
    } else if ((e.key === 'Enter' || e.key === ' ') && row) {
      e.preventDefault();
      if (row.node.kind === 'bookmark') openBookmark(row.node.url, openHow({ ctrl: e.ctrlKey, shift: e.shiftKey, alt: e.altKey }));
      else if (row.node.kind === 'folder') setOpen(row.node.id, !row.open);
    } else if (e.key === 'Delete' && row && row.node.parent !== null) {
      e.preventDefault();
      const next = rows[at + 1] ?? rows[at - 1];
      void deleteNodes([row.node.id]);
      cursor = next?.node.id ?? null;
    } else if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
      if (!row) return;
      e.preventDefault();
      const el = listEl?.querySelector<HTMLElement>(`[data-bm-node="${row.node.id}"]`);
      const r = el?.getBoundingClientRect();
      ui.openMenu(r ? r.left + 24 : 100, r ? r.bottom : 100, bookmarkNodeMenu(row.node));
    }
  }

  // ---------------------------------------------------------------- history view
  // Kept here so the last list stays on screen while a new search runs (BookmarksPanelHistory).
  let history = $state<HistoryEntry[]>([]);

  // ---------------------------------------------------------------- header
  function viewMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const pick = (v: 'bookmarks' | 'history') => () => {
      query = '';
      settings.update({ bookmarksPanelView: v });
      void tick().then(() => search?.focus());
    };
    ui.openMenu(r.left, r.bottom + 4, [
      { label: 'Bookmarks', icon: 'bookmark', checked: view === 'bookmarks', run: pick('bookmarks') },
      { label: 'History', icon: 'history', checked: view === 'history', run: pick('history') },
    ]);
  }

  const close = () => settings.update({ bookmarksPanel: false });

  // The keyboard goes to the search box when the sidebar opens.
  onMount(() => {
    void webviewApi.focusView(null);
    void tick().then(() => search?.focus());
  });

  // ---------------------------------------------------------------- resizing
  let resizing = $state(false);
  function startResize(e: PointerEvent) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const startX = e.clientX;
    const startW = el.parentElement?.offsetWidth ?? settings.value.bookmarksPanelWidth;
    el.setPointerCapture(e.pointerId);
    resizing = true;
    const move = (ev: PointerEvent) => {
      settings.value.bookmarksPanelWidth = Math.round(Math.min(PANEL_MAX, Math.max(PANEL_MIN, startW + ev.clientX - startX)));
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      resizing = false;
      settings.save(false);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  const path = (n: BookmarkNode) =>
    ancestors(ix, n.id)
      .map((a) => a.title || ROOT_TITLES[a.id])
      .join(' / ');
</script>

<!-- It slides in and out of its own slot; the page beside it resizes once (Zen-style). -->
<aside
  class="panel"
  aria-label={view === 'history' ? 'History' : 'Bookmarks'}
  style:--panel-w="{settings.value.bookmarksPanelWidth}px"
  in:revealX={{ duration: 250 }}
  out:revealX={{ duration: 180, easing: cubicOut }}
>
  <header>
    <button class="title" onclick={viewMenu} aria-haspopup="menu">
      <Icon name={view === 'history' ? 'history' : 'bookmark'} size={18} />
      <span>{view === 'history' ? 'History' : 'Bookmarks'}</span>
      <Icon name="chevronDown" size={14} />
    </button>
    <button class="icon-btn" title="Close the sidebar" aria-label="Close the sidebar" onclick={close}><Icon name="close" size={16} /></button>
  </header>

  <div class="search">
    <Icon name="search" size={15} />
    <input
      bind:this={search}
      bind:value={query}
      placeholder={view === 'history' ? 'Search history' : 'Search bookmarks'}
      spellcheck="false"
      autocomplete="off"
      onkeydown={(e) => {
        if (e.key === 'Escape' && query) {
          e.stopPropagation();
          query = '';
        } else if (e.key === 'ArrowDown' && view === 'bookmarks' && !found) {
          e.preventDefault();
          listEl?.focus();
          if (cursor === null && rows[0]) cursor = rows[0].node.id;
        }
      }}
    />
    {#if query}
      <button class="clear" title="Clear" aria-label="Clear the search" onclick={() => ((query = ''), search?.focus())}><Icon name="close" size={13} /></button>
    {/if}
  </div>

  {#if view === 'history'}
    <BookmarksPanelHistory {query} bind:history bind:list={listEl} />
  {:else if found}
    <div class="list" bind:this={listEl}>
      {#each found as n (n.id)}
        <button
          class="row leaf"
          title={`${path(n)}\n${n.url}`}
          data-bm-node={n.id}
          onclick={(e) => activate(e, n)}
          onauxclick={(e) => e.button === 1 && openBookmark(n.url, 'background')}
          oncontextmenu={(e) => menu(e, n)}
        >
          <span class="tw"></span>
          <Favicon url={n.url} favicon={faviconFor(n.url)} size={16} />
          <span class="label ellipsis">{nodeLabel(n)}</span>
          <span class="sub ellipsis">{displayHost(n.url)}</span>
        </button>
      {:else}
        <p class="empty">No matching bookmarks</p>
      {/each}
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="list" role="tree" tabindex="0" aria-label="Bookmarks" bind:this={listEl} {onkeydown}>
      {#each rows as r (r.node.id)}
        {@const n = r.node}
        <div
          class="row"
          class:cursor={cursor === n.id}
          class:dragging={bmDnd.dragging(n.id)}
          role="treeitem"
          tabindex="-1"
          aria-level={r.depth + 1}
          aria-selected={cursor === n.id}
          aria-expanded={n.kind === 'folder' ? r.open : undefined}
          title={n.kind === 'bookmark' ? n.url : undefined}
          data-bm-node={n.id}
          data-mark={bmDnd.markFor(n.id)}
          style:padding-left="{4 + r.depth * 16}px"
          onclick={(e) => activate(e, n)}
          onauxclick={(e) => e.button === 1 && n.kind === 'bookmark' && openBookmark(n.url, 'background')}
          oncontextmenu={(e) => menu(e, n)}
          onkeydown={() => {}}
          onpointerdown={(e) =>
            n.parent !== null && bmDnd.start(e, [n.id], { label: nodeLabel(n), url: n.url, folder: n.kind === 'folder' })}
        >
          {#if n.kind === 'separator'}
            <span class="tw"></span>
            <span class="rule"></span>
          {:else}
            <button
              class="tw"
              class:hidden={!r.folders}
              tabindex="-1"
              aria-label={r.open ? 'Collapse' : 'Expand'}
              onclick={(e) => {
                e.stopPropagation();
                setOpen(n.id, !r.open);
              }}
            >
              <Icon name={r.open ? 'chevronDown' : 'chevronRight'} size={13} />
            </button>
            {#if n.parent === null}
              <Icon name={rootIcon(n.id)} size={17} />
            {:else if n.kind === 'folder'}
              <Icon name={r.open ? 'folderOpen' : 'folder'} size={17} />
            {:else}
              <Favicon url={n.url} favicon={faviconFor(n.url)} size={16} />
            {/if}
            <span class="label ellipsis">{label(n)}</span>
          {/if}
        </div>
      {/each}
      {#if bookmarks.loaded && bookmarks.count === 0}
        <p class="empty">
          No bookmarks yet. Press Ctrl+D on a page to add one, or
          <button class="link" onclick={() => (ui.bookmarkImport = true)}>import your bookmarks</button>.
        </p>
      {/if}
    </div>
  {/if}

  <div
    class="splitter"
    class:resizing
    role="separator"
    aria-orientation="vertical"
    title="Drag to resize, double-click to reset"
    onpointerdown={startResize}
    ondblclick={() => settings.update({ bookmarksPanelWidth: PANEL_DEFAULT })}
  ></div>
</aside>

<style>
  .panel {
    position: relative;
    flex: none;
    width: var(--panel-w);
    max-width: 60%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-content);
    background: var(--page-bg);
    box-shadow: var(--zen-big-shadow);
    overflow: hidden;
  }
  header {
    flex: none;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 8px 0 8px;
    border-bottom: 1px solid var(--line);
  }
  .title {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 8px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    font: 500 15px/1 var(--font);
  }
  .title:hover {
    background: var(--hover);
  }
  .search {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    margin: 8px 8px 4px;
    padding: 0 8px 0 10px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-3);
  }
  .search:focus-within {
    background: var(--field-focus);
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 13px;
  }
  .clear {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 9px;
    color: var(--text-3);
  }
  .clear:hover {
    background: var(--hover);
    color: var(--text);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 4px 8px;
    outline: none;
  }
  .row {
    position: relative;
    width: 100%;
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 28px;
    padding-right: 8px;
    border-radius: var(--r-item);
    corner-shape: var(--sq);
    text-align: left;
    font-size: 13px;
    color: var(--text);
  }
  .row.leaf {
    padding-left: 4px;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.cursor {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .list:not(:focus-within) .row.cursor {
    background: var(--hover);
  }
  .row.dragging {
    opacity: 0.45;
  }
  .tw {
    width: 18px;
    height: 24px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .tw.hidden {
    visibility: hidden;
  }
  .label {
    min-width: 0;
    flex: 1;
  }
  .sub {
    flex: 0 1 38%;
    min-width: 0;
    font-size: 11.5px;
    color: var(--text-3);
    text-align: right;
  }
  .rule {
    flex: 1;
    height: 1px;
    background: var(--separator);
  }
  .empty {
    margin: 24px 16px;
    text-align: center;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-3);
  }
  .link {
    color: var(--accent-fg);
    text-decoration: underline;
  }

  [data-mark='before']::before,
  [data-mark='after']::after {
    content: '';
    position: absolute;
    left: 8px;
    right: 4px;
    height: 2px;
    border-radius: 1px;
    background: var(--drag-indicator);
    pointer-events: none;
  }
  [data-mark='before']::before {
    top: -1px;
  }
  [data-mark='after']::after {
    bottom: -1px;
  }
  .row[data-mark='into'] {
    background: color-mix(in srgb, var(--accent) 28%, transparent);
    outline: 1px solid color-mix(in srgb, var(--accent) 60%, transparent);
  }

  .splitter {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 5px;
    cursor: ew-resize;
    background: var(--accent-button);
    opacity: 0;
    transition: opacity 0.2s ease-in-out;
  }
  .splitter:hover,
  .splitter.resizing {
    opacity: 0.7;
    transition-delay: 0.2s;
  }
  .splitter.resizing {
    transition-delay: 0s;
  }
</style>
