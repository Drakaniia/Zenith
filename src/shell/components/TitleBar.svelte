<script lang="ts">
  import { onMount } from 'svelte';
  import { Maximize2, Minimize2, Minus, X } from 'lucide-svelte';
  import TabBar from '../../features/tabs/components/TabBar.svelte';
  import { toastsState } from '../state/toasts.svelte';
  import {
    closeWindow as closeAppWindow,
    getWindowState,
    maximizeWindow as maximizeAppWindow,
    minimizeWindow as minimizeAppWindow,
    restoreWindow as restoreAppWindow
  } from '../../bridge/commands/windows';
  import { isTauriRuntime } from '../../bridge/runtime';
  import type { WindowState } from '../../shared/types/backend';

  const controlsSide = /Mac|iPhone|iPad|iPod/.test(navigator.userAgent) ? 'left' : 'right';
  const nativeRuntime = isTauriRuntime();

  let isMaximized = $state(false);

  onMount(() => {
    void refreshWindowState();
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

  function showWindowError(action: string, error: unknown): void {
    const message = error instanceof Error ? error.message : String(error);
    toastsState.error(`${action} window failed: ${message}`);
  }

  function showUnavailableWindowApi(action: string): void {
    toastsState.error(`${action} window failed: Tauri window API is unavailable.`);
  }
</script>

<svelte:window onresize={refreshWindowStateFromEvent} onfocus={refreshWindowStateFromEvent} />

<header class="title-bar" data-controls-side={controlsSide}>
  {#if controlsSide === 'left'}
    <div class="window-controls" aria-label="Window controls">
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
    </div>
  {/if}

  <div class="tab-region">
    <TabBar embedded />
  </div>

  {#if controlsSide === 'right'}
    <div class="window-controls" aria-label="Window controls">
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
