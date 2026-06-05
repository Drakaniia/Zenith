<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { Maximize2, Minimize2, Minus, X } from 'lucide-svelte';
  import { toastsState } from '../state/toasts.svelte';

  interface Props {
    title: string;
  }

  let { title }: Props = $props();

  const controlsSide = /Mac|iPhone|iPad|iPod/.test(navigator.userAgent) ? 'left' : 'right';

  let isMaximized = $state(false);
  let displayTitle = $derived(title.trim() || 'Rust Browser');

  $effect(() => {
    void refreshMaximizedState();
  });

  async function refreshMaximizedState(): Promise<void> {
    const appWindow = getAppWindow();
    if (!appWindow) {
      isMaximized = false;
      return;
    }

    try {
      isMaximized = await appWindow.isMaximized();
    } catch {
      isMaximized = false;
    }
  }

  async function minimizeWindow(): Promise<void> {
    const appWindow = getAppWindow();
    if (!appWindow) {
      showUnavailableWindowApi('Minimize');
      return;
    }

    try {
      await appWindow.minimize();
    } catch (error) {
      showWindowError('Minimize', error);
    }
  }

  async function toggleMaximizeWindow(): Promise<void> {
    const appWindow = getAppWindow();
    if (!appWindow) {
      showUnavailableWindowApi(isMaximized ? 'Restore' : 'Maximize');
      return;
    }

    try {
      await appWindow.toggleMaximize();
      await refreshMaximizedState();
    } catch (error) {
      showWindowError(isMaximized ? 'Restore' : 'Maximize', error);
    }
  }

  async function closeWindow(): Promise<void> {
    const appWindow = getAppWindow();
    if (!appWindow) {
      showUnavailableWindowApi('Close');
      return;
    }

    try {
      await appWindow.close();
    } catch (error) {
      showWindowError('Close', error);
    }
  }

  function showWindowError(action: string, error: unknown): void {
    const message = error instanceof Error ? error.message : String(error);
    toastsState.error(`${action} window failed: ${message}`);
  }

  function showUnavailableWindowApi(action: string): void {
    toastsState.error(`${action} window failed: Tauri window API is unavailable.`);
  }

  function getAppWindow(): ReturnType<typeof getCurrentWindow> | null {
    try {
      return getCurrentWindow();
    } catch {
      return null;
    }
  }
</script>

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

  <div class="title-region" data-tauri-drag-region>
    <span class="app-mark" aria-hidden="true"></span>
    <span class="title-text">{displayTitle}</span>
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
    grid-template-columns: minmax(96px, auto) minmax(0, 1fr) minmax(96px, auto);
    align-items: center;
    width: 100%;
    height: 36px;
    min-height: 36px;
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

  .title-bar[data-controls-side='left'] .title-region {
    grid-column: 2 / 4;
    padding-right: 104px;
  }

  .title-bar[data-controls-side='right'] .title-region {
    grid-column: 1 / 3;
    padding-left: 104px;
  }

  .title-bar[data-controls-side='right'] .window-controls {
    grid-column: 3;
    justify-content: end;
    padding-right: 4px;
  }

  .title-region {
    display: grid;
    grid-template-columns: 12px minmax(0, auto);
    gap: 9px;
    align-items: center;
    justify-content: center;
    min-width: 0;
    height: 100%;
    padding-inline: 12px;
    cursor: default;
  }

  .app-mark {
    width: 10px;
    height: 10px;
    border: 1px solid color-mix(in srgb, var(--color-accent) 74%, white);
    border-radius: 3px;
    background:
      linear-gradient(135deg, color-mix(in srgb, var(--color-accent) 92%, white), transparent 58%),
      var(--color-surface-elevated);
    box-shadow: 0 0 16px color-mix(in srgb, var(--color-accent) 38%, transparent);
  }

  .title-text {
    min-width: 0;
    max-width: min(58vw, 680px);
    overflow: hidden;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
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
