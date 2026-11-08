<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import HistoryPage from '$features/history/HistoryPage.svelte';
  import DownloadsPage from '$features/downloads/DownloadsPage.svelte';
  import LibraryToolbar from './LibraryToolbar.svelte';
  import LibraryTree from './LibraryTree.svelte';
  import LibraryTable from './LibraryTable.svelte';
  import LibraryDetails from './LibraryDetails.svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { type MenuItem } from '$shared/ui/menu';
  import { bookmarksApi } from './api';
  import { ROOTS, ancestors } from './bookmarks';
  import {
    backupBookmarksFile,
    editNode,
    exportBookmarksFile,
    importBookmarksFile,
    newFolderDialog,
    restoreBookmarks,
  } from './bookmark-actions';
  import { COLS, date, type Col, type Scope, type SortBy } from './library';
  import type { BookmarkBackup, BookmarkNode } from './types';

  // The Bookmarks Library (Ctrl+Shift+O), laid out like Zen's: a tree of History, Downloads, tags
  // and all bookmarks on the left, the contents of the chosen folder as a table (Name, Tags,
  // Location) on the right, and the chosen bookmark's Name, URL, Tags and Keyword below it.
  // The parts are LibraryToolbar, LibraryTree, LibraryTable and LibraryDetails; where we are,
  // the selection and the view settings live here.

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
  let table: LibraryTable | undefined = $state();

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

  // ---------------------------------------------------------------- the selection
  const picked = $derived(selected.map((id) => ix.byId.get(id)).filter((n): n is BookmarkNode => !!n));
  const one = $derived(picked.length === 1 ? picked[0] : undefined);

  function setSort(by: SortBy) {
    if (sortBy === by && by !== 'none') sortDir = sortDir === 1 ? -1 : 1;
    else {
      sortBy = by;
      sortDir = by === 'added' || by === 'modified' ? -1 : 1;
    }
  }

  function deleteSelected() {
    table?.deleteSelected();
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
</script>

<div class="lib">
  <LibraryToolbar
    {scope}
    bind:query
    canBack={navAt !== 0}
    canForward={navAt < nav.length - 1}
    {back}
    {forward}
    {organizeMenu}
    {viewsMenu}
    {sortMenu}
  />

  <div class="main">
    <LibraryTree {scope} bind:treeOpen {go} />

    <section class="right">
      {#if scope.kind === 'history'}
        <div class="embed"><HistoryPage /></div>
      {:else if scope.kind === 'downloads'}
        <div class="embed"><DownloadsPage /></div>
      {:else}
        <LibraryTable
          bind:this={table}
          {scope}
          {query}
          {cols}
          {sortBy}
          {sortDir}
          {setSort}
          bind:selected
          bind:anchor
          {picked}
          {one}
          {go}
          {newParent}
          {organizeMenu}
        />

        <LibraryDetails {one} {picked} />
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
  .main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 240px minmax(0, 1fr);
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
</style>
