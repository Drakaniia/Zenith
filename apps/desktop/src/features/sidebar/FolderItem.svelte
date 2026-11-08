<script lang="ts">
  import { slide } from 'svelte/transition';
  import { tick } from 'svelte';
  import TabItem from './TabItem.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { dnd } from './sidebar-dnd.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { type MenuItem } from '$shared/ui/menu';
  import type { Folder, Space } from '$features/browser/types';

  // A Zen tab folder (src/zen/folders): header with a two-layer folder icon that opens when the
  // folder is expanded, and its tabs indented below. A collapsed folder keeps its active tab.
  let { folder, space, collapsed = false }: { folder: Folder; space: Space; collapsed?: boolean } = $props();

  const members = $derived(browser.folderMembers(space, folder).map((id) => browser.tabs[id]));
  const activeId = $derived(browser.activeTab?.id);
  const hasActive = $derived(members.some((t) => t.id === activeId));
  const shown = $derived(folder.collapsed ? members.filter((t) => t.id === activeId) : members);
  const renaming = $derived(browser.renamingFolderId === folder.id && !collapsed);
  let input: HTMLInputElement | undefined = $state();
  let dropping = $derived(dnd.drag?.started && dnd.drag.target?.zone === 'folder' && dnd.drag.target.folderId === folder.id);

  $effect(() => {
    if (renaming) void tick().then(() => (input?.focus(), input?.select()));
  });

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
      {
        label: 'New tab in folder',
        icon: 'plus',
        run: () => {
          const t = browser.openTab(undefined, { pinned: true, spaceId: space.id });
          browser.moveToFolder(t.id, folder.id);
        },
      },
      { label: 'Unload all tabs', icon: 'moonSleep', run: () => members.forEach((t) => void browser.sleep(t.id)) },
      { separator: true },
      { label: 'Unpack folder', icon: 'folder', run: () => browser.deleteFolder(folder.id) },
      { label: 'Delete folder and its tabs', icon: 'trash', danger: true, run: () => browser.deleteFolder(folder.id, true) },
    ];
    ui.openMenu(e.clientX, e.clientY, items);
  }
</script>

<div class="folder" class:collapsed={folder.collapsed} class:icon-only={collapsed} data-folder-id={folder.id}>
  <div
    class="head"
    class:selected={hasActive && folder.collapsed}
    class:dropping
    role="button"
    tabindex="-1"
    aria-expanded={!folder.collapsed}
    title={collapsed ? folder.name : undefined}
    data-folder-head={folder.id}
    onclick={() => !renaming && browser.toggleFolder(folder.id)}
    onkeydown={(e) => e.key === 'Enter' && browser.toggleFolder(folder.id)}
    oncontextmenu={menu}
  >
    <span class="bg"></span>
    <svg class="folder-icon" data-sidebar-icon="folder:{folder.id}" class:open={!folder.collapsed} width="28" height="28" viewBox="0 0 28 28" aria-hidden="true">
      <path class="back" d="M5 8.6a2.2 2.2 0 0 1 2.2-2.2h4.1c.5 0 1 .2 1.3.6l1.2 1.4h7.3a2.2 2.2 0 0 1 2.2 2.2v9.3a2.2 2.2 0 0 1-2.2 2.2H7.2A2.2 2.2 0 0 1 5 19.9z" />
      <path class="front" d="M5 12.1c0-1 .8-1.8 1.8-1.8h14.4c1 0 1.8.8 1.8 1.8v7.8a2.2 2.2 0 0 1-2.2 2.2H7.2A2.2 2.2 0 0 1 5 19.9z" />
      {#if hasActive && folder.collapsed}
        <g class="dots"><circle cx="10.5" cy="16.4" r="1" /><circle cx="14" cy="16.4" r="1" /><circle cx="17.5" cy="16.4" r="1" /></g>
      {/if}
    </svg>
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
      {/if}
    {/if}
  </div>
  {#if shown.length}
    <div class="children" transition:slide={{ duration: 150 }}>
      {#each shown as tab (tab.id)}
        <TabItem
          {tab}
          {collapsed}
          active={activeId === tab.id}
          drop={dnd.mark(tab.id)}
          dragging={dnd.drag?.started && dnd.drag.id === tab.id}
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
    padding-inline-end: 8px;
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
  .children {
    display: flex;
    flex-direction: column;
    margin-inline-start: 14px;
    transition: var(--zen-indent);
  }
  .folder.icon-only .head {
    width: calc(var(--collapsed-tab-w) + var(--tab-mb) * 2);
    margin-inline: auto;
    justify-content: center;
  }
  .folder.icon-only .folder-icon {
    margin: 0;
  }
  .folder.icon-only .children {
    margin-inline-start: 0;
  }
</style>
