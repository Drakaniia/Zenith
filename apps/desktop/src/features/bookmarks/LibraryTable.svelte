<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { type MenuItem } from '$shared/ui/menu';
  import { bmDnd } from './bookmark-dnd.svelte';
  import { ROOTS, ROOT_TITLES, compareText, folderPath, formatTags, hasTag, isInside, kids, nodeLabel, searchBookmarks } from './bookmarks';
  import { faviconFor } from '$shared/favicons';
  import { bookmarkNodeMenu, rootIcon } from './menus';
  import { copyNodes, deleteNodes, editNode, mouseHow, openBookmark, pasteNodes } from './bookmark-actions';
  import { COLS, date, type Col, type Scope, type SortBy } from './library';
  import type { BookmarkNode } from './types';

  // The Library's table: the contents of the chosen folder or tag (or the search results) with
  // the chosen columns, sorting, selection, keyboard use and dragging.

  let {
    scope,
    query,
    cols,
    sortBy,
    sortDir,
    setSort,
    selected = $bindable(),
    anchor = $bindable(),
    picked,
    one,
    go,
    newParent,
    organizeMenu,
  }: {
    scope: Scope;
    query: string;
    cols: Col[];
    sortBy: SortBy;
    sortDir: 1 | -1;
    setSort: (by: SortBy) => void;
    selected: number[];
    anchor: number | null;
    /** The selected nodes, in selection order. */
    picked: BookmarkNode[];
    one: BookmarkNode | undefined;
    go: (next: Scope) => void;
    /** Where new and pasted bookmarks go. */
    newParent: () => number;
    organizeMenu: () => MenuItem[];
  } = $props();

  const ix = $derived(bookmarks.index);
  let tableEl: HTMLDivElement | undefined = $state();

  // ---------------------------------------------------------------- the table
  const scopeFolder = $derived(scope.kind === 'folder' ? scope.id : undefined);
  const searching = $derived(query.trim().length > 0);
  const listed = $derived.by<BookmarkNode[]>(() => {
    const s = scope;
    if (s.kind === 'history' || s.kind === 'downloads') return [];
    if (searching) {
      const found = searchBookmarks(ix, query, { within: scopeFolder });
      return s.kind === 'tag' ? found.filter((n) => hasTag(n, s.tag)) : found;
    }
    if (s.kind === 'all') return ROOTS.map((r) => ix.byId.get(r)).filter((n): n is BookmarkNode => !!n);
    if (s.kind === 'tag') {
      return [...ix.byId.values()]
        .filter((n) => n.kind === 'bookmark' && hasTag(n, s.tag))
        .sort((a, b) => compareText(nodeLabel(a), nodeLabel(b)));
    }
    return [...kids(ix, s.id)];
  });

  function sortKey(n: BookmarkNode, by: SortBy): string | number {
    switch (by) {
      case 'name':
        return nodeLabel(n);
      case 'tags':
        return formatTags(n.tags);
      case 'location':
        return n.url;
      case 'keyword':
        return n.keyword;
      case 'added':
        return n.created;
      case 'modified':
        return n.modified;
      default:
        return 0;
    }
  }

  const rows = $derived.by(() => {
    if (sortBy === 'none') return listed;
    const dir = sortDir;
    const by = sortBy;
    return [...listed].sort((a, b) => {
      if (by === 'name' && a.kind !== b.kind) return a.kind === 'folder' ? -1 : b.kind === 'folder' ? 1 : 0;
      const x = sortKey(a, by);
      const y = sortKey(b, by);
      return (typeof x === 'number' && typeof y === 'number' ? x - y : compareText(String(x), String(y))) * dir;
    });
  });

  /** The visible order can be changed by dragging only where it is the real order. */
  const reorderable = $derived(scope.kind === 'folder' && !searching && sortBy === 'none');

  const columns = $derived(
    `minmax(170px, 2.2fr)${cols.map(() => ' minmax(90px, 1fr)').join('')}${searching ? ' minmax(110px, 1fr)' : ''}`,
  );

  function cell(n: BookmarkNode, c: Col): string {
    if (c === 'tags') return formatTags(n.tags);
    if (c === 'location') return n.kind === 'bookmark' ? n.url : '';
    if (c === 'keyword') return n.keyword;
    return n.kind === 'separator' ? '' : date(c === 'added' ? n.created : n.modified);
  }

  // ---------------------------------------------------------------- selecting and opening
  function pick(e: MouseEvent, n: BookmarkNode) {
    if (e.shiftKey && anchor !== null) {
      const a = rows.findIndex((r) => r.id === anchor);
      const b = rows.findIndex((r) => r.id === n.id);
      if (a >= 0 && b >= 0) selected = rows.slice(Math.min(a, b), Math.max(a, b) + 1).map((r) => r.id);
    } else if (e.ctrlKey || e.metaKey) {
      selected = selected.includes(n.id) ? selected.filter((i) => i !== n.id) : [...selected, n.id];
      anchor = n.id;
    } else {
      selected = [n.id];
      anchor = n.id;
    }
  }

  function openRow(n: BookmarkNode, e?: MouseEvent | KeyboardEvent) {
    if (n.kind === 'bookmark') {
      const how = e instanceof MouseEvent ? mouseHow(e) : 'tab';
      // The Library is a tab itself: its bookmarks open in tabs of their own.
      openBookmark(n.url, how === 'here' ? 'tab' : how);
    } else if (n.kind === 'folder') go({ kind: 'folder', id: n.id });
  }

  function rowMenu(e: MouseEvent, n: BookmarkNode) {
    e.preventDefault();
    e.stopPropagation();
    if (!selected.includes(n.id)) {
      selected = [n.id];
      anchor = n.id;
    }
    ui.openMenu(e.clientX, e.clientY, bookmarkNodeMenu(n, picked));
  }

  export function deleteSelected() {
    const ids = picked.filter((n) => n.parent !== null).map((n) => n.id);
    if (!ids.length) return;
    const i = rows.findIndex((r) => r.id === ids[0]);
    void deleteNodes(ids);
    const next = rows.find((r, j) => j > i && !ids.includes(r.id)) ?? [...rows].reverse().find((r) => !ids.includes(r.id));
    selected = next ? [next.id] : [];
    anchor = next?.id ?? null;
  }

  function onkeydown(e: KeyboardEvent) {
    if ((e.target as HTMLElement).closest('input')) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === 'z' || e.key === 'Z' || e.key === 'y' || e.key === 'Y')) {
      e.preventDefault();
      if (e.key.toLowerCase() === 'y' || e.shiftKey) void bookmarks.redo();
      else void bookmarks.undo();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && ['c', 'x', 'v'].includes(e.key.toLowerCase())) {
      e.preventDefault();
      const k = e.key.toLowerCase();
      if (k === 'v') {
        const into = one?.kind === 'folder' ? one.id : newParent();
        void pasteNodes(into);
      } else copyNodes(picked.map((n) => n.id), k === 'x');
      return;
    }
    const at = rows.findIndex((r) => r.id === (selected[selected.length - 1] ?? -1));
    const move = (to: number, extend: boolean) => {
      const r = rows[Math.max(0, Math.min(rows.length - 1, to))];
      if (!r) return;
      if (extend && anchor !== null) {
        const a = rows.findIndex((x) => x.id === anchor);
        const b = rows.indexOf(r);
        selected = rows.slice(Math.min(a, b), Math.max(a, b) + 1).map((x) => x.id);
      } else {
        selected = [r.id];
        anchor = r.id;
      }
      tableEl?.querySelector(`[data-row="${r.id}"]`)?.scrollIntoView({ block: 'nearest' });
    };
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      move(at + 1, e.shiftKey);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      move(at < 0 ? rows.length - 1 : at - 1, e.shiftKey);
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      move(e.key === 'Home' ? 0 : rows.length - 1, e.shiftKey);
    } else if (e.key === 'a' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      selected = rows.map((r) => r.id);
    } else if (e.key === 'Enter' && picked[0]) {
      e.preventDefault();
      openRow(picked[0], e);
    } else if (e.key === 'Delete') {
      e.preventDefault();
      deleteSelected();
    } else if (e.key === 'F2' && picked.length === 1 && picked[0].kind !== 'separator') {
      e.preventDefault();
      editNode(picked[0]);
    } else if (e.key === 'Escape' && selected.length) {
      e.stopPropagation();
      selected = [];
    }
  }

  const empty = $derived(
    searching ? 'No matching bookmarks' : scope.kind === 'tag' ? 'No bookmarks with this tag' : 'There are no bookmarks here yet',
  );

  const grab = (e: PointerEvent, n: BookmarkNode) => {
    if (n.parent === null) return;
    const ids = selected.includes(n.id) ? picked.filter((p) => p.parent !== null).map((p) => p.id) : [n.id];
    bmDnd.start(e, ids, { label: ids.length > 1 ? `${ids.length} items` : nodeLabel(n), url: n.url, folder: n.kind === 'folder' });
  };

  /** The folder a search result is in. */
  const where = (n: BookmarkNode) => folderPath(ix, n.id).split(' / ').slice(-1)[0] ?? '';
  const inside = (n: BookmarkNode) => scope.kind === 'folder' && isInside(ix, n.id, scope.id);
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="table"
  role="grid"
  tabindex="0"
  aria-label="Bookmarks"
  aria-multiselectable="true"
  bind:this={tableEl}
  {onkeydown}
  data-bm-list={reorderable && scope.kind === 'folder' ? scope.id : undefined}
  oncontextmenu={(e) => {
    e.preventDefault();
    ui.openMenu(e.clientX, e.clientY, organizeMenu());
  }}
  onpointerdown={(e) => e.target === e.currentTarget && (selected = [])}
