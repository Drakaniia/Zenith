<script lang="ts">
  import { flip } from 'svelte/animate';
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import Favicon from './Favicon.svelte';
  import WindowControls from './WindowControls.svelte';
  import ProfileButton from './sidebar/ProfileButton.svelte';
  import SidebarFooter from './sidebar/SidebarFooter.svelte';
  import UrlPill from './UrlPill.svelte';
  import ExtensionButtons from './ExtensionButtons.svelte';
  import { api } from '../lib/ipc';
  import { appMenu } from '../lib/menus';
  import { browser, needsWebview, tabTitle } from '../lib/browser.svelte';
  import { dnd } from '../lib/sidebar-dnd.svelte';
  import { ui, type MenuItem } from '../lib/ui.svelte';
  import { sidebarMenu, spaceMenu, tabMenu } from '../lib/menus';
  import { runAction, withHint } from '../lib/actions';
  import { settings } from '../lib/settings.svelte';
  import { incognito } from '../lib/app.svelte';
  import type { Folder, Tab } from '../lib/types';

  // The sidebar as a horizontal tab strip (sidebar position top or bottom): Essentials and pinned
  // tabs as icons, pinned folders, the space's tabs and the New Tab button, then the sidebar's
  // footer (downloads, notes, the space switcher, "+"). As the window's top row it also holds the
  // window controls. The collapsed layout makes every tab icon-only.
  // `merged`: the strip is also the toolbar (one row at the top of the window): navigation
  // buttons, the address bar and the page and app menus sit between the tabs and the footer.
  let {
    edge = 'top',
    floating = false,
    collapsed = false,
    controls = false,
    merged = false,
  }: { edge?: 'top' | 'bottom'; floating?: boolean; collapsed?: boolean; controls?: boolean; merged?: boolean } = $props();

  const live = $derived(browser.activeTab?.live ? browser.activeTab : undefined);
  const space = $derived(browser.activeSpace);
  const activeId = $derived(browser.activeTab?.id);
  const essentials = $derived(browser.essentials.map((id) => browser.tabs[id]).filter(Boolean));
  const folders = $derived(space?.folders ?? []);
  const pinned = $derived(
    (space?.pinnedIds ?? []).filter((id) => browser.tabs[id] && !browser.folderOf(id)).map((id) => browser.tabs[id]),
  );
  const regular = $derived((space?.tabIds ?? []).map((id) => browser.tabs[id]).filter(Boolean));
  const splitIds = $derived(space?.split?.ids ?? []);

  const sleeping = (t: Tab) => !t.live && needsWebview(t.url);
  const picked = (t: Tab) => ui.selectedTabs.length > 1 && ui.selectedTabs.includes(t.id);

  /** Same as a sidebar tab: Ctrl adds the tab to the picked ones, Shift picks a range. */
  function click(e: MouseEvent, tab: Tab) {
    if (e.ctrlKey || e.metaKey) ui.toggleTabSelected(tab.id, browser.activeTab?.id);
    else if (e.shiftKey) ui.selectTabRange(browser.order(), ui.selectedTabs[0] ?? browser.activeTab?.id, tab.id);
    else {
      ui.clearTabSelection();
      browser.activate(tab.id);
    }
  }

  /** Pinned tabs unload instead of closing; an unloaded pinned tab's X removes it, as in Zen. */
  function close(e: MouseEvent, tab: Tab) {
    e.stopPropagation();
    if (tab.pinned && sleeping(tab)) browser.removeTab(tab.id, true);
    else browser.closeTab(tab.id);
  }

  function tabContext(e: MouseEvent, tab: Tab) {
    e.preventDefault();
    ui.openMenu(e.clientX, e.clientY, tabMenu(tab));
  }

  /** Menus hang below the strip at the top of the window and above it at the bottom. */
  function below(el: HTMLElement) {
    const r = el.getBoundingClientRect();
    return { x: r.left, y: edge === 'top' ? r.bottom + 4 : r.top - 8 };
  }

  // ---------------------------------------------------------------- folders
  // A folder is a button; its tabs open from a menu (there is no room to expand it in a row).
  function folderTabs(folder: Folder): Tab[] {
    return space ? browser.folderMembers(space, folder).map((id) => browser.tabs[id]).filter(Boolean) : [];
  }
  function openFolder(e: MouseEvent, folder: Folder) {
    const items: MenuItem[] = folderTabs(folder).map((t) => ({
      label: tabTitle(t),
      site: { url: t.url, favicon: t.favicon },
      checked: t.id === activeId,
      run: () => browser.activate(t.id),
    }));
    if (!items.length) items.push({ label: '(Empty)', heading: true });
    const p = below(e.currentTarget as HTMLElement);
    ui.openMenu(p.x, p.y, items);
  }
  function folderContext(e: MouseEvent, folder: Folder) {
    e.preventDefault();
    const sid = space?.id;
    ui.openMenu(e.clientX, e.clientY, [
      {
        label: 'New tab in folder',
        icon: 'plus',
        run: () => {
          const t = browser.openTab(undefined, { pinned: true, spaceId: sid });
          browser.moveToFolder(t.id, folder.id);
        },
      },
      { label: 'Unload all tabs', icon: 'moonSleep', run: () => folderTabs(folder).forEach((t) => void browser.sleep(t.id)) },
      { separator: true },
      { label: 'Unpack folder', icon: 'folder', run: () => browser.deleteFolder(folder.id) },
      { label: 'Delete folder and its tabs', icon: 'trash', danger: true, run: () => browser.deleteFolder(folder.id, true) },
    ]);
  }

  // ---------------------------------------------------------------- scrolling
  // The tabs scroll sideways; the mouse wheel scrolls them too, and the active tab stays in view.
  let scroller: HTMLDivElement | undefined = $state();
  function onwheel(e: WheelEvent) {
    if (!scroller || Math.abs(e.deltaX) > Math.abs(e.deltaY)) return;
    if (scroller.scrollWidth <= scroller.clientWidth) return;
    e.preventDefault();
    scroller.scrollLeft += e.deltaY;
  }
  $effect(() => {
    const id = activeId;
    void regular.length;
    void tick().then(() =>
      scroller?.querySelector(`[data-tab-id="${CSS.escape(id ?? '')}"]`)?.scrollIntoView({ block: 'nearest', inline: 'nearest' }),
    );
  });

  function stripContext(e: MouseEvent) {
    if (e.defaultPrevented) return;
    e.preventDefault();
    ui.openMenu(e.clientX, e.clientY, sidebarMenu());
  }
