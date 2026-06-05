<script lang="ts">
  import {
    AlertTriangle,
    Ban,
    CheckCircle2,
    Download,
    LoaderCircle,
    Trash2,
    X
  } from 'lucide-svelte';
  import { listenDownloadProgress } from '../../../bridge/events/downloads';
  import { isTauriRuntime } from '../../../bridge/runtime';
  import type { DownloadItem } from '../../../shared/types/backend';
  import { displayUrl } from '../../../shared/utils/url';
  import { toastsState } from '../../../shell/state/toasts.svelte';
  import { downloadsState } from '../state/downloads.svelte';

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  const nativeRuntime = isTauriRuntime();

  let wasOpen = false;
  let cancellingId = $state<string | null>(null);
  let isClearing = $state(false);

  let countLabel = $derived(formatCount(downloadsState.items.length, 'download'));
  let canClearCompleted = $derived(downloadsState.completedDownloads.length > 0 && !isClearing);

  $effect(() => {
    if (!nativeRuntime) {
      return;
    }

    let cancelled = false;
    let unlisten: (() => void) | null = null;

    void (async () => {
      try {
        unlisten = await listenDownloadProgress((progress) => {
          downloadsState.applyProgress(progress);
        });

        if (cancelled) {
          unlisten();
        }
      } catch (error) {
        toastsState.error(`Failed to listen for downloads: ${errorMessage(error)}`);
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  $effect(() => {
    if (open && !wasOpen) {
      void downloadsState.load();
    }

    wasOpen = open;
  });

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (open && event.key === 'Escape') {
      onClose();
    }
  }

  async function cancelDownload(id: string): Promise<void> {
    cancellingId = id;

    try {
      await downloadsState.cancel(id);
    } finally {
      cancellingId = null;
    }
  }

  async function clearCompleted(): Promise<void> {
    isClearing = true;

    try {
      await downloadsState.clearCompleted();
    } finally {
      isClearing = false;
    }
  }

  function canCancel(item: DownloadItem): boolean {
    return item.status === 'downloading' || item.status === 'paused';
  }

  function progressPercent(item: DownloadItem): number | null {
    if (!item.total_bytes || item.total_bytes <= 0) {
      return null;
    }

    return Math.min(100, Math.max(0, (item.downloaded_bytes / item.total_bytes) * 100));
  }

  function progressText(item: DownloadItem): string {
    if (!item.total_bytes || item.total_bytes <= 0) {
      return formatBytes(item.downloaded_bytes);
    }

    return `${formatBytes(item.downloaded_bytes)} / ${formatBytes(item.total_bytes)}`;
  }

  function statusLabel(status: string): string {
    if (!status) {
      return 'Unknown';
    }

    return `${status[0].toUpperCase()}${status.slice(1)}`;
  }

  function formatBytes(value: number): string {
    if (!Number.isFinite(value) || value <= 0) {
      return '0 B';
    }

    const units = ['B', 'KB', 'MB', 'GB'];
    let nextValue = value;
    let unitIndex = 0;

    while (nextValue >= 1024 && unitIndex < units.length - 1) {
      nextValue /= 1024;
      unitIndex += 1;
    }

    const digits = nextValue >= 10 || unitIndex === 0 ? 0 : 1;
    return `${nextValue.toFixed(digits)} ${units[unitIndex]}`;
  }

  function formatCount(count: number, singular: string): string {
    return `${count} ${count === 1 ? singular : `${singular}s`}`;
  }

  function errorMessage(error: unknown): string {
    if (error instanceof Error) {
      return error.message;
    }

    if (typeof error === 'string') {
      return error;
    }

    return 'Unknown error';
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

{#if open}
  <aside class="download-manager" aria-label="Downloads">
    <header class="panel-header">
      <div class="panel-heading">
        <Download size={18} strokeWidth={2.25} />
        <div class="panel-heading__text">
          <h2>Downloads</h2>
          <span>{countLabel}</span>
        </div>
      </div>

      <button
        class="icon-button"
        type="button"
        aria-label="Close downloads"
        title="Close"
        onclick={onClose}
      >
        <X size={18} strokeWidth={2.35} />
      </button>
    </header>

    <div class="panel-tools">
      <button
        class="clear-button"
        type="button"
        disabled={!canClearCompleted}
        onclick={clearCompleted}
      >
        {#if isClearing}
          <LoaderCircle size={16} strokeWidth={2.35} />
        {:else}
          <Trash2 size={16} strokeWidth={2.3} />
        {/if}
        <span>Clear Completed</span>
      </button>
    </div>

    <div class="panel-body">
      {#if downloadsState.isLoading && downloadsState.items.length === 0}
        <div class="empty-state" role="status">Loading downloads</div>
      {:else if downloadsState.items.length === 0}
        <div class="empty-state" role="status">No downloads yet</div>
      {:else}
        <div class="download-list" role="list">
          {#each downloadsState.items as item (item.id)}
            {@const percent = progressPercent(item)}
            <article class={`download-row download-row--${item.status}`} role="listitem">
              <div class="download-status" aria-hidden="true">
                {#if item.status === 'completed'}
                  <CheckCircle2 size={17} strokeWidth={2.35} />
                {:else if item.status === 'failed' || item.status === 'cancelled'}
                  <AlertTriangle size={17} strokeWidth={2.35} />
                {:else}
                  <LoaderCircle size={17} strokeWidth={2.35} />
                {/if}
              </div>

              <div class="download-main">
                <div class="download-title-row">
                  <span class="download-title" title={item.filename}>{item.filename}</span>
                  <span class="download-badge">{statusLabel(item.status)}</span>
                </div>

                <span class="download-url" title={item.url}>{displayUrl(item.url)}</span>

                <div
                  class={`progress ${percent === null ? 'progress--indeterminate' : ''}`}
                  role="progressbar"
                  aria-label={`${item.filename} download progress`}
                  aria-valuemin="0"
                  aria-valuemax="100"
                  aria-valuenow={percent === null ? undefined : Math.round(percent)}
                >
                  <span
                    class="progress__bar"
                    style={percent === null ? undefined : `width: ${percent}%;`}
                  ></span>
                </div>

                <span class="download-meta">{progressText(item)}</span>
              </div>

              <button
                class="icon-button row-action"
                type="button"
                aria-label={`Cancel ${item.filename}`}
                title="Cancel"
                disabled={!canCancel(item) || cancellingId === item.id}
                onclick={() => cancelDownload(item.id)}
              >
                {#if cancellingId === item.id}
                  <LoaderCircle size={16} strokeWidth={2.35} />
                {:else}
                  <Ban size={16} strokeWidth={2.25} />
                {/if}
              </button>
            </article>
          {/each}
        </div>
      {/if}
    </div>
  </aside>
{/if}

<style>
  .download-manager {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    min-width: 0;
    height: 100%;
    color: var(--color-text-primary);
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.035), transparent 180px),
      color-mix(in srgb, var(--color-surface) 94%, var(--color-background));
    border-left: 1px solid var(--color-border);
    box-shadow: -16px 0 42px rgb(0 0 0 / 0.18);
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-width: 0;
    min-height: 60px;
    padding: 12px 12px 10px;
    border-bottom: 1px solid var(--color-border);
  }

  .panel-heading {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    min-width: 0;
  }

  .panel-heading :global(svg) {
    color: var(--color-accent);
  }

  .panel-heading__text {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  .panel-heading h2 {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 14px;
    font-weight: 720;
    letter-spacing: 0;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .panel-heading span,
  .download-url,
  .download-meta {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    font-size: 11px;
    font-weight: 620;
    letter-spacing: 0;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .panel-tools {
    display: flex;
    justify-content: flex-end;
    min-width: 0;
    padding: 10px 12px;
    border-bottom: 1px solid color-mix(in srgb, var(--color-border) 72%, transparent);
  }

  .clear-button {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr);
    gap: 7px;
    align-items: center;
    width: 154px;
    height: 34px;
    min-width: 0;
    padding: 0 10px;
    color: var(--color-text-primary);
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--color-border) 86%, transparent);
    border-radius: 8px;
    cursor: default;
  }

  .clear-button span {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 670;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .clear-button:hover:not(:disabled) {
    color: var(--color-danger);
    background: color-mix(in srgb, var(--color-danger) 11%, transparent);
    border-color: color-mix(in srgb, var(--color-danger) 38%, var(--color-border));
  }

  .panel-body {
    min-height: 0;
    overflow: hidden;
  }

  .download-list {
    display: grid;
    gap: 6px;
    height: 100%;
    padding: 8px;
    overflow-y: auto;
  }

  .download-row {
    display: grid;
    grid-template-columns: 30px minmax(0, 1fr) 30px;
    gap: 8px;
    align-items: start;
    min-width: 0;
    padding: 9px 6px;
    border-radius: 8px;
  }

  .download-row:hover {
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  }

  .download-status {
    display: grid;
    width: 30px;
    height: 30px;
    place-items: center;
    color: var(--color-accent);
  }

  .download-row--completed .download-status {
    color: var(--color-success);
  }

  .download-row--failed .download-status,
  .download-row--cancelled .download-status {
    color: var(--color-warning);
  }

  .download-row--downloading .download-status :global(svg),
  .row-action:disabled :global(svg),
  .clear-button:disabled :global(svg) {
    animation: spin 820ms linear infinite;
  }

  .download-main {
    display: grid;
    gap: 5px;
    min-width: 0;
  }

  .download-title-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 8px;
    align-items: center;
    min-width: 0;
  }

  .download-title {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 680;
    letter-spacing: 0;
    line-height: 1.15;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .download-badge {
    max-width: 86px;
    padding: 3px 6px;
    overflow: hidden;
    color: var(--color-text-secondary);
    font-size: 10px;
    font-weight: 730;
    letter-spacing: 0;
    line-height: 1;
    text-overflow: ellipsis;
    text-transform: uppercase;
    white-space: nowrap;
    background: color-mix(in srgb, var(--color-border) 58%, transparent);
    border-radius: 999px;
  }

  .progress {
    position: relative;
    height: 5px;
    overflow: hidden;
    background: color-mix(in srgb, var(--color-border) 70%, transparent);
    border-radius: 999px;
  }

  .progress__bar {
    position: absolute;
    inset: 0 auto 0 0;
    width: 0;
    background: var(--color-accent);
    border-radius: inherit;
    transition: width 140ms ease;
  }

  .progress--indeterminate .progress__bar {
    width: 38%;
    animation: indeterminate 1.1s ease-in-out infinite;
  }

  .icon-button {
    display: grid;
    width: 30px;
    height: 30px;
    min-width: 30px;
    place-items: center;
    padding: 0;
    color: var(--color-text-secondary);
    background: transparent;
    border: 0;
    border-radius: 7px;
    cursor: default;
  }

  .icon-button:hover:not(:disabled) {
    color: var(--color-text-primary);
    background: rgb(255 255 255 / 0.085);
  }

  .row-action:hover:not(:disabled) {
    color: var(--color-danger);
  }

  .icon-button:disabled,
  .clear-button:disabled {
    color: color-mix(in srgb, var(--color-text-secondary) 48%, transparent);
  }

  .empty-state {
    display: grid;
    height: 100%;
    min-height: 160px;
    place-items: center;
    padding: 24px;
    color: var(--color-text-secondary);
    font-size: 13px;
    font-weight: 650;
    letter-spacing: 0;
    text-align: center;
  }

  :global(.app-shell[data-theme='light']) .download-manager {
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.72), transparent 180px),
      color-mix(in srgb, var(--color-surface) 96%, var(--color-background));
    box-shadow: -16px 0 42px rgb(27 31 38 / 0.1);
  }

  :global(.app-shell[data-theme='light']) .download-row:hover {
    background: color-mix(in srgb, var(--color-accent) 10%, transparent);
  }

  :global(.app-shell[data-theme='light']) .icon-button:hover:not(:disabled) {
    background: rgb(0 0 0 / 0.065);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @keyframes indeterminate {
    0% {
      transform: translateX(-110%);
    }

    100% {
      transform: translateX(270%);
    }
  }
</style>
