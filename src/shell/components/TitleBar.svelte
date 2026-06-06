<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Bookmark,
    Download,
    History,
    Maximize2,
    Menu,
    Minimize2,
    Minus,
    PanelTop,
    Search,
    Settings,
    X
  } from 'lucide-svelte';
  import TabBar from '../../features/tabs/components/TabBar.svelte';
  import { tabsState } from '../../features/tabs/state/tabs.svelte';
  import { toastsState } from '../state/toasts.svelte';
  import {
    closeWindow as closeAppWindow,
    getWindowState,
    maximizeWindow as maximizeAppWindow,
    minimizeWindow as minimizeAppWindow,
    restoreWindow as restoreAppWindow
  } from '../../bridge/commands/windows';
  import { isDesktopRuntime } from '../../bridge/runtime';
  import type { WindowState } from '../../shared/types/backend';
  import { BROWSER_HOME_URL } from '../../shared/utils/internal-urls';

  type ShellPanel = 'bookmarks' | 'history' | 'downloads' | 'settings' | 'find';

  interface Props {
    onOpenPanel: (panel: ShellPanel) => void;
    onOpenTab?: () => void;
    focusMenuRequest?: number;
    openMenuRequest?: number;
    onMenuOpenChange?: (open: boolean) => void;
  }

  let {
    onOpenPanel,
    onOpenTab,
    focusMenuRequest = 0,
    openMenuRequest = 0,
    onMenuOpenChange
  }: Props = $props();

  const controlsSide = /Mac|iPhone|iPad|iPod/.test(navigator.userAgent) ? 'left' : 'right';
  const nativeRuntime = isDesktopRuntime();

  let isMaximized = $state(false);
  let isMenuOpen = $state(false);
  let menuButtonElement: HTMLButtonElement | undefined = $state();
  let lastFocusMenuRequest = 0;
  let lastOpenMenuRequest = 0;
  let canOpenTab = $derived(
    Boolean(tabsState.activeTab && tabsState.activeTab.url !== BROWSER_HOME_URL)
  );

  onMount(() => {
    void refreshWindowState();
  });

  $effect(() => {
    const request = focusMenuRequest;

    if (request <= 0 || request === lastFocusMenuRequest) {
      return;
    }

    lastFocusMenuRequest = request;
    window.setTimeout(() => {
      menuButtonElement?.focus();
    }, 0);
  });

  $effect(() => {
    const request = openMenuRequest;

    if (request <= 0 || request === lastOpenMenuRequest) {
      return;
    }

    lastOpenMenuRequest = request;
    setMenuOpen(true);
    window.setTimeout(() => {
      menuButtonElement?.focus();
    }, 0);
  });

  async function refreshWindowState(): Promise<void> {
    if (!nativeRuntime) {
      isMaximized = false;
      return;
    }

    try {
      applyWindowState(await getWindowState());
    } catch {
      isMaximized = false;
    }
  }

  async function minimizeWindow(): Promise<void> {
    if (!nativeRuntime) {
      showUnavailableWindowApi('Minimize');
      return;
    }

    try {
      applyWindowState(await minimizeAppWindow());
    } catch (error) {
      showWindowError('Minimize', error);
    }
  }

  async function toggleMaximizeWindow(): Promise<void> {
    const action = isMaximized ? 'Restore' : 'Maximize';

    if (!nativeRuntime) {
      showUnavailableWindowApi(isMaximized ? 'Restore' : 'Maximize');
      return;
    }

    try {
      applyWindowState(await (isMaximized ? restoreAppWindow() : maximizeAppWindow()));
    } catch (error) {
      showWindowError(action, error);
    }
  }

  async function closeWindow(): Promise<void> {
    if (!nativeRuntime) {
      showUnavailableWindowApi('Close');
      return;
    }

    try {
      await closeAppWindow();
    } catch (error) {
      showWindowError('Close', error);
    }
  }

  function refreshWindowStateFromEvent(): void {
    void refreshWindowState();
  }

  function applyWindowState(state: WindowState): void {
    isMaximized = state.is_maximized;
  }

  function openMenuPanel(panel: ShellPanel): void {
    onOpenPanel(panel);
    setMenuOpen(false);
  }

  function openNewTab(): void {
    if (!canOpenTab) {
      return;
    }

    onOpenTab?.();
    setMenuOpen(false);
  }

  function handleWindowClick(): void {
    setMenuOpen(false);
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      setMenuOpen(false);
    }
  }

  function toggleMenuOpen(): void {
    setMenuOpen(!isMenuOpen);
  }

  function setMenuOpen(open: boolean): void {
    if (isMenuOpen === open) {
      return;
    }

    isMenuOpen = open;
    onMenuOpenChange?.(open);
  }

  function showWindowError(action: string, error: unknown): void {
    const message = error instanceof Error ? error.message : String(error);
    toastsState.error(`${action} window failed: ${message}`);
  }

  function showUnavailableWindowApi(action: string): void {
    toastsState.error(`${action} window failed: desktop window API is unavailable.`);
  }
