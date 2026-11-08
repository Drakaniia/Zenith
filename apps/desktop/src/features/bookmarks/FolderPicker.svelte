<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { ROOTS, ROOT_TITLES, ancestors, isInside, kids, recentFolders } from './bookmarks';
  import { rootIcon } from '../../app/menus';
  import { ui } from '$shared/ui/ui.svelte';

  // Where a bookmark goes (Zen's "Location"): a menu of the built-in folders and the folders used
  // lately, and an expander that shows the whole folder tree with a "New folder" button.
  let {
    value = $bindable(),
    expanded = $bindable(false),
    exclude,
    onchange,
  }: { value: number; expanded?: boolean; exclude?: number; onchange?: (id: number) => void } = $props();

  let root: HTMLDivElement | undefined = $state();
  let listOpen = $state(false);
  let adding = $state(false);
  let newName = $state('');
  let nameInput: HTMLInputElement | undefined = $state();
  let open = $state(new Set<number>());

  const ix = $derived(bookmarks.index);
  const current = $derived(ix.byId.get(value));
  const title = $derived(current?.title || ROOT_TITLES[value] || 'Folder');
  const recents = $derived(
    recentFolders(ix, 5)
      .filter((f) => f.id !== value && f.id !== exclude && (exclude === undefined || !isInside(ix, f.id, exclude)))
      .slice(0, 4),
  );

  // The tree starts open down to the chosen folder.
  $effect(() => {
    const next = new Set(open);
    for (const r of ROOTS) next.add(r);
    for (const a of ancestors(ix, value)) next.add(a.id);
    if (next.size !== open.size) open = next;
  });

  interface Row {
    id: number;
    title: string;
    depth: number;
    folders: boolean;
    open: boolean;
  }
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const walk = (id: number, depth: number) => {
      const n = ix.byId.get(id);
      if (!n || n.kind !== 'folder' || id === exclude) return;
      const sub = kids(ix, id).filter((c) => c.kind === 'folder' && c.id !== exclude);
      const isOpen = open.has(id);
      out.push({ id, title: n.title || ROOT_TITLES[id] || 'Untitled folder', depth, folders: sub.length > 0, open: isOpen });
      if (isOpen) for (const c of sub) walk(c.id, depth + 1);
    };
    for (const r of ROOTS) walk(r, 0);
    return out;
  });

  function pick(id: number) {
    value = id;
    listOpen = false;
    onchange?.(id);
  }

  function toggle(id: number) {
    const next = new Set(open);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    open = next;
  }

  async function startAdding() {
    adding = true;
    newName = '';
    await tick();
    nameInput?.focus();
  }

  async function createFolder() {
    const name = newName.trim();
    adding = false;
    if (!name) return;
    try {
      const id = await bookmarks.addFolder(value, name);
      open = new Set(open).add(value);
      pick(id);
    } catch (e) {
      ui.toast(`Couldn't make the folder: ${e}`, { icon: 'alert' });
    }
  }

  // Closes the menu on a press anywhere else.
  $effect(() => {
    if (!listOpen) return;
    const away = (e: PointerEvent) => {
      if (!root?.contains(e.target as Node)) listOpen = false;
    };
    window.addEventListener('pointerdown', away, true);
    return () => window.removeEventListener('pointerdown', away, true);
  });
</script>