>
  <div class="head" style:grid-template-columns={columns} role="row">
    <button class="th" role="columnheader" onclick={() => setSort('name')}>
      Name{#if sortBy === 'name'}<Icon name={sortDir === 1 ? 'arrowUp' : 'arrowDown'} size={12} />{/if}
    </button>
    {#each cols as c (c)}
      <button class="th" role="columnheader" onclick={() => setSort(c)}>
        {COLS.find((k) => k.id === c)?.label}{#if sortBy === c}<Icon name={sortDir === 1 ? 'arrowUp' : 'arrowDown'} size={12} />{/if}
      </button>
    {/each}
    {#if searching}<span class="th static" role="columnheader">Folder</span>{/if}
  </div>
  <div class="body">
    {#each rows as n (n.id)}
      {@const on = selected.includes(n.id)}
      <div
        class="trow"
        class:on
        class:dragging={bmDnd.dragging(n.id)}
        class:muted={!searching && scope.kind !== 'folder' && !inside(n)}
        style:grid-template-columns={columns}
        role="row"
        tabindex="-1"
        aria-selected={on}
        data-row={n.id}
        data-bm-node={reorderable ? n.id : undefined}
        data-bm-into={!reorderable && n.kind === 'folder' ? n.id : undefined}
        data-mark={bmDnd.markFor(n.id)}
        onclick={(e) => pick(e, n)}
        ondblclick={(e) => openRow(n, e)}
        onauxclick={(e) => e.button === 1 && n.kind === 'bookmark' && openBookmark(n.url, 'background')}
        oncontextmenu={(e) => rowMenu(e, n)}
        onpointerdown={(e) => grab(e, n)}
        onkeydown={() => {}}
      >
        <span class="td name" role="gridcell">
          {#if n.kind === 'separator'}
            <span class="rule"></span>
          {:else}
            {#if n.kind === 'folder'}
              <Icon name={n.parent === null ? rootIcon(n.id) : 'folder'} size={17} />
            {:else}
              <Favicon url={n.url} favicon={faviconFor(n.url)} size={16} />
            {/if}
            <span class="ellipsis">{n.parent === null ? bookmarks.node(n.id)?.title || ROOT_TITLES[n.id] : nodeLabel(n)}</span>
          {/if}
        </span>
        {#each cols as c (c)}
          <span class="td ellipsis" role="gridcell" title={cell(n, c)}>{cell(n, c)}</span>
        {/each}
        {#if searching}<span class="td ellipsis" role="gridcell">{where(n)}</span>{/if}
      </div>
    {:else}
      <p class="empty">
        <Icon name="bookmarkStar" size={26} />
        {empty}
      </p>
    {/each}
  </div>
</div>

<style>
  .table {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    outline: none;
  }
  .head {
    flex: none;
    display: grid;
    border-bottom: 1px solid var(--line-strong);
    background: color-mix(in srgb, var(--text) 4%, transparent);
  }
  .th {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    text-align: left;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-2);
    border-right: 1px solid var(--line);
  }
  .th:hover:not(.static) {
    color: var(--text);
    background: var(--hover);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .trow {
    position: relative;
    display: grid;
    align-items: center;
    min-height: 30px;
    font-size: 13px;
  }
  .trow:nth-child(even) {
    background: color-mix(in srgb, var(--text) 3%, transparent);
  }
  .trow:hover {
    background: var(--hover);
  }
  .trow.on {
    background: color-mix(in srgb, var(--accent) 34%, transparent);
  }
  .table:not(:focus-within) .trow.on {
    background: color-mix(in srgb, var(--text) 14%, transparent);
  }
  .trow.dragging {
    opacity: 0.45;
  }
  .trow.muted {
    color: var(--text-2);
  }
  .td {
    min-width: 0;
    padding: 0 10px;
    color: var(--text-2);
  }
  .td.name {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }
  .rule {
    flex: 1;
    height: 1px;
    background: var(--separator);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    margin: 0;
    padding: 60px 0;
    color: var(--text-3);
  }
  [data-mark='before']::before,
  [data-mark='after']::after {
    content: '';
    position: absolute;
    left: 6px;
    right: 6px;
    height: 2px;
    border-radius: 1px;
    background: var(--drag-indicator);
    pointer-events: none;
    z-index: 2;
  }
  [data-mark='before']::before {
    top: -1px;
  }
  [data-mark='after']::after {
    bottom: -1px;
  }
  .trow[data-mark='into'] {
    outline: 1px solid color-mix(in srgb, var(--accent) 70%, transparent);
    outline-offset: -1px;
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
</style>
