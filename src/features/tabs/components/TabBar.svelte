<script lang="ts">
  import { Copy, Globe, LoaderCircle, MoreHorizontal, Plus, RotateCw, X } from 'lucide-svelte';
  import { focusAddressInput } from '../../../shell/utils/focus';
  import { BROWSER_HOME_URL } from '../../../shared/utils/internal-urls';
  import { type BrowserTab, tabsState } from '../state/tabs.svelte';

  interface ContextMenuState {
    x: number;
    y: number;
    tabId: string;
  }

  interface Props {
    embedded?: boolean;
    onOpenTab?: () => void;
  }

  let { embedded = false, onOpenTab }: Props = $props();

  let contextMenu = $state<ContextMenuState | null>(null);
  let visibleTabs = $derived(tabsState.tabs.filter((tab) => tab.url !== BROWSER_HOME_URL));
  let canOpenTab = $derived(
    Boolean(tabsState.activeTab && tabsState.activeTab.url !== BROWSER_HOME_URL)
  );
  let contextTab = $derived(
    contextMenu ? (visibleTabs.find((tab) => tab.id === contextMenu?.tabId) ?? null) : null
  );

  function openDefaultTab(): void {
    if (onOpenTab) {
      onOpenTab();
    } else if (canOpenTab) {
      tabsState.openInternalTab(BROWSER_HOME_URL);
      focusAddressInput();
    }

    closeContextMenu();
  }

  function activateTab(id: string): void {
    tabsState.setActiveTab(id);
  }

  function closeTab(event: MouseEvent, id: string): void {
    event.stopPropagation();
    tabsState.closeTab(id);

    if (contextMenu?.tabId === id) {
      closeContextMenu();
    }
  }

  function duplicateTab(id: string): void {
    tabsState.duplicateTab(id);
    closeContextMenu();
  }

  function closeOtherTabs(id: string): void {
    tabsState.closeOtherTabs(id);
    closeContextMenu();
  }

  function requestReload(id: string): void {
    tabsState.requestReload(id);
    closeContextMenu();
  }

  function openContextMenu(event: MouseEvent, id: string): void {
    event.preventDefault();
    event.stopPropagation();
    tabsState.setActiveTab(id);

    const menuWidth = 196;
    const menuHeight = 164;
    const margin = 8;
    const x = Math.min(event.clientX, window.innerWidth - menuWidth - margin);
    const y = Math.min(event.clientY, window.innerHeight - menuHeight - margin);

    contextMenu = {
      x: Math.max(margin, x),
      y: Math.max(margin, y),
      tabId: id
    };
  }

  function openContextMenuFromKeyboard(event: KeyboardEvent, id: string): void {
    event.preventDefault();
    event.stopPropagation();
    tabsState.setActiveTab(id);

    const target = event.currentTarget;

    if (!(target instanceof HTMLElement)) {
      return;
    }

    const rect = target.getBoundingClientRect();
    const menuWidth = 196;
    const menuHeight = 164;
    const margin = 8;
    const x = Math.min(rect.left, window.innerWidth - menuWidth - margin);
    const y = Math.min(rect.bottom + 4, window.innerHeight - menuHeight - margin);

    contextMenu = {
      x: Math.max(margin, x),
      y: Math.max(margin, y),
      tabId: id
    };
  }

  function closeContextMenu(): void {
    contextMenu = null;
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      closeContextMenu();
    }
  }

  function handleTabKeydown(event: KeyboardEvent, id: string): void {
    const key = event.key.toLocaleLowerCase();

    if (event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey && key === 'h') {
      event.preventDefault();
      event.stopPropagation();
      tabsState.toggleTabSelection(id);
      return;
    }

    if (event.ctrlKey && !event.altKey && !event.metaKey) {
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        event.preventDefault();
        event.stopPropagation();
        tabsState.setActiveTab(id);
        tabsState.moveActiveTab(event.key === 'ArrowLeft' ? -1 : 1);
        focusActiveTabAfterReorder();
        return;
      }
    }

    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault();
      event.stopPropagation();
      focusAdjacentTab(id, event.key === 'ArrowLeft' ? -1 : 1);
      return;
    }

    if (event.key === 'ContextMenu' || event.key === 'Apps') {
      openContextMenuFromKeyboard(event, id);
      return;
    }

    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      tabsState.setActiveTab(id);
    }
  }

  function handleFaviconError(tab: BrowserTab): void {
    tabsState.updateTab(tab.id, { favicon_url: null });
  }

  function focusAdjacentTab(id: string, direction: -1 | 1): void {
    const index = visibleTabs.findIndex((tab) => tab.id === id);

    if (index === -1) {
      return;
    }

    const targetIndex = (index + direction + visibleTabs.length) % Math.max(visibleTabs.length, 1);
    const targetTab = visibleTabs[targetIndex];

    if (!targetTab) {
      return;
    }

    tabsState.setActiveTab(targetTab.id);
    focusActiveTabAfterReorder();
  }

  function focusActiveTabAfterReorder(): void {
    window.requestAnimationFrame(() => {
      const activeTab = document.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
      activeTab?.focus();
    });
  }
