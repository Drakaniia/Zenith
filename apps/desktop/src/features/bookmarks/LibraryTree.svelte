<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { ROOTS, ROOT_TITLES, kids } from './bookmarks';
  import { rootIcon } from './menus';
  import type { Scope } from './library';

  // The tree on the left of the Library: History, Downloads, tags and all bookmark folders.

  let {
    scope,
    treeOpen = $bindable(),
    go,
  }: {
    scope: Scope;
    /** Keys of the open tree rows ('tags', 'all', `f<folder id>`). */
    treeOpen: Set<string>;
    go: (next: Scope) => void;
  } = $props();

  const ix = $derived(bookmarks.index);

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
</script>

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

<style>
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
</style>