<div class="picker" bind:this={root}>
  <div class="row">
    <button
      type="button"
      class="select"
      aria-haspopup="listbox"
      aria-expanded={listOpen}
      onclick={() => (listOpen = !listOpen)}
    >
      <Icon name={rootIcon(value)} size={16} />
      <span class="name ellipsis">{title}</span>
      <Icon name="chevronDown" size={13} />
    </button>
    <button
      type="button"
      class="expander"
      class:on={expanded}
      title={expanded ? 'Hide the folder list' : 'Show all folders'}
      aria-label={expanded ? 'Hide the folder list' : 'Show all folders'}
      aria-expanded={expanded}
      onclick={() => (expanded = !expanded)}
    >
      <Icon name={expanded ? 'chevronUp' : 'chevronDown'} size={15} />
    </button>
  </div>

  {#if listOpen}
    <div class="menu glass" role="listbox">
      {#each ROOTS as r (r)}
        {#if r !== exclude}
          <button type="button" role="option" aria-selected={value === r} class="opt" onclick={() => pick(r)}>
            <span class="mark">{#if value === r}<Icon name="check" size={14} />{/if}</span>
            <Icon name={rootIcon(r)} size={16} />
            <span class="ellipsis">{bookmarks.node(r)?.title || ROOT_TITLES[r]}</span>
          </button>
        {/if}
      {/each}
      {#if recents.length}
        <div class="sep"></div>
        {#each recents as f (f.id)}
          <button type="button" role="option" aria-selected={value === f.id} class="opt" onclick={() => pick(f.id)}>
            <span class="mark"></span>
            <Icon name="folder" size={16} />
            <span class="ellipsis">{f.title || 'Untitled folder'}</span>
          </button>
        {/each}
      {/if}
      <div class="sep"></div>
      <button
        type="button"
        class="opt"
        onclick={() => {
          listOpen = false;
          expanded = true;
        }}
      >
        <span class="mark"></span>
        <Icon name="folderOpen" size={16} />
        <span>Choose…</span>
      </button>
    </div>
  {/if}

  {#if expanded}
    <div class="tree" role="tree" aria-label="Folders">
      {#each rows as r (r.id)}
        <div
          class="node"
          class:sel={r.id === value}
          role="treeitem"
          aria-selected={r.id === value}
          aria-expanded={r.folders ? r.open : undefined}
          tabindex="-1"
          style:padding-left="{4 + r.depth * 16}px"
        >
          <button
            type="button"
            class="twisty"
            class:hidden={!r.folders}
            tabindex="-1"
            aria-label={r.open ? 'Collapse' : 'Expand'}
            onclick={() => toggle(r.id)}
          >
            <Icon name={r.open ? 'chevronDown' : 'chevronRight'} size={12} />
          </button>
          <button type="button" class="pickrow" onclick={() => pick(r.id)} ondblclick={() => r.folders && toggle(r.id)}>
            <Icon name={r.depth === 0 ? rootIcon(r.id) : r.open ? 'folderOpen' : 'folder'} size={16} />
            <span class="ellipsis">{r.title}</span>
          </button>
        </div>
      {/each}
    </div>
    <div class="make">
      {#if adding}
        <input
          class="field"
          bind:this={nameInput}
          bind:value={newName}
          placeholder="Folder name"
          spellcheck="false"
          onkeydown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') {
              e.preventDefault();
              void createFolder();
            } else if (e.key === 'Escape') adding = false;
          }}
          onblur={() => void createFolder()}
        />
      {:else}
        <button type="button" class="btn ghost small" onclick={startAdding}><Icon name="plus" size={14} />New folder</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
    min-width: 0;
    flex: 1;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .select,
  .expander {
    height: 32px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
    transition: background var(--fast);
  }
  .select {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    text-align: left;
    font-size: 13px;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  .expander {
    flex: none;
    width: 34px;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  .select:hover,
  .expander:hover,
  .expander.on {
    background: var(--field-focus);
  }
  .menu {
    position: absolute;
    z-index: 5;
    left: 0;
    right: 40px;
    top: 36px;
    padding: 4px;
    border-radius: var(--r-panel);
    corner-shape: var(--sq);
  }
  .opt {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 30px;
    padding: 0 8px;
    border-radius: var(--r-item);
    corner-shape: var(--sq);
    text-align: left;
    font-size: 13px;
  }
  .opt:hover {
    background: color-mix(in srgb, currentColor 9%, transparent);
  }
  .mark {
    width: 14px;
    display: grid;
    place-items: center;
    flex: none;
    color: var(--text-2);
  }
  .sep {
    height: 1px;
    margin: 4px 2px;
    background: color-mix(in srgb, currentColor 15%, transparent);
  }
  .tree {
    margin-top: 8px;
    max-height: 190px;
    overflow-y: auto;
    padding: 4px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .node {
    display: flex;
    align-items: center;
    min-height: 28px;
    border-radius: var(--r-item);
    corner-shape: var(--sq);
  }
  .node:hover {
    background: var(--hover);
  }
  .node.sel {
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .twisty {
    width: 18px;
    height: 24px;
    display: grid;
    place-items: center;
    flex: none;
    color: var(--text-3);
  }
  .twisty.hidden {
    visibility: hidden;
  }
  .pickrow {
    flex: 1;
    min-width: 0;
    height: 28px;
    display: flex;
    align-items: center;
    gap: 7px;
    text-align: left;
    font-size: 13px;
  }
  .make {
    margin-top: 6px;
  }
  .make .field {
    width: 100%;
    height: 30px;
  }
  .btn.small {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
</style>