</script>

<svelte:window onclick={closeContextMenu} onkeydown={handleWindowKeydown} />

<nav
  class={`tab-bar ${embedded ? 'tab-bar--embedded' : ''}`}
  aria-label="Browser tabs"
  data-tauri-drag-region="deep"
>
  <div class="tab-strip" role="tablist" aria-label="Open tabs" aria-multiselectable="true">
    {#each visibleTabs as tab (tab.id)}
      <div
        class={`tab ${tab.id === tabsState.activeTabId ? 'tab--active' : ''} ${tabsState.isTabSelected(tab.id) ? 'tab--selected' : ''}`}
        role="tab"
        aria-selected={tab.id === tabsState.activeTabId}
        aria-label={`${tab.title || tab.url || 'Untitled'} tab${tabsState.isTabSelected(tab.id) ? ', selected' : ''}`}
        tabindex={tab.id === tabsState.activeTabId ? 0 : -1}
        title={tab.title || tab.url || 'Untitled'}
        data-tauri-drag-region="false"
        onclick={() => activateTab(tab.id)}
        onkeydown={(event) => handleTabKeydown(event, tab.id)}
        oncontextmenu={(event) => openContextMenu(event, tab.id)}
      >
        <span class="tab__icon" aria-hidden="true">
          {#if tab.favicon_url}
            <img
              class="tab__favicon"
              src={tab.favicon_url}
              alt=""
              draggable="false"
              onerror={() => handleFaviconError(tab)}
            />
          {:else}
            <Globe size={15} strokeWidth={2.1} />
          {/if}

          {#if tab.is_loading}
            <span class="tab__spinner">
              <LoaderCircle size={12} strokeWidth={2.4} />
            </span>
          {/if}
        </span>

        <span class="tab__title">{tab.title || tab.url || 'Untitled'}</span>

        <button
          class="tab__close"
          type="button"
          aria-label={`Close ${tab.title || 'tab'}`}
          title="Close tab"
          data-tauri-drag-region="false"
          onclick={(event) => closeTab(event, tab.id)}
        >
          <X size={14} strokeWidth={2.3} />
        </button>
      </div>

      {#if tab.id === tabsState.activeTabId && canOpenTab}
        <button
          class="new-tab-button"
          type="button"
          aria-label="Open default screen"
          title="Default screen"
          data-tauri-drag-region="false"
          onclick={openDefaultTab}
        >
          <Plus size={17} strokeWidth={2.35} />
        </button>
      {/if}
    {/each}
  </div>
</nav>

{#if contextMenu && contextTab}
  <div
    class="tab-menu"
    role="menu"
    tabindex="-1"
    style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px;`}
  >
    <button
      class="tab-menu__item"
      type="button"
      role="menuitem"
      onclick={() => requestReload(contextTab.id)}
    >
      <RotateCw size={15} strokeWidth={2.25} />
      <span>Reload</span>
    </button>
    <button
      class="tab-menu__item"
      type="button"
      role="menuitem"
      onclick={() => duplicateTab(contextTab.id)}
    >
      <Copy size={15} strokeWidth={2.25} />
      <span>Duplicate Tab</span>
    </button>
    <button
      class="tab-menu__item"
      type="button"
      role="menuitem"
      onclick={() => tabsState.closeTab(contextTab.id)}
    >
      <X size={15} strokeWidth={2.25} />
      <span>Close Tab</span>
    </button>
    <button
      class="tab-menu__item"
      type="button"
      role="menuitem"
      disabled={visibleTabs.length <= 1}
      onclick={() => closeOtherTabs(contextTab.id)}
    >
      <MoreHorizontal size={15} strokeWidth={2.25} />
      <span>Close Other Tabs</span>
    </button>
  </div>
{/if}

<style>
  .tab-bar {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-items: center;
    width: 100%;
    height: var(--shell-tabbar-height);
    min-height: var(--shell-tabbar-height);
    padding: 3px 6px 3px 8px;
    color: var(--color-text-primary);
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.035), transparent),
      color-mix(in srgb, var(--color-surface) 88%, var(--color-background));
    border-bottom: 1px solid var(--color-border);
    user-select: none;
  }

  .tab-bar--embedded {
    height: 100%;
    min-height: 0;
    background: transparent;
    border-bottom: 0;
  }

  .tab-strip {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    height: 30px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .tab-strip::-webkit-scrollbar {
    display: none;
  }

  .tab {
    position: relative;
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr) 24px;
    flex: 0 1 232px;
    align-items: center;
    gap: 6px;
    width: clamp(132px, 18vw, 232px);
    min-width: 132px;
    max-width: 232px;
    height: 30px;
    padding: 0 5px 0 8px;
    overflow: hidden;
    color: var(--color-text-secondary);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px 8px 6px 6px;
    cursor: default;
  }

  .tab::after {
    position: absolute;
    inset: auto 10px 0;
    height: 2px;
    content: '';
    background: transparent;
    border-radius: 999px;
  }

  .tab:hover {
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-surface-elevated) 68%, transparent);
    border-color: color-mix(in srgb, var(--color-border) 72%, transparent);
  }

  .tab--active {
    color: var(--color-text-primary);
    background:
      linear-gradient(
        180deg,
        color-mix(in srgb, var(--color-accent) 16%, transparent),
        transparent
      ),
      color-mix(in srgb, var(--color-surface-elevated) 92%, var(--color-accent));
    border-color: color-mix(in srgb, var(--color-accent) 46%, var(--color-border));
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.06);
  }

  .tab--active::after {
    background: var(--color-accent);
  }

  .tab--selected {
    border-color: color-mix(in srgb, var(--color-warning) 62%, var(--color-border));
  }

  .tab--selected::before {
    position: absolute;
    inset: 4px auto auto 8px;
    width: 6px;
    height: 6px;
    content: '';
    background: var(--color-warning);
    border-radius: 999px;
    box-shadow: 0 0 0 2px var(--color-surface-elevated);
  }

  .tab__icon,
  .tab__close,
  .new-tab-button {
    display: grid;
    place-items: center;
  }

  .tab__icon {
    position: relative;
    width: 20px;
    height: 20px;
    min-width: 20px;
    color: var(--color-text-secondary);
  }

  .tab__favicon {
    width: 16px;
    height: 16px;
    object-fit: contain;
    border-radius: 3px;
  }

  .tab__spinner {
    position: absolute;
    right: -2px;
    bottom: -2px;
    display: grid;
    width: 13px;
    height: 13px;
    place-items: center;
    color: var(--color-accent);
    background: var(--color-surface-elevated);
    border-radius: 999px;
    animation: tab-spin 820ms linear infinite;
  }

  .tab__title {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tab__close {
    width: 24px;
    height: 24px;
    min-width: 24px;
    padding: 0;
    color: var(--color-text-secondary);
    background: transparent;
    border: 0;
    border-radius: 6px;
    opacity: 0.72;
    cursor: default;
  }

  .tab__close:hover,
  .new-tab-button:hover {
    color: var(--color-text-primary);
    background: rgb(255 255 255 / 0.085);
    opacity: 1;
  }

  .new-tab-button {
    flex: 0 0 32px;
    width: 32px;
    height: 30px;
    min-width: 32px;
    padding: 0;
    color: var(--color-text-secondary);
    background: transparent;
    border: 0;
    border-radius: 7px;
    cursor: default;
  }

  .tab-menu {
    position: fixed;
    z-index: 900;
    display: grid;
    gap: 2px;
    width: 196px;
    padding: 6px;
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-surface-elevated) 96%, transparent);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    box-shadow: var(--shadow-elevated);
    backdrop-filter: blur(18px);
  }

  .tab-menu__item {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    width: 100%;
    height: 32px;
    padding: 0 8px;
    color: var(--color-text-primary);
    text-align: left;
    background: transparent;
    border: 0;
    border-radius: 6px;
    cursor: default;
  }

  .tab-menu__item span {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tab-menu__item:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
  }

  .tab-menu__item:disabled {
    color: color-mix(in srgb, var(--color-text-secondary) 56%, transparent);
  }

  :global(.app-shell[data-theme='light']) .tab-bar {
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.86), transparent),
      color-mix(in srgb, var(--color-surface) 92%, var(--color-background));
  }

  :global(.app-shell[data-theme='light']) .tab:hover {
    background: rgb(0 0 0 / 0.045);
  }

  :global(.app-shell[data-theme='light']) .tab--active {
    background:
      linear-gradient(
        180deg,
        color-mix(in srgb, var(--color-accent) 10%, transparent),
        transparent
      ),
      var(--color-surface);
  }

  :global(.app-shell[data-theme='light']) .tab__close:hover,
  :global(.app-shell[data-theme='light']) .new-tab-button:hover {
    background: rgb(0 0 0 / 0.065);
  }

  @keyframes tab-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
