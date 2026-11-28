<script lang="ts">
  import { slide } from 'svelte/transition';
  import { tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import TabItem from './TabItem.svelte';
  import FolderItem from './FolderItem.svelte';
  import { browser, tabTitle } from '$features/browser/browser.svelte';
  import { MAX_FOLDER_DEPTH, childFolders, folderDepth } from '$features/browser/folders';
  import { dnd } from './sidebar-dnd.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { type MenuItem } from '$shared/ui/menu';
  import type { Folder, Space } from '$features/browser/types';

  // Zen tab folder (src/zen/folders): up to 5 levels deep, optional custom icon, inline search,
  // and keeps the active descendant tab visible even while collapsed.
  let {
    folder,
    space,
    collapsed = false,
    depth = 0,
    filter = '',
  }: { folder: Folder; space: Space; collapsed?: boolean; depth?: number; filter?: string } = $props();

  const FOLDER_EMOJIS = ['📁', '⭐', '💼', '📚', '🎨', '🧪', '🎮', '🎵', '🔥', '🌿', '💡', '🛒'];

  const subFolders = $derived(childFolders(space.folders ?? [], folder.id));
  const members = $derived(browser.folderMembers(space, folder).map((id) => browser.tabs[id]).filter(Boolean));
  const allDescIds = $derived(browser.folderDescendantTabs(space, folder));
  const activeId = $derived(browser.activeTab?.id);
  const hasActive = $derived(!!activeId && allDescIds.includes(activeId));

  let searching = $state(false);
  let query = $state('');
  let searchEl: HTMLInputElement | undefined = $state();
  const activeQuery = $derived((filter || (searching ? query : '')).trim().toLowerCase());
  const matchTab = (t: (typeof members)[number]) =>
    !activeQuery || tabTitle(t).toLowerCase().includes(activeQuery) || t.url.toLowerCase().includes(activeQuery);

  const visibleTabs = $derived(
    activeQuery
      ? members.filter(matchTab)
      : folder.collapsed
        ? members.filter((t) => t.id === activeId)
        : members,
  );
  const visibleSubs = $derived(
    activeQuery
      ? subFolders.filter((sub) =>
          sub.name.toLowerCase().includes(activeQuery) ||
          browser.folderDescendantTabs(space, sub).some((id) => {
            const t = browser.tabs[id];
            return t && matchTab(t);
          }),
        )
      : folder.collapsed
        ? subFolders.filter((sub) => !!activeId && browser.folderDescendantTabs(space, sub).includes(activeId))
        : subFolders,
  );

  const renaming = $derived(browser.renamingFolderId === folder.id && !collapsed);
  let input: HTMLInputElement | undefined = $state();
  const fMark = $derived(dnd.folderMark(folder.id));
  const dragging = $derived(dnd.drag?.started && dnd.drag.kind === 'folder' && dnd.drag.id === folder.id);
  const canNestHere = $derived(folderDepth(space.folders ?? [], folder.id) + 1 < MAX_FOLDER_DEPTH);

  $effect(() => {
    if (renaming) void tick().then(() => (input?.focus(), input?.select()));
  });

  function openSearch(e?: MouseEvent) {
    e?.stopPropagation();
    if (folder.collapsed) browser.toggleFolder(folder.id, false);
    searching = true;
    void tick().then(() => searchEl?.focus());
  }

  function closeSearch() {
    searching = false;
    query = '';
  }

  function finish(save: boolean) {
    if (!renaming) return;
    if (save && input) browser.renameFolder(folder.id, input.value);
    browser.renamingFolderId = null;
    ui.renaming = false;
  }

  function menu(e: MouseEvent) {
    e.preventDefault();
    const items: MenuItem[] = [
      { label: 'Rename folder', icon: 'edit', disabled: collapsed, run: () => (browser.renamingFolderId = folder.id) },
      {
        label: folder.collapsed ? 'Expand folder' : 'Collapse folder',
        icon: folder.collapsed ? 'chevronDown' : 'chevronUp',
        run: () => browser.toggleFolder(folder.id),
      },
      { label: 'Search in folder', icon: 'search', disabled: collapsed || allDescIds.length < 1, run: () => openSearch() },
      {
        label: 'New subfolder',
        icon: 'folder',
        disabled: !canNestHere,
        run: () => browser.createFolder([], space.id, folder.id),
      },
      {
        label: 'New tab in folder',
        icon: 'plus',
        run: () => {
          const t = browser.openTab(undefined, { pinned: true, spaceId: space.id });
          browser.moveToFolder(t.id, folder.id);
        },
      },
      {
        label: 'Folder icon',
        icon: 'folder',
        children: [
          { label: 'Default folder icon', checked: !folder.icon, run: () => browser.setFolderIcon(folder.id, undefined) },
          { separator: true },
          ...FOLDER_EMOJIS.map((em) => ({
            label: em,
            checked: folder.icon === em,
            run: () => browser.setFolderIcon(folder.id, em),
          })),
        ],
      },
      { label: 'Unload all tabs', icon: 'moonSleep', run: () => allDescIds.forEach((id) => void browser.sleep(id)) },
      { separator: true },
      { label: 'Unpack folder', icon: 'folder', run: () => browser.deleteFolder(folder.id) },
      { label: 'Delete folder and its tabs', icon: 'trash', danger: true, run: () => browser.deleteFolder(folder.id, true) },
    ];
    ui.openMenu(e.clientX, e.clientY, items);
  }
</script>

<div class="folder" class:collapsed={folder.collapsed} class:icon-only={collapsed} class:dragging data-folder-id={folder.id}>
  <div
    class="head"
    class:selected={hasActive && folder.collapsed}
    class:dropping={fMark === 'inside'}
    role="button"
    tabindex="-1"
    aria-expanded={!folder.collapsed}
    title={collapsed ? folder.name : undefined}
    data-folder-head={folder.id}
    onclick={() => !renaming && browser.toggleFolder(folder.id)}
    onkeydown={(e) => e.key === 'Enter' && browser.toggleFolder(folder.id)}
    oncontextmenu={menu}
    onpointerdown={(e) => dnd.startFolder(e, folder.id)}
  >
    <span class="bg"></span>
    {#if folder.icon}
      <span class="custom-icon" data-sidebar-icon="folder:{folder.id}">{folder.icon}</span>
    {:else}
      <svg class="folder-icon" data-sidebar-icon="folder:{folder.id}" class:open={!folder.collapsed} width="28" height="28" viewBox="0 0 28 28" aria-hidden="true">
        <path class="back" d="M5 8.6a2.2 2.2 0 0 1 2.2-2.2h4.1c.5 0 1 .2 1.3.6l1.2 1.4h7.3a2.2 2.2 0 0 1 2.2 2.2v9.3a2.2 2.2 0 0 1-2.2 2.2H7.2A2.2 2.2 0 0 1 5 19.9z" />
        <path class="front" d="M5 12.1c0-1 .8-1.8 1.8-1.8h14.4c1 0 1.8.8 1.8 1.8v7.8a2.2 2.2 0 0 1-2.2 2.2H7.2A2.2 2.2 0 0 1 5 19.9z" />
        {#if hasActive && folder.collapsed}
          <g class="dots"><circle cx="10.5" cy="16.4" r="1" /><circle cx="14" cy="16.4" r="1" /><circle cx="17.5" cy="16.4" r="1" /></g>
        {/if}
      </svg>
    {/if}
    {#if !collapsed}
      {#if renaming}
        <input
          class="name rename"
          bind:this={input}
          value={folder.name}
          spellcheck="false"
          onclick={(e) => e.stopPropagation()}
          onfocus={() => (ui.renaming = true)}
          onkeydown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') finish(true);
            else if (e.key === 'Escape') finish(false);
          }}
          onblur={() => finish(true)}
        />
      {:else}
        <span
          class="name ellipsis"
          role="presentation"
          ondblclick={(e) => {
            e.stopPropagation();
            browser.renamingFolderId = folder.id;
          }}>{folder.name}</span
        >
        {#if allDescIds.length >= 2 && !filter}
          <button
            type="button"
            class="search-btn"
            class:on={searching}
            title="Search in folder"
            aria-label="Search in folder"
            onclick={(e) => (searching ? (e.stopPropagation(), closeSearch()) : openSearch(e))}
          >
            <Icon name="search" size={12} />
          </button>
        {/if}
      {/if}
    {/if}
    {#if fMark === 'before' || fMark === 'after'}
      <span class="drop-mark" class:after={fMark === 'after'}></span>
    {/if}
  </div>

  {#if searching && !collapsed}
    <div class="search-row" transition:slide={{ duration: 120 }}>
      <input
        class="search-input"
        bind:this={searchEl}
        bind:value={query}
        placeholder="Filter folder…"
        spellcheck="false"
        onkeydown={(e) => {
          e.stopPropagation();
          if (e.key === 'Escape') closeSearch();
        }}
      />
      {#if query}
        <button type="button" class="clear-q" aria-label="Clear filter" onclick={closeSearch}><Icon name="close" size={11} /></button>
      {/if}
    </div>
  {/if}

  {#if visibleSubs.length || visibleTabs.length}
    <div class="children" transition:slide={{ duration: 150 }}>
      {#each visibleSubs as sub (sub.id)}
        <FolderItem folder={sub} {space} {collapsed} depth={depth + 1} filter={activeQuery} />
      {/each}
      {#each visibleTabs as tab (tab.id)}
        <TabItem
          {tab}
          {collapsed}
          active={activeId === tab.id}
          drop={dnd.mark(tab.id)}
          dragging={dnd.drag?.started && dnd.drag.kind === 'tab' && dnd.drag.id === tab.id}
          onpointerdown={(e) => dnd.start(e, tab.id)}
        />
      {/each}
    </div>
  {/if}
</div>

<style>
  .folder {
    --folder-behind: color-mix(in srgb, var(--accent) 60%, #c1c1c1);
    --folder-front: color-mix(in srgb, var(--accent), black 40%);
    --folder-stroke: color-mix(in srgb, var(--accent) 15%, #ebebeb);
    display: flex;
    flex-direction: column;
  }
  .folder.dragging {
    opacity: 0.45;
  }
  :global(:root[data-theme='light']) .folder {
    --folder-behind: color-mix(in srgb, var(--accent) 60%, gray);
    --folder-front: color-mix(in srgb, var(--accent), white 70%);
    --folder-stroke: color-mix(in srgb, var(--accent) 50%, black);
  }
  .head {
    position: relative;
    display: flex;
    align-items: center;
    height: calc(var(--tab-h) + var(--tab-mb) * 2);
    padding: var(--tab-mb);
    font-size: var(--tab-font);
    font-weight: 600;
  }
  .bg {
    position: absolute;
    inset: var(--tab-mb);
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    pointer-events: none;
    transition: background-color 0.1s ease;
  }
  .head:hover .bg,
  .head.selected .bg {
    background: var(--hover);
  }
  .head.dropping .bg {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    box-shadow: inset 0 0 0 1.5px var(--drag-indicator);
  }
  .custom-icon {
    width: 28px;
    height: 28px;
    flex: none;
    display: grid;
    place-items: center;
    margin-inline: 5px 4px;
    font-size: 15px;
    line-height: 1;
  }
  .folder-icon {
    flex: none;
    margin-inline: 5px 4px;
    filter: contrast(125%);
    overflow: visible;
  }
  .back {
    fill: var(--folder-behind);
  }
  .front {
    fill: var(--folder-front);
    stroke: var(--folder-stroke);
    stroke-width: 1.1;
    stroke-linejoin: round;
    transform-origin: 14px 22px;
    transition: transform 0.15s ease;
  }
  .folder-icon.open .front {
    transform: skewX(-14deg) scaleY(0.86);
  }
  .dots circle {
    fill: var(--folder-stroke);
  }
  .name {
    flex: 1;
    min-width: 0;
    padding-inline-end: 6px;
  }
  .rename {
    margin: 0;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    font: inherit;
    color: inherit;
  }
  .search-btn {
    position: relative;
    flex: none;
    display: none;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-inline-end: 4px;
    border-radius: var(--r-btn);
    opacity: 0.7;
  }
  .head:hover .search-btn,
  .search-btn.on {
    display: grid;
  }
  .search-btn:hover,
  .search-btn.on {
    opacity: 1;
    background: color-mix(in srgb, currentColor 10%, transparent);
  }
  .search-row {
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 2px 6px 4px 16px;
    padding: 0 6px;
    height: 26px;
    border-radius: 7px;
    background: var(--element-bg);
  }
  .search-input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    font-size: 12px;
    color: var(--text);
  }
  .clear-q {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    opacity: 0.7;
  }
  .clear-q:hover {
    opacity: 1;
    background: var(--hover);
  }
  .children {
    display: flex;
    flex-direction: column;
    margin-inline-start: 14px;
    transition: var(--zen-indent);
  }
  :global(.app.animating) .children {
    transition: none;
  }
  .folder.icon-only .head {
    width: calc(var(--collapsed-tab-w) + var(--tab-mb) * 2);
    margin-inline: auto;
    justify-content: center;
  }
  .folder.icon-only .folder-icon,
  .folder.icon-only .custom-icon {
    margin: 0;
  }
  .folder.icon-only .children {
    margin-inline-start: 0;
  }
  .drop-mark {
    position: absolute;
    left: 14px;
    right: 6px;
    top: -1px;
    height: 2px;
    border-radius: 5px;
    background: var(--drag-indicator);
    pointer-events: none;
    z-index: 2;
  }
  .drop-mark.after {
    top: auto;
    bottom: -1px;
  }
</style>