</script>

{#snippet browserMenuControl()}
  <div class="menu-anchor" data-tauri-drag-region="false">
    <button
      bind:this={menuButtonElement}
      class="window-control browser-menu-button"
      type="button"
      aria-label="Open browser menu"
      aria-expanded={isMenuOpen}
      title="Menu"
      onclick={(event) => {
        event.stopPropagation();
        toggleMenuOpen();
      }}
    >
      <Menu size={15} strokeWidth={2.35} />
    </button>

    {#if isMenuOpen}
      <div class="browser-menu" role="menu" tabindex="-1">
        <button
          class="browser-menu__item"
          type="button"
          role="menuitem"
          disabled={!canOpenTab}
          onclick={openNewTab}
        >
          <PanelTop size={16} strokeWidth={2.25} />
          <span>Default Screen</span>
        </button>
        <button
          class="browser-menu__item"
          type="button"
          role="menuitem"
          onclick={() => openMenuPanel('bookmarks')}
        >
          <Bookmark size={16} strokeWidth={2.25} />
          <span>Bookmarks</span>
        </button>
        <button
          class="browser-menu__item"
          type="button"
          role="menuitem"
          onclick={() => openMenuPanel('history')}
        >
          <History size={16} strokeWidth={2.25} />
          <span>History</span>
        </button>
        <button
          class="browser-menu__item"
          type="button"
          role="menuitem"
          onclick={() => openMenuPanel('downloads')}
        >
          <Download size={16} strokeWidth={2.25} />
          <span>Downloads</span>
        </button>
        <button
          class="browser-menu__item"
          type="button"
          role="menuitem"
          onclick={() => openMenuPanel('settings')}
        >
          <Settings size={16} strokeWidth={2.25} />
          <span>Settings</span>
        </button>
        <button
          class="browser-menu__item"
          type="button"
          role="menuitem"
          onclick={() => openMenuPanel('find')}
        >
          <Search size={16} strokeWidth={2.25} />
          <span>Find in Page</span>
        </button>
      </div>
    {/if}
  </div>
{/snippet}

<svelte:window
  onclick={handleWindowClick}
  onkeydown={handleWindowKeydown}
  onresize={refreshWindowStateFromEvent}
  onfocus={refreshWindowStateFromEvent}
/>

<header class="title-bar" data-controls-side={controlsSide}>
  {#if controlsSide === 'left'}
    <div class="window-controls" aria-label="Browser and window controls">
      <button
        class="window-control"
        type="button"
        aria-label="Close window"
        title="Close"
        onclick={closeWindow}
      >
        <X size={14} strokeWidth={2.4} />
      </button>
      <button
        class="window-control"
        type="button"
        aria-label="Minimize window"
        title="Minimize"
        onclick={minimizeWindow}
      >
        <Minus size={14} strokeWidth={2.4} />
      </button>
      <button
        class="window-control"
        type="button"
        aria-label={isMaximized ? 'Restore window' : 'Maximize window'}
        title={isMaximized ? 'Restore' : 'Maximize'}
        onclick={toggleMaximizeWindow}
      >
        {#if isMaximized}
          <Minimize2 size={13} strokeWidth={2.35} />
        {:else}
          <Maximize2 size={13} strokeWidth={2.35} />
        {/if}
      </button>
      {@render browserMenuControl()}
    </div>
  {/if}

  <div class="tab-region">
    <TabBar embedded {onOpenTab} />
  </div>

  {#if controlsSide === 'right'}
    <div class="window-controls" aria-label="Browser and window controls">
      {@render browserMenuControl()}
      <button
        class="window-control"
        type="button"
        aria-label="Minimize window"
        title="Minimize"
        onclick={minimizeWindow}
      >
        <Minus size={14} strokeWidth={2.4} />
      </button>
      <button
        class="window-control"
        type="button"
        aria-label={isMaximized ? 'Restore window' : 'Maximize window'}
        title={isMaximized ? 'Restore' : 'Maximize'}
        onclick={toggleMaximizeWindow}
      >
        {#if isMaximized}
          <Minimize2 size={13} strokeWidth={2.35} />
        {:else}
          <Maximize2 size={13} strokeWidth={2.35} />
        {/if}
      </button>
      <button
        class="window-control window-control--close"
        type="button"
        aria-label="Close window"
        title="Close"
        onclick={closeWindow}
      >
        <X size={14} strokeWidth={2.4} />
      </button>
    </div>
  {/if}
</header>

<style>
  .title-bar {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(96px, auto);
    align-items: center;
    width: 100%;
    height: var(--shell-tabbar-height);
    min-height: var(--shell-tabbar-height);
    user-select: none;
    color: var(--color-text-primary);
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.035), transparent),
      color-mix(in srgb, var(--color-surface) 96%, var(--color-background));
    border-bottom: 1px solid var(--color-border);
  }

  .title-bar[data-controls-side='left'] .window-controls {
    grid-column: 1;
    justify-content: start;
    padding-left: 8px;
  }

  .title-bar[data-controls-side='left'] {
    grid-template-columns: minmax(96px, auto) minmax(0, 1fr);
  }

  .title-bar[data-controls-side='left'] .tab-region {
    grid-column: 2;
  }

  .title-bar[data-controls-side='right'] .tab-region {
    grid-column: 1;
  }

  .title-bar[data-controls-side='right'] .window-controls {
    grid-column: 2;
    justify-content: end;
    padding-right: 4px;
  }

  .tab-region {
    min-width: 0;
    height: 100%;
  }

  .window-controls {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 100%;
  }

  .menu-anchor {
    position: relative;
    display: grid;
    place-items: center;
  }

  .window-control {
    display: grid;
    width: 32px;
    height: 32px;
    min-width: 32px;
    min-height: 32px;
    place-items: center;
    padding: 0;
    color: var(--color-text-secondary);
    background: transparent;
    border: 0;
    border-radius: 7px;
    cursor: default;
  }

  .window-control:hover {
    color: var(--color-text-primary);
    background: rgb(255 255 255 / 0.075);
  }

  .window-control:active {
    transform: translateY(1px);
  }

  .window-control--close:hover {
    color: #ffffff;
    background: var(--color-danger);
  }

  .browser-menu {
    position: absolute;
    top: calc(100% + 7px);
    right: 0;
    z-index: 930;
    display: grid;
    gap: 2px;
    width: 204px;
    padding: 6px;
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-surface-elevated) 96%, transparent);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    box-shadow: var(--shadow-elevated);
    backdrop-filter: blur(18px);
  }

  .title-bar[data-controls-side='left'] .browser-menu {
    right: auto;
    left: 0;
  }

  .browser-menu__item {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    width: 100%;
    height: 34px;
    padding: 0 8px;
    color: var(--color-text-primary);
    text-align: left;
    background: transparent;
    border: 0;
    border-radius: 6px;
    cursor: default;
  }

  .browser-menu__item span {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .browser-menu__item:hover {
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
  }

  .browser-menu__item:disabled {
    color: color-mix(in srgb, var(--color-text-secondary) 56%, transparent);
    pointer-events: none;
  }

  :global(.app-shell[data-theme='light']) .title-bar {
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.8), transparent),
      color-mix(in srgb, var(--color-surface) 96%, var(--color-background));
  }

  :global(.app-shell[data-theme='light']) .window-control:hover {
    background: rgb(0 0 0 / 0.065);
  }

  :global(.app-shell[data-theme='light']) .window-control--close:hover {
    color: #ffffff;
    background: var(--color-danger);
  }
</style>
