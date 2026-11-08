<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import HistoryPage from '$features/history/HistoryPage.svelte';
  import DownloadsPage from '$features/downloads/DownloadsPage.svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { type MenuItem } from '$shared/ui/menu';
  import { bookmarksApi } from './api';
  import { bmDnd } from './bookmark-dnd.svelte';
  import {
    ROOTS,
    ROOT_TITLES,
    ancestors,
    compareText,
    folderPath,
    formatTags,
    hasTag,
    isInside,
    kids,
    nodeLabel,
    parseTags,
    searchBookmarks,
  } from './bookmarks';
  import { faviconFor } from '$shared/favicons';
  import { bookmarkNodeMenu, rootIcon } from '../../app/menus';
  import {
    backupBookmarksFile,
    copyNodes,
    deleteNodes,
    editNode,
    exportBookmarksFile,
    importBookmarksFile,
    mouseHow,
    newFolderDialog,
    openAll,
    openBookmark,
    pasteNodes,
    restoreBookmarks,
  } from './bookmark-actions';
  import { isWebUrl } from '$shared/url';
  import type { BookmarkBackup, BookmarkNode, BookmarkPatch } from './types';

  // The Bookmarks Library (Ctrl+Shift+O), laid out like Zen's: a tree of History, Downloads, tags
  // and all bookmarks on the left, the contents of the chosen folder as a table (Name, Tags,
  // Location) on the right, and the chosen bookmark's Name, URL, Tags and Keyword below it.
  type Scope =
    | { kind: 'all' }
    | { kind: 'folder'; id: number }
    | { kind: 'tag'; tag: string }
    | { kind: 'history' }
    | { kind: 'downloads' };

  type Col = 'tags' | 'location' | 'keyword' | 'added' | 'modified';
  type SortBy = 'none' | 'name' | Col;
  const COLS: { id: Col; label: string }[] = [
    { id: 'tags', label: 'Tags' },
    { id: 'location', label: 'Location' },
    { id: 'keyword', label: 'Keyword' },
    { id: 'added', label: 'Added' },
    { id: 'modified', label: 'Last modified' },
  ];

  const ix = $derived(bookmarks.index);

  // ---------------------------------------------------------------- remembered view settings
  const KEY = 'zenith.library';
  function loadView(): { cols: Col[]; by: SortBy; dir: 1 | -1 } {
    try {
      const v = JSON.parse(localStorage.getItem(KEY) ?? 'null');
      const ids = COLS.map((c) => c.id);
      return {
        cols: Array.isArray(v?.cols) ? v.cols.filter((c: Col) => ids.includes(c)) : ['tags', 'location'],
        by: ['none', 'name', ...ids].includes(v?.by) ? v.by : 'none',
        dir: v?.dir === -1 ? -1 : 1,
      };
    } catch {
      return { cols: ['tags', 'location'], by: 'none', dir: 1 };
    }
  }
  const view0 = loadView();
  let cols = $state<Col[]>(view0.cols);
  let sortBy = $state<SortBy>(view0.by);
  let sortDir = $state<1 | -1>(view0.dir);
  $effect(() => {
    try {
      localStorage.setItem(KEY, JSON.stringify({ cols, by: sortBy, dir: sortDir }));
    } catch {
      /* Not remembered: fine. */
    }
  });

  // ---------------------------------------------------------------- where we are
  let scope = $state<Scope>({ kind: 'folder', id: ROOTS[0] });
  let nav = $state<Scope[]>([{ kind: 'folder', id: ROOTS[0] }]);
  let navAt = $state(0);
  let query = $state('');
  let selected = $state<number[]>([]);
  let anchor = $state<number | null>(null);
  let treeOpen = $state(new Set<string>(['all']));
  let tableEl: HTMLDivElement | undefined = $state();

  function go(next: Scope, push = true) {
    scope = next;
    selected = [];
    anchor = null;
    query = '';
    if (push) {
      nav = [...nav.slice(0, navAt + 1), next];
      navAt = nav.length - 1;
    }
    if (next.kind === 'folder') {
      const t = new Set(treeOpen);
      t.add('all');
      for (const a of ancestors(ix, next.id)) t.add(`f${a.id}`);
      treeOpen = t;
    }
  }
  const back = () => navAt > 0 && go(nav[--navAt], false);
  const forward = () => navAt < nav.length - 1 && go(nav[++navAt], false);

  // A folder that was deleted (here or in another window) leaves the view on the toolbar folder.
  $effect(() => {
    if (!bookmarks.loaded) return;
    const s = scope;
    if (s.kind === 'folder' && !ix.byId.has(s.id)) untrack(() => go({ kind: 'folder', id: ROOTS[0] }, false));
  });

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

  const picked = $derived(selected.map((id) => ix.byId.get(id)).filter((n): n is BookmarkNode => !!n));
  const columns = $derived(
    `minmax(170px, 2.2fr)${cols.map(() => ' minmax(90px, 1fr)').join('')}${searching ? ' minmax(110px, 1fr)' : ''}`,
  );

  function setSort(by: SortBy) {
    if (sortBy === by && by !== 'none') sortDir = sortDir === 1 ? -1 : 1;
    else {
      sortBy = by;
      sortDir = by === 'added' || by === 'modified' ? -1 : 1;
    }
  }

  const date = (ms: number) => new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });

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

  function deleteSelected() {
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

  // ---------------------------------------------------------------- the details pane
  const one = $derived(picked.length === 1 ? picked[0] : undefined);
  let dName = $state('');
  let dUrl = $state('');
  let dTags = $state('');
  let dKeyword = $state('');
  const signature = $derived(one ? [one.id, one.title, one.url, one.tags.join(','), one.keyword].join('\u0001') : '');
  $effect(() => {
    void signature;
    untrack(() => {
      dName = one?.title ?? '';
      dUrl = one?.url ?? '';
      dTags = formatTags(one?.tags ?? []);
      dKeyword = one?.keyword ?? '';
    });
  });

  async function commit(field: 'title' | 'url' | 'tags' | 'keyword') {
    const n = one;
    if (!n) return;
    const patch: BookmarkPatch = {};
    if (field === 'title' && dName.trim() !== n.title) patch.title = dName.trim();
    if (field === 'url' && n.kind === 'bookmark' && dUrl.trim() !== n.url) {
      if (!isWebUrl(dUrl.trim())) {
        ui.toast('Bookmarks need an http(s) address', { icon: 'alert' });
        dUrl = n.url;
        return;
      }
      patch.url = dUrl.trim();
    }
    if (field === 'tags') {
      const tags = parseTags(dTags);
      if (formatTags(tags) !== formatTags(n.tags)) patch.tags = tags;
    }
    if (field === 'keyword') {
      const word = dKeyword.trim().toLowerCase();
      if (/\s/.test(word)) {
        ui.toast("A keyword can't contain spaces", { icon: 'alert' });
        dKeyword = n.keyword;
        return;
      }
      if (word !== n.keyword) patch.keyword = word;
    }
    if (!Object.keys(patch).length) return;
    try {
      await bookmarks.update(n.id, patch);
    } catch (e) {
      ui.toast(String(e), { icon: 'alert' });
      dName = n.title;
      dUrl = n.url;
      dTags = formatTags(n.tags);
      dKeyword = n.keyword;
    }
  }

  const enterBlurs = (e: KeyboardEvent) => {
    if (e.key === 'Enter') (e.currentTarget as HTMLElement).blur();
  };

  // ---------------------------------------------------------------- the tree on the left
  interface TreeRow {
    key: string;
    depth: number;
    label: string;
    icon: 'history' | 'download' | 'tag' | 'bookmarkStar' | 'bookmarkToolbar' | 'bookmarkMenu' | 'folder' | 'folderOpen';
    scope: Scope | null;
    /** Has something to open (shows the arrow). */
    twisty: boolean;
    open: boolean;
    /** A folder you can drop bookmarks into. */
    into?: number;
  }

  const tree = $derived.by(() => {
    const out: TreeRow[] = [];
    out.push({ key: 'history', depth: 0, label: 'History', icon: 'history', scope: { kind: 'history' }, twisty: false, open: false });
    out.push({ key: 'downloads', depth: 0, label: 'Downloads', icon: 'download', scope: { kind: 'downloads' }, twisty: false, open: false });
    const tags = bookmarks.tags;
    const tagsOpen = treeOpen.has('tags');
    out.push({ key: 'tags', depth: 0, label: 'Tags', icon: 'tag', scope: null, twisty: tags.length > 0, open: tagsOpen });
    if (tagsOpen) {
      for (const t of tags) {
        out.push({ key: `t:${t.tag.toLowerCase()}`, depth: 1, label: `${t.tag}  ·  ${t.count}`, icon: 'tag', scope: { kind: 'tag', tag: t.tag }, twisty: false, open: false });
      }
    }
    const allOpen = treeOpen.has('all');
    out.push({ key: 'all', depth: 0, label: 'All Bookmarks', icon: 'bookmarkStar', scope: { kind: 'all' }, twisty: true, open: allOpen });
    if (allOpen) {
      const walk = (id: number, depth: number) => {
        const n = ix.byId.get(id);
        if (!n || n.kind !== 'folder') return;
        const sub = kids(ix, id).filter((c) => c.kind === 'folder');
        const isOpen = treeOpen.has(`f${id}`);
        out.push({
          key: `f${id}`,
          depth,
          label: n.title || ROOT_TITLES[id] || 'Untitled folder',
          icon: n.parent === null ? rootIcon(id) : isOpen ? 'folderOpen' : 'folder',
          scope: { kind: 'folder', id },
          twisty: sub.length > 0,
          open: isOpen,
          into: id,
        });
        if (isOpen) for (const c of sub) walk(c.id, depth + 1);
      };
      for (const r of ROOTS) walk(r, 1);
    }
    return out;
  });

  function toggleTree(key: string) {
    const t = new Set(treeOpen);
    if (t.has(key)) t.delete(key);
    else t.add(key);
    treeOpen = t;
  }

  /** Whether a tree row is the place being shown. */
  function isCurrent(r: TreeRow): boolean {
    const s = scope;
    const t = r.scope;
    if (!t || s.kind !== t.kind) return false;
    if (s.kind === 'folder' && t.kind === 'folder') return s.id === t.id;
    if (s.kind === 'tag' && t.kind === 'tag') return s.tag === t.tag;
    return true;
  }

  function pickTree(r: TreeRow) {
    if (r.scope) go(r.scope);
    else toggleTree(r.key);
  }

  // ---------------------------------------------------------------- the toolbar menus
  let backups = $state<BookmarkBackup[]>([]);
  async function loadBackups() {
    backups = await bookmarksApi.bookmarkBackups().catch(() => []);
  }
  onMount(() => {
    void bookmarks.load(true);
    void loadBackups();
  });

  const newParent = () => (scope.kind === 'folder' ? scope.id : bookmarks.defaultParent());

  function importMenu(): MenuItem[] {
    return [
      { label: 'Import from another browser…', icon: 'importBox', run: () => (ui.bookmarkImport = true) },
      { label: 'Import from an HTML file…', icon: 'arrowDownToLine', run: () => void importBookmarksFile() },
      { label: 'Export to an HTML file…', icon: 'upload', run: () => void exportBookmarksFile() },
      { separator: true },
      { label: 'Back up…', icon: 'save', run: () => void backupBookmarksFile().then(loadBackups) },
      { label: 'Restore', icon: 'history', children: () => restoreItems() },
    ];
  }

  function restoreItems(): MenuItem[] {
    const items: MenuItem[] = backups.slice(0, 8).map((b) => ({
      label: `${date(b.created)} (${b.count})`,
      run: () => void restoreBookmarks(b.name),
    }));
    if (items.length) items.push({ separator: true });
    else items.push({ label: 'No automatic backups yet', heading: true }, { separator: true });
    items.push({ label: 'Choose a file…', icon: 'folder', run: () => void restoreBookmarks() });
    return items;
  }

  function organizeMenu(): MenuItem[] {
    const parent = newParent();
    const s = scope;
    return [
      { label: bookmarks.undoLabel ? `Undo ${bookmarks.undoLabel}` : 'Undo', icon: 'undo', hint: 'Ctrl+Z', disabled: !bookmarks.canUndo, run: () => void bookmarks.undo() },
      { label: bookmarks.redoLabel ? `Redo ${bookmarks.redoLabel}` : 'Redo', icon: 'redo', hint: 'Ctrl+Y', disabled: !bookmarks.canRedo, run: () => void bookmarks.redo() },
      { separator: true },
      { label: 'New bookmark…', icon: 'bookmark', run: () => (ui.bookmarkDialog = { kind: 'bookmark', url: 'https://', title: '', parent }) },
      { label: 'New folder…', icon: 'folder', run: () => newFolderDialog(parent) },
      { label: 'New separator', icon: 'minimize', run: () => void bookmarks.addSeparator(parent) },
      { separator: true },
      {
        label: 'Edit…',
        icon: 'edit',
        disabled: picked.length !== 1 || picked[0].kind === 'separator' || picked[0].parent === null,
        run: () => picked[0] && editNode(picked[0]),
      },
      { label: 'Delete', icon: 'trash', disabled: !picked.some((n) => n.parent !== null), run: deleteSelected },
      { label: 'Sort by name', icon: 'sort', disabled: s.kind !== 'folder', run: () => s.kind === 'folder' && void bookmarks.sortByName(s.id) },
      { separator: true },
      { label: 'Import and backup', icon: 'save', children: () => importMenu() },
    ];
  }

  function viewsMenu(): MenuItem[] {
    return [
      { label: 'Columns', heading: true },
      { label: 'Name', checked: true, disabled: true },
      ...COLS.map(
        (c): MenuItem => ({
          label: c.label,
          checked: cols.includes(c.id),
          run: () => (cols = cols.includes(c.id) ? cols.filter((x) => x !== c.id) : COLS.map((k) => k.id).filter((k) => k === c.id || cols.includes(k))),
        }),
      ),
    ];
  }

  function sortMenu(): MenuItem[] {
    const by = (label: string, value: SortBy): MenuItem => ({ label, checked: sortBy === value, run: () => setSort(value) });
    return [
      by('Unsorted', 'none'),
      { separator: true },
      by('Sort by name', 'name'),
      by('Sort by tags', 'tags'),
      by('Sort by location', 'location'),
      by('Sort by keyword', 'keyword'),
      by('Sort by date added', 'added'),
      by('Sort by last modified', 'modified'),
      { separator: true },
      { label: 'Ascending', checked: sortDir === 1, disabled: sortBy === 'none', run: () => (sortDir = 1) },
      { label: 'Descending', checked: sortDir === -1, disabled: sortBy === 'none', run: () => (sortDir = -1) },
    ];
  }

  function drop(e: MouseEvent, items: () => MenuItem[]) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    ui.openMenu(r.left, r.bottom + 4, items());
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

<div class="lib">
  <header class="bar">
    <div class="nav">
      <button class="icon-btn" title="Back" aria-label="Back" disabled={navAt === 0} onclick={back}><Icon name="chevronLeft" size={16} /></button>
      <button class="icon-btn" title="Forward" aria-label="Forward" disabled={navAt >= nav.length - 1} onclick={forward}><Icon name="chevronRight" size={16} /></button>
    </div>
    <button class="drop" title="Organize" aria-haspopup="menu" onclick={(e) => drop(e, organizeMenu)}>
      <Icon name="gear" size={17} /><Icon name="chevronDown" size={12} />
    </button>
    <button class="drop" title="Views" aria-haspopup="menu" onclick={(e) => drop(e, viewsMenu)}>
      <Icon name="listView" size={17} /><Icon name="chevronDown" size={12} />
    </button>
    <button class="drop" title="Sort" aria-haspopup="menu" onclick={(e) => drop(e, sortMenu)}>
      <Icon name="sort" size={17} /><Icon name="chevronDown" size={12} />
    </button>
    <span class="grow"></span>
    {#if scope.kind !== 'history' && scope.kind !== 'downloads'}
      <label class="search">
        <Icon name="search" size={15} />
        <input
          bind:value={query}
          placeholder={scope.kind === 'folder' ? 'Search in this folder' : 'Search Bookmarks'}
          spellcheck="false"
          autocomplete="off"
          onkeydown={(e) => {
            if (e.key === 'Escape' && query) {
              e.stopPropagation();
              query = '';
            }
          }}
        />
      </label>
    {/if}
  </header>

  <div class="main">
    <nav class="left" aria-label="Library">
      {#each tree as r (r.key)}
        {@const here = isCurrent(r)}
        <div
          class="node"
          class:sel={here}
          style:padding-left="{6 + r.depth * 16}px"
          data-bm-into={r.into}
          role="treeitem"
          tabindex="-1"
          aria-selected={here}
          aria-expanded={r.twisty ? r.open : undefined}
          onkeydown={() => {}}
          ondblclick={() => r.twisty && toggleTree(r.key)}
        >
          <button
            class="tw"
            class:hidden={!r.twisty}
            tabindex="-1"
            aria-label={r.open ? 'Collapse' : 'Expand'}
            onclick={() => toggleTree(r.key)}
          >
            <Icon name={r.open ? 'chevronDown' : 'chevronRight'} size={13} />
          </button>
          <button class="pick" onclick={() => pickTree(r)}>
            <Icon name={r.icon} size={18} />
            <span class="ellipsis">{r.label}</span>
          </button>
        </div>
      {/each}
    </nav>

    <section class="right">
      {#if scope.kind === 'history'}
        <div class="embed"><HistoryPage /></div>
      {:else if scope.kind === 'downloads'}
        <div class="embed"><DownloadsPage /></div>
      {:else}
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

        <div class="details" class:idle={!one || one.kind === 'separator'}>
          {#if one && one.kind !== 'separator'}
            <div class="drow">
              <label for="lib-name">Name</label>
              <input id="lib-name" class="field" bind:value={dName} spellcheck="false" onchange={() => void commit('title')} onkeydown={enterBlurs} disabled={one.parent === null} />
            </div>
            {#if one.kind === 'bookmark'}
              <div class="drow">
                <label for="lib-url">URL</label>
                <input id="lib-url" class="field" bind:value={dUrl} spellcheck="false" onchange={() => void commit('url')} onkeydown={enterBlurs} />
              </div>
              <div class="drow top">
                <label for="lib-tags">Tags</label>
                <div class="stack">
                  <input id="lib-tags" class="field" bind:value={dTags} placeholder="Separate tags with commas" spellcheck="false" onchange={() => void commit('tags')} onkeydown={enterBlurs} />
                  <small>Use tags to organize and search for bookmarks from the address bar</small>
                </div>
              </div>
              <div class="drow top">
                <label for="lib-kw">Keyword</label>
                <div class="stack">
                  <input id="lib-kw" class="field" bind:value={dKeyword} spellcheck="false" onchange={() => void commit('keyword')} onkeydown={enterBlurs} />
                  <small>Use a single keyword to open bookmarks directly from the address bar</small>
                </div>
              </div>
            {:else}
              <p class="count">
                {kids(ix, one.id).length} {kids(ix, one.id).length === 1 ? 'item' : 'items'}
                {#if kids(ix, one.id).some((k) => k.kind === 'bookmark')}
                  <button class="link" onclick={() => openAll(one)}>Open all in tabs</button>
                {/if}
              </p>
            {/if}
          {:else if picked.length > 1}
            <p class="count">{picked.length} items selected</p>
          {:else}
            <p class="count">Select a bookmark to see and change its details.</p>
          {/if}
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .lib {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    user-select: none;
  }
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 46px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
  }
  .nav {
    display: flex;
    margin-right: 4px;
    border-radius: var(--r-btn);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .nav .icon-btn {
    width: 30px;
    height: 28px;
  }
  .drop {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 30px;
    padding: 0 8px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-2);
  }
  .drop:hover {
    background: var(--field-focus);
    color: var(--text);
  }
  .grow {
    flex: 1;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    width: min(320px, 40%);
    height: 30px;
    padding: 0 10px;
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
  .main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 240px minmax(0, 1fr);
  }
  .left {
    min-height: 0;
    overflow-y: auto;
    padding: 6px 4px;
    border-right: 1px solid var(--line);
    background: color-mix(in srgb, var(--text) 3%, transparent);
  }
  .node {
    position: relative;
    display: flex;
    align-items: center;
    min-height: 30px;
    border-radius: var(--r-item);
    corner-shape: var(--sq);
  }
  .node:hover {
    background: var(--hover);
  }
  .node.sel {
    background: color-mix(in srgb, var(--accent) 26%, transparent);
  }
  .tw {
    width: 18px;
    height: 26px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .tw.hidden {
    visibility: hidden;
  }
  .pick {
    flex: 1;
    min-width: 0;
    height: 30px;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    font-size: 13px;
  }
  .right {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .embed {
    flex: 1;
    min-height: 0;
    user-select: text;
  }
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
  .details {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px 16px;
    border-top: 1px solid var(--line-strong);
    background: color-mix(in srgb, var(--accent) 5%, var(--surface));
    user-select: text;
  }
  .details.idle {
    padding: 14px 16px;
  }
  .drow {
    display: grid;
    grid-template-columns: 70px minmax(0, 1fr);
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }
  .drow.top {
    align-items: start;
  }
  .drow.top label {
    padding-top: 7px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  small {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .count {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .link {
    color: var(--accent-fg);
    text-decoration: underline;
  }
</style>