</script>

{#snippet tabButton(tab: Tab, iconOnly: boolean)}
  {@const mark = dnd.mark(tab.id)}
  <div
    class="tab"
    class:icon-only={iconOnly}
    class:active={activeId === tab.id}
    class:picked={picked(tab)}
    class:sleeping={sleeping(tab)}
    class:in-split={splitIds.includes(tab.id)}
    class:dragging={dnd.drag?.started && dnd.drag.id === tab.id}
    role="tab"
    tabindex="-1"
    aria-selected={activeId === tab.id}
    data-tab-id={tab.id}
    title={tabTitle(tab)}
    onclick={(e) => click(e, tab)}
    onauxclick={(e) => e.button === 1 && browser.closeTab(tab.id)}
    onkeydown={(e) => e.key === 'Enter' && browser.activate(tab.id)}
    oncontextmenu={(e) => tabContext(e, tab)}
    onpointerdown={(e) => dnd.start(e, tab.id)}
  >
    <span class="bg"></span>
    <span class="icon">
      {#if tab.crashed}
        <span class="warn"><Icon name="alert" size={15} /></span>
      {:else}
        <Favicon url={tab.url} favicon={tab.favicon} />
      {/if}
      {#if iconOnly && (tab.audible || tab.muted)}
        <span class="audio-badge"><Icon name={tab.muted ? 'volumeOff' : 'volume'} size={9} stroke={2.6} /></span>
      {/if}
    </span>
    {#if !iconOnly}
      <span class="title ellipsis">{tabTitle(tab)}</span>
      {#if tab.audible || tab.muted}
        <button
          class="mini"
          title={tab.muted ? 'Unmute tab' : 'Mute tab'}
          onclick={(e) => {
            e.stopPropagation();
            browser.setMuted(tab.id, !tab.muted);
          }}
        >
          <Icon name={tab.muted ? 'volumeOff' : 'volume'} size={13} />
        </button>
      {/if}
      {#if !tab.essential}
        <button class="mini close" title="Close tab" aria-label="Close tab" onclick={(e) => close(e, tab)}>
          <Icon name="close" size={13} />
        </button>
      {/if}
    {/if}
    {#if mark}<span class="mark" class:after={mark === 'after'}></span>{/if}
  </div>
{/snippet}

<div
  class="strip {edge}"
  class:floating
  class:has-controls={controls && !floating}
  role="toolbar"
  aria-label="Tabs"
  tabindex="-1"
  data-horizontal
  data-tauri-drag-region
  oncontextmenu={stripContext}
>
  {#if !incognito}
    <ProfileButton />
  {/if}
  <button
    class="icon-btn"
    title={withHint(settings.value.compact ? 'Turn off compact mode' : 'Compact mode', 'toggleCompact')}
    data-action="compact"
    onclick={() => void runAction('toggleCompact')}
  >
    <Icon name="sidebar" />
  </button>
  {#if incognito}
    <span class="incognito-badge" title="Incognito window: history, cookies and site data are deleted when it closes">
      <Icon name="incognito" size={14} />
    </span>
  {/if}

  {#if merged}
    <button class="icon-btn" title={withHint('Back', 'back')} disabled={!live?.canGoBack} onclick={() => live && api.tabCommand(live.id, 'back')}>
      <Icon name="back" />
    </button>
    <button class="icon-btn" title={withHint('Forward', 'forward')} disabled={!live?.canGoForward} onclick={() => live && api.tabCommand(live.id, 'forward')}>
      <Icon name="forward" />
    </button>
    {#if live?.loading}
      <button class="icon-btn" title="Stop" onclick={() => api.tabCommand(live.id, 'stop')}><Icon name="stop" /></button>
    {:else}
      {@const t = browser.activeTab}
      <button
        class="icon-btn"
        title={withHint('Reload', 'reload')}
        disabled={!live && !t?.crashed}
        onclick={() => t && (t.crashed ? browser.reviveCrashed(t.id) : api.tabCommand(t.id, 'reload'))}
      >
        <Icon name="reload" />
      </button>
    {/if}
  {/if}

  {#if essentials.length}
    <div class="group essentials" data-zone="essentials">
      {#each essentials as tab (tab.id)}
        <div class="cell" animate:flip={{ duration: 160 }}>{@render tabButton(tab, true)}</div>
      {/each}
    </div>
  {/if}

  {#if space}
    <button
      class="space"
      title={`${space.name} — space options`}
      onclick={(e) => {
        const p = below(e.currentTarget as HTMLElement);
        ui.openMenu(p.x, p.y, spaceMenu(space));
      }}
      oncontextmenu={(e) => {
        e.preventDefault();
        ui.openMenu(e.clientX, e.clientY, spaceMenu(space));
      }}
    >
      <span class="space-icon">{space.icon}</span>
      {#if !collapsed}<span class="space-name ellipsis">{space.name}</span>{/if}
    </button>
  {/if}

  {#if pinned.length || folders.length || dnd.active}
    <div class="group pinned" data-zone="pinned">
      {#each folders as folder (folder.id)}
        {@const members = folderTabs(folder)}
        <button
          class="folder"
          class:active={members.some((t) => t.id === activeId)}
          class:dropping={dnd.drag?.started && dnd.drag.target?.zone === 'folder' && dnd.drag.target.folderId === folder.id}
          title={`${folder.name} (${members.length})`}
          data-folder-head={folder.id}
          onclick={(e) => openFolder(e, folder)}
          oncontextmenu={(e) => folderContext(e, folder)}
        >
          <Icon name="folder" size={15} />
          {#if !collapsed}<span class="ellipsis">{folder.name}</span>{/if}
          <Icon name="chevronDown" size={12} />
        </button>
      {/each}
      {#each pinned as tab (tab.id)}
        <div class="cell" animate:flip={{ duration: 160 }}>{@render tabButton(tab, true)}</div>
      {/each}
      {#if !pinned.length && !folders.length}
        <div class="pin-hint">Pin</div>
      {/if}
    </div>
    <span class="divider"></span>
  {/if}

  <div class="scroller" bind:this={scroller} {onwheel}>
    <div class="group tabs" data-zone="tabs">
      {#each regular as tab (tab.id)}
        <div class="cell grow" class:icon-only={collapsed} animate:flip={{ duration: 160 }}>{@render tabButton(tab, collapsed)}</div>
      {/each}
    </div>
  </div>
  <button class="icon-btn new-tab" title={withHint('New Tab', 'newTab')} onclick={() => ui.openPalette('newTab')}>
    <Icon name="plus" />
  </button>

  {#if merged}
    <UrlPill variant="toolbar" />
    <span class="spacer after-pill" data-tauri-drag-region></span>
    <button
      class="icon-btn"
      class:on={!!browser.activeSplit}
      title={browser.activeSplit ? withHint('Exit split view', 'unsplit') : withHint('Split view', 'splitVertical')}
      data-action="split"
      onclick={() => void runAction(browser.activeSplit ? 'unsplit' : 'splitVertical')}
      disabled={!browser.activeTab}
    >
      <Icon name={browser.activeSplit ? 'unsplit' : 'split'} />
    </button>
    <ExtensionButtons />
    <button
      class="icon-btn"
      title="Menu"
      data-action="app-menu"
      onclick={(e) => {
        const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
        ui.openMenu(Math.max(4, r.right - 240), r.bottom + 6, appMenu());
      }}
    >
      <Icon name="more" />
    </button>
  {:else}
    <span class="spacer" data-tauri-drag-region></span>
  {/if}
  <SidebarFooter inline />
  {#if controls && !floating}
    <WindowControls />
  {/if}
</div>

{#if dnd.drag?.started && browser.tabs[dnd.drag.id]}
  {@const t = browser.tabs[dnd.drag.id]}
  <div class="ghost" style:transform="translate({dnd.drag.x + 12}px, {dnd.drag.y + 8}px)">
    <Favicon url={t.url} favicon={t.favicon} />
    <span class="ellipsis">{tabTitle(t)}</span>
  </div>
{/if}

<style>
  .strip {
    height: 100%;
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    padding: 0 0 0 var(--zen-pad);
    color: var(--text);
  }
  /* Without window controls at its end the strip keeps a margin there too. */
  .strip:not(.has-controls) {
    padding-right: var(--zen-pad);
  }
  .strip > :global(*) {
    flex-shrink: 0;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .cell {
    display: flex;
  }
  .scroller {
    flex: 0 1 auto !important;
    min-width: 40px;
    height: 100%;
    display: flex;
    align-items: center;
    overflow: auto hidden;
    scrollbar-width: none;
    overscroll-behavior: contain;
  }
  .tabs {
    min-width: 100%;
    height: 100%;
  }
  /* Tabs share the width like a classic tab row: up to 200px each, down to 100px, then scroll. */
  .cell.grow {
    flex: 0 1 200px;
    min-width: 100px;
  }
  .cell.grow.icon-only {
    flex: none;
    min-width: 0;
  }
  .spacer {
    flex: 1 1 0 !important;
    min-width: 8px;
    align-self: stretch;
  }
  /* The address bar gives up width to the tabs, which scroll first once the row is full. */
  .strip > :global(.pill.toolbar) {
    flex: 100 1 0%;
    min-width: 140px;
  }
  .spacer.after-pill {
    flex: 1 1 0 !important;
    min-width: 4px;
  }
  .icon-btn.on {
    background: var(--element-bg);
  }
  .divider {
    width: 1px;
    height: 18px;
    background: var(--separator);
  }

  .tab {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 6px 0 10px;
    font-size: 13px;
    transition:
      scale 0.1s ease,
      opacity var(--med) ease;
  }
  .tab.icon-only {
    flex: none;
    width: 36px;
    padding: 0;
    justify-content: center;
  }
  .tab:active:not(:has(button:active)) {
    scale: 0.98;
  }
  .bg {
    position: absolute;
    inset: 0;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    pointer-events: none;
    transition: background-color 0.1s ease;
  }
  .tab:hover .bg {
    background: var(--hover);
  }
  .tab.active .bg {
    background: var(--tab-selected);
    box-shadow: var(--tab-shadow);
  }
  .tab.picked .bg {
    background: color-mix(in srgb, var(--accent) 24%, var(--hover));
  }
  .tab.dragging {
    opacity: 0.4;
  }
  /* Tabs shown in split view get Zen's split colour as a bar along the strip's inner edge. */
  .tab.in-split::after {
    content: '';
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 1px;
    height: 2px;
    border-radius: 2px;
    background: var(--split-outline);
  }
  .strip.bottom .tab.in-split::after {
    bottom: auto;
    top: 1px;
  }
  .icon {
    position: relative;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    flex: none;
  }
  .tab.sleeping .icon {
    opacity: 0.5;
  }
  .warn {
    color: var(--warn);
  }
  .title {
    position: relative;
    flex: 1;
    min-width: 0;
    line-height: 1.2;
  }
  .mini {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    opacity: 0.8;
  }
  .mini:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
    opacity: 1;
  }
  .close {
    visibility: hidden;
  }
  .tab:hover .close,
  .tab.active .close {
    visibility: visible;
  }
  .audio-badge {
    position: absolute;
    right: -7px;
    top: -6px;
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--panel-bg);
    box-shadow: 0 0 0 1px var(--line);
  }
  /* Drop indicator: a 2px bar before or after the tab. */
  .mark {
    position: absolute;
    z-index: 2;
    top: 6px;
    bottom: 6px;
    left: -2px;
    width: 2px;
    border-radius: 2px;
    background: var(--drag-indicator);
    pointer-events: none;
  }
  .mark.after {
    left: auto;
    right: -2px;
  }

  .space,
  .folder {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    max-width: 150px;
    padding: 0 8px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    font-size: 12.5px;
    font-weight: 600;
    transition: background-color 0.1s;
  }
  .space:hover,
  .folder:hover {
    background: var(--hover);
  }
  .space-icon {
    font-size: 14px;
    line-height: 1;
  }
  .space-name {
    min-width: 0;
    color: color-mix(in srgb, currentColor 70%, transparent);
  }
  .folder {
    font-weight: 500;
    color: color-mix(in srgb, currentColor 85%, transparent);
  }
  .folder.active {
    background: var(--element-bg);
  }
  .folder.dropping {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .pin-hint {
    display: grid;
    place-items: center;
    height: 28px;
    padding: 0 10px;
    border: 1px dashed var(--line-strong);
    border-radius: var(--r-btn);
    color: var(--text-3);
    font-size: 11px;
  }
  .incognito-badge {
    display: inline-grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 13px;
    background: color-mix(in srgb, var(--accent) 28%, transparent);
  }

  .ghost {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 240px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    background: var(--tab-selected);
    backdrop-filter: blur(20px);
    box-shadow: var(--zen-big-shadow);
    pointer-events: none;
    font-size: 13px;
  }
</style>
