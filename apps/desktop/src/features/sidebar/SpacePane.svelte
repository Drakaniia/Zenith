<script lang="ts">
  import { flip } from 'svelte/animate';
  import { fly, slide } from 'svelte/transition';
  import { tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import TabItem from './TabItem.svelte';
  import FolderItem from './FolderItem.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { dnd } from './sidebar-dnd.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { spaceMenu } from '../../app/menus';
  import { ui } from '$shared/ui/ui.svelte';
  import type { Space, Tab, TabId } from '$features/browser/types';

  // One Zen workspace column: indicator, pinned section (folders and pinned tabs), separator
  // with "Clear", the New Tab button and the regular tabs.
  let { space, collapsed = false }: { space: Space; collapsed?: boolean } = $props();

  const activeId = $derived(space.activeTabId);
  const looseIds = $derived(space.pinnedIds.filter((id) => browser.tabs[id] && !browser.folderOf(id)));
  const folders = $derived(space.folders ?? []);
  const hasPinned = $derived(space.pinnedIds.length > 0 || folders.length > 0);
  const regular = $derived(space.tabIds.filter((id) => browser.tabs[id]));
  const split = $derived(space.split && space.split.ids.every((id) => browser.tabs[id]) ? space.split : undefined);

  type Row = { key: string; tab: Tab } | { key: string; group: Tab[] };
  /** Rows of a list, with the split tabs drawn as one group row (at the first of them) where all
   *  of them are in the list. */
  function rows(ids: TabId[]): Row[] {
    const out: Row[] = [];
    const members = split && split.ids.every((id) => ids.includes(id)) ? split.ids : null;
    const lead = members && ids.find((id) => members.includes(id));
    for (const id of ids) {
      if (members && id === lead) out.push({ key: 'split:' + id, group: members.map((m) => browser.tabs[m]) });
      else if (!members?.includes(id)) out.push({ key: id, tab: browser.tabs[id] });
    }
    return out;
  }
  const pinnedRows = $derived(rows(looseIds));
  const tabRows = $derived(rows(regular));
  const loneSplit = (id: TabId) => !!split?.ids.includes(id);

  // ---------------------------------------------------------------- indicator and renaming
  let renaming = $state(false);
  let nameInput: HTMLInputElement | undefined = $state();
  async function rename() {
    renaming = true;
    ui.renaming = true;
    await tick();
    nameInput?.focus();
    nameInput?.select();
  }
  function finishRename(save: boolean) {
    if (!renaming) return;
    const v = nameInput?.value.trim();
    if (save && v) space.name = v;
    renaming = false;
    ui.renaming = false;
  }

  // ---------------------------------------------------------------- scrolling
  let scroller: HTMLDivElement | undefined = $state();
  let atStart = $state(true);
  let atEnd = $state(true);
  let offscreen = $state<'up' | 'down' | null>(null);

  function measure() {
    const el = scroller;
    if (!el) return;
    atStart = el.scrollTop <= 1;
    atEnd = el.scrollTop + el.clientHeight >= el.scrollHeight - 1;
    const item = activeId ? el.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(activeId)}"]`) : null;
    if (!item) return (offscreen = null);
    const r = item.getBoundingClientRect();
    const b = el.getBoundingClientRect();
    offscreen = r.bottom < b.top + 8 ? 'up' : r.top > b.bottom - 8 ? 'down' : null;
  }
  $effect(() => {
    void activeId;
    void regular.length;
    void tick().then(measure);
  });
  $effect(() => {
    const el = scroller;
    if (!el) return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  });
  function scrollToActive() {
    scroller?.querySelector(`[data-tab-id="${CSS.escape(activeId ?? '')}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }
</script>

{#snippet tabRow(row: Row)}
  {#if 'group' in row}
    <!-- Zen's split-view group: the split tabs side by side in one rounded row. -->
    <div
      class="split-group"
      class:active={row.group.some((t) => t.id === activeId)}
      class:collapsed
      data-split-group
      role="group"
      aria-label="Split view"
    >
      {#each row.group as t (t.id)}
        <TabItem
          tab={t}
          grouped
          {collapsed}
          active={activeId === t.id}
          drop={dnd.mark(t.id)}
          dragging={dnd.drag?.started && dnd.drag.id === t.id}
          onpointerdown={(e) => dnd.start(e, t.id)}
        />
      {/each}
      {#if !collapsed}
        <button class="unsplit" title="Exit split view" onclick={() => browser.unsplit()}><Icon name="unsplit" size={14} /></button>
      {/if}
    </div>
  {:else}
    <TabItem
      tab={row.tab}
      {collapsed}
      active={activeId === row.tab.id}
      inSplit={loneSplit(row.tab.id)}
      drop={dnd.mark(row.tab.id)}
      dragging={dnd.drag?.started && dnd.drag.id === row.tab.id}
      onpointerdown={(e) => dnd.start(e, row.tab.id)}
    />
  {/if}
{/snippet}

<div class="pane" class:collapsed>
  {#if !collapsed}
    <div
      class="indicator"
      class:has-pinned={hasPinned}
      class:pins-collapsed={space.pinnedCollapsed}
      role="presentation"
      oncontextmenu={(e) => {
        e.preventDefault();
        ui.openMenu(e.clientX, e.clientY, spaceMenu(space));
      }}
    >
      <button
        class="indicator-main"
        title={hasPinned ? (space.pinnedCollapsed ? 'Show pinned tabs' : 'Hide pinned tabs') : space.name}
        onclick={() => hasPinned && (space.pinnedCollapsed = !space.pinnedCollapsed)}
        ondblclick={() => void rename()}
      >
        <span class="stack">
          <span class="space-icon">{space.icon}</span>
          <span class="chevron"><Icon name="chevronDown" size={14} /></span>
        </span>
        {#if renaming}
          <input
            class="space-name rename"
            bind:this={nameInput}
            value={space.name}
            spellcheck="false"
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => {
              e.stopPropagation();
              if (e.key === 'Enter') finishRename(true);
              else if (e.key === 'Escape') finishRename(false);
            }}
            onblur={() => finishRename(true)}
          />
        {:else}
          <span class="space-name ellipsis">{space.name}</span>
        {/if}
      </button>
      <button
        class="indicator-actions"
        title="Space options"
        onclick={(e) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          ui.openMenu(r.left, r.bottom + 4, [{ label: 'Rename space', icon: 'edit', run: () => void rename() }, ...spaceMenu(space)]);
        }}
      >
        <Icon name="more" size={16} />
      </button>
    </div>
  {:else}
    <button class="indicator-mini" title={space.name} onclick={() => (ui.spaceEditor = { spaceId: space.id })}>
      <span>{space.icon}</span>
    </button>
  {/if}

  <div
    class="scroller"
    class:line-top={!atStart}
    class:line-bottom={!atEnd}
    bind:this={scroller}
    onscroll={measure}
  >
    {#if !space.pinnedCollapsed || collapsed}
      <div class="list pinned" data-zone="pinned" role="tablist" aria-label="Pinned tabs" transition:slide={{ duration: 150 }}>
        {#each folders as folder (folder.id)}
          <div animate:flip={{ duration: 160 }}>
            <FolderItem {folder} {space} {collapsed} />
          </div>
        {/each}
        {#each pinnedRows as row (row.key)}
          <div animate:flip={{ duration: 160 }}>{@render tabRow(row)}</div>
        {/each}
        {#if !hasPinned && dnd.active}
          <div class="pin-hint">{collapsed ? '📌' : 'Drop here to pin'}</div>
        {/if}
      </div>
    {/if}

    {#if hasPinned || dnd.active}
      <div class="separator" class:collapsed>
        <span class="line"></span>
        {#if regular.length && settings.value.showClearButton && !collapsed}
          <button class="clear" title="Close all unpinned tabs" onclick={() => browser.clearSpace(space.id)}>
            <Icon name="arrowDown" size={10} stroke={2.6} />
            <span>Clear</span>
          </button>
        {/if}
      </div>
    {/if}

    <button class="new-tab" class:collapsed title={collapsed ? 'New Tab' : undefined} onclick={() => ui.openPalette('newTab')}>
      <span class="new-tab-bg"></span>
      <span class="new-tab-icon"><Icon name="plus" size={16} /></span>
      {#if !collapsed}<span class="new-tab-label">New Tab</span>{/if}
    </button>

    <div class="list tabs" data-zone="tabs" role="tablist" aria-label="Tabs">
      {#each tabRows as row (row.key)}
        <div animate:flip={{ duration: 160 }} in:fly={{ y: -6, duration: 160 }}>{@render tabRow(row)}</div>
      {/each}
      <div class="list-tail" role="presentation" ondblclick={() => ui.openPalette('newTab')}></div>
    </div>
  </div>

  {#if offscreen && !collapsed}
    {@const t = browser.tabs[activeId ?? '']}
    {#if t}
      <button class="scroll-to {offscreen}" onclick={scrollToActive} transition:fly={{ y: offscreen === 'up' ? -6 : 6, duration: 150 }}>
        <Icon name={offscreen === 'up' ? 'arrowUp' : 'arrowDown'} size={13} />
        <Favicon url={t.url} favicon={t.favicon} size={16} />
      </button>
    {/if}
  {/if}
</div>

<style>
  .pane {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    color: var(--text);
  }

  /* ---- Space indicator (zen-current-workspace-indicator) ---- */
  .indicator {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    height: var(--zen-indicator-h);
    padding: 4px calc(var(--zen-pad) + 2px) 4px calc(var(--zen-pad) + 2px);
  }
  .indicator-main {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 0 8px 0 9px;
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    font-size: 13px;
    font-weight: 600;
    text-align: left;
    transition: background-color 0.1s;
  }
  .indicator:hover .indicator-main {
    background: var(--hover);
  }
  .stack {
    position: relative;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    flex: none;
  }
  .space-icon {
    font-size: 15px;
    line-height: 1;
    transition: opacity 0.15s;
  }
  .chevron {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    opacity: 0;
    transition:
      opacity 0.15s,
      transform 0.1s;
  }
  .indicator.has-pinned:hover .space-icon,
  .indicator.pins-collapsed .space-icon {
    opacity: 0;
  }
  .indicator.has-pinned:hover .chevron,
  .indicator.pins-collapsed .chevron {
    opacity: 1;
  }
  .indicator.pins-collapsed .chevron {
    transform: rotate(-90deg);
  }
  .space-name {
    flex: 1;
    min-width: 0;
    color: color-mix(in srgb, currentColor 70%, transparent);
  }
  .rename {
    margin: 0;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    font: inherit;
    color: var(--text);
  }
  .indicator-actions {
    position: absolute;
    right: calc(var(--zen-pad) + 6px);
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: max(calc(var(--r-tab) - 4px), 4px);
    corner-shape: var(--sq);
    opacity: 0;
    visibility: hidden;
    transition: opacity 0.1s;
  }
  .indicator:hover .indicator-actions {
    opacity: 1;
    visibility: visible;
  }
  .indicator-actions:hover {
    background: var(--hover);
  }
  .indicator-mini {
    flex: none;
    height: 38px;
    display: grid;
    place-items: center;
    font-size: 15px;
    opacity: 0.4;
    transition: opacity 0.1s;
  }
  .indicator-mini:hover {
    opacity: 1;
  }

  /* ---- Scroll box with Zen's 1px overflow edges ---- */
  .scroller {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden auto;
    overscroll-behavior: contain;
    padding: 0 var(--zen-pad);
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, currentColor 35%, transparent) transparent;
    border-top: 1px solid transparent;
    border-bottom: 1px solid transparent;
    transition: border-color 0.1s;
  }
  .scroller.line-top {
    border-top-color: var(--line);
  }
  .scroller.line-bottom {
    border-bottom-color: var(--line);
  }
  .pane.collapsed .scroller {
    padding: 0;
    scrollbar-width: none;
  }
  .list {
    display: flex;
    flex-direction: column;
  }
  .tabs {
    flex: 1;
    min-height: 60px;
  }
  .list-tail {
    flex: 1;
    min-height: 40px;
  }
  .pin-hint {
    height: 36px;
    margin: 2px;
    display: grid;
    place-items: center;
    border: 1px dashed var(--line-strong);
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    color: var(--text-3);
    font-size: 12px;
  }

  /* ---- Pinned separator with the Clear button ---- */
  .separator {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    height: 22px;
    margin-inline: 4px 2px;
  }
  .separator.collapsed {
    margin-inline: 8px;
  }
  .line {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    height: 1px;
    background: var(--separator);
    transition: opacity 0.15s;
  }
  .clear {
    position: relative;
    display: flex;
    align-items: center;
    gap: 2px;
    height: 20px;
    padding: 0 6px 0 8px;
    border-radius: 6px;
    font-size: 10px;
    font-weight: 500;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.15s;
  }
  .clear :global(svg) {
    transition: transform 0.15s ease-in-out;
  }
  .clear:active :global(svg) {
    transform: translateY(2px);
  }
  :global(.sidebar:hover) .separator:has(.clear) .line {
    opacity: 0;
  }
  :global(.sidebar:hover) .clear {
    opacity: 0.5;
    pointer-events: auto;
  }
  .clear:hover {
    opacity: 1 !important;
  }

  /* ---- New Tab button (looks like a tab) ---- */
  .new-tab {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    height: calc(var(--tab-h) + var(--tab-mb) * 2);
    padding: var(--tab-mb);
    font-size: var(--tab-font);
    color: color-mix(in srgb, currentColor 70%, transparent);
    transition: scale 0.1s ease;
  }
  .new-tab:active {
    scale: 0.985;
  }
  .new-tab-bg {
    position: absolute;
    inset: var(--tab-mb);
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    transition: background-color 0.1s;
  }
  .new-tab:hover .new-tab-bg {
    background: var(--hover);
  }
  .new-tab-icon {
    position: relative;
    display: grid;
    margin-inline: calc(var(--tab-pad) + 3px) 10px;
  }
  .new-tab-label {
    position: relative;
  }
  .new-tab.collapsed {
    width: calc(var(--collapsed-tab-w) + var(--tab-mb) * 2);
    margin-inline: auto;
    justify-content: center;
  }
  .new-tab.collapsed .new-tab-icon {
    margin: 0;
  }

  /* ---- Split group row ---- */
  .split-group {
    position: relative;
    display: flex;
    align-items: center;
    min-height: calc(var(--tab-h) + var(--tab-mb) * 2);
    margin: var(--tab-mb);
    padding: 0 2px;
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    transition:
      scale 0.1s ease,
      background-color 0.1s;
  }
  .split-group:hover {
    background: var(--element-bg);
  }
  .split-group.active {
    background: var(--tab-selected);
    box-shadow: var(--tab-shadow);
  }
  .split-group.collapsed {
    flex-direction: column;
    width: calc(var(--collapsed-tab-w) - 10px);
    margin-inline: auto;
    padding: 0;
    outline: 2px solid var(--element-bg);
    outline-offset: -2px;
  }
  .unsplit {
    flex: none;
    display: none;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-inline-end: 4px;
    border-radius: 6px;
  }
  .split-group:hover .unsplit {
    display: grid;
  }
  .unsplit:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }

  /* ---- Scroll to the selected tab (zen-workspace-scroll-to-tab) ---- */
  .scroll-to {
    position: absolute;
    left: 0;
    right: 0;
    margin-inline: auto;
    width: fit-content;
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px;
    border-radius: 999px;
    color: var(--text);
    background: color-mix(in srgb, var(--accent-button) 50%, black);
    box-shadow: var(--tab-shadow), 0 0 10px rgba(0, 0, 0, 0.1);
    transition: scale 0.15s ease-in-out;
  }
  :global(:root[data-theme='light']) .scroll-to {
    background: color-mix(in srgb, var(--accent-button) 10%, white);
  }
  .scroll-to:hover:not(:active) {
    scale: 1.05;
  }
  .scroll-to.up {
    top: calc(var(--zen-indicator-h) + 8px);
  }
  .scroll-to.down {
    bottom: 8px;
  }
</style>
