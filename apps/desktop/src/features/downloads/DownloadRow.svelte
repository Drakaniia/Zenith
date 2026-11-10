<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { downloadsApi } from './api';
  import { shell } from '$platform/shell';
  import { downloads } from './downloads.svelte';
  import { torrents } from './torrents.svelte';
  import { timeAgo } from '$shared/format';
  import { ui } from '$shared/ui/ui.svelte';
  import { downloadStatus } from './media';
  import type { DownloadItem } from './types';
  import type { IconName } from '$shared/ui/icons';

  let { d, compact = false }: { d: DownloadItem; compact?: boolean } = $props();

  const name = $derived(d.fileName || d.path.split(/[\\/]/).pop() || d.url);
  const pct = $derived(d.total > 0 ? Math.min(100, (d.received / d.total) * 100) : 0);
  const status = $derived(downloadStatus(d, timeAgo));
  const isTorrent = $derived(d.kind === 'torrent' || d.mime === 'application/x-bittorrent');
  const icon = $derived<IconName>(
    d.state === 'interrupted'
      ? 'alert'
      : isTorrent
        ? 'magnet'
        : d.kind !== 'media'
          ? 'file'
          : d.live
            ? 'radio'
            : d.mime.startsWith('audio/')
              ? 'music'
              : 'film',
  );
  /** Torrent extras (live torrents only; finished ones are ordinary history entries). */
  const t = $derived(d.kind === 'torrent' ? d.torrent : undefined);
  /** A torrent with a file list worth opening: several files, or one to play. */
  const hasFiles = $derived(!!t && t.files > 0 && (t.files > 1 || t.streamable));
  const indeterminate = $derived(d.total <= 0 || t?.stage === 'metadata' || t?.stage === 'choosing');

  async function act(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      ui.toast(String(e), { icon: 'alert' });
    }
  }
</script>

<div class="row" class:compact class:done={d.state === 'completed'}>
  <button class="file" title={d.path} disabled={d.state !== 'completed'} onclick={() => act(() => shell.openPath(d.path))}>
    <span class="ficon" class:bad={d.state === 'interrupted'} class:live={d.live && d.state === 'progress'} class:seeding={t?.stage === 'seeding'}
      ><Icon name={icon} size={17} /></span
    >
    <span class="meta">
      <span class="name ellipsis">{name}</span>
      <span class="status ellipsis" class:bad={d.state === 'interrupted'}>{status}</span>
      {#if d.state === 'progress'}
        <span class="bar" class:indeterminate class:paused={d.paused}><span style:width="{pct}%"></span></span>
      {/if}
    </span>
  </button>
  <div class="actions">
    {#if t?.stage === 'choosing'}
      <button class="btn primary choose" title="Choose the files to download" onclick={() => torrents.openPicker(d.id)}
        ><Icon name="listView" size={13} />Choose files</button
      >
      <button class="icon-btn sm" title="Cancel" onclick={() => act(() => downloadsApi.downloadAction(d.id, 'cancel'))}><Icon name="close" size={14} /></button>
    {:else if t?.stage === 'seeding'}
      {#if hasFiles}
        <button class="icon-btn sm" title="Files" onclick={() => torrents.openPicker(d.id)}><Icon name="listView" size={14} /></button>
      {/if}
      <button class="icon-btn sm" title="Stop seeding (the files stay)" onclick={() => act(() => downloadsApi.downloadAction(d.id, 'stop'))}
        ><Icon name="square" size={13} /></button
      >
      <button class="icon-btn sm" title="Show in folder" onclick={() => act(() => shell.showInFolder(d.path))}><Icon name="folder" size={14} /></button>
    {:else if d.state === 'progress' && d.live}
      <button class="icon-btn sm" title="Stop recording and keep the file" onclick={() => act(() => downloadsApi.downloadAction(d.id, 'stop'))}><Icon name="square" size={13} /></button>
      <button class="icon-btn sm" title="Cancel and discard" onclick={() => act(() => downloadsApi.downloadAction(d.id, 'cancel'))}><Icon name="close" size={14} /></button>
    {:else if d.state === 'progress'}
      {#if t?.stage !== 'metadata'}
        {#if d.paused}
          <button class="icon-btn sm" title="Resume" onclick={() => act(() => downloadsApi.downloadAction(d.id, 'resume'))}><Icon name="play" size={14} /></button>
        {:else}
          <button class="icon-btn sm" title="Pause" onclick={() => act(() => downloadsApi.downloadAction(d.id, 'pause'))}><Icon name="pause" size={14} /></button>
        {/if}
      {/if}
      {#if hasFiles}
        <button class="icon-btn sm" title={t?.streamable ? 'Files (choose, or play while downloading)' : 'Files'} onclick={() => torrents.openPicker(d.id)}
          ><Icon name="listView" size={14} /></button
        >
      {/if}
      <button class="icon-btn sm" title={t ? 'Cancel and delete what was downloaded' : 'Cancel'} onclick={() => act(() => downloadsApi.downloadAction(d.id, 'cancel'))}
        ><Icon name="close" size={14} /></button
      >
    {:else if d.state === 'interrupted' && d.canResume}
      <button class="icon-btn sm" title="Retry" onclick={() => act(() => downloadsApi.downloadAction(d.id, 'resume'))}><Icon name="reload" size={14} /></button>
      {#if d.live}
        <button class="icon-btn sm" title="Save what was recorded" onclick={() => act(() => downloadsApi.downloadAction(d.id, 'stop'))}><Icon name="save" size={14} /></button>
      {/if}
      <button class="icon-btn sm" title="Remove from list" onclick={() => act(() => downloads.remove(d.id))}><Icon name="trash" size={14} /></button>
    {:else}
      {#if d.state === 'completed'}
        <button class="icon-btn sm" title="Show in folder" onclick={() => act(() => shell.showInFolder(d.path))}><Icon name="folder" size={14} /></button>
      {/if}
      <button class="icon-btn sm" title="Remove from list" onclick={() => act(() => downloads.remove(d.id))}><Icon name="trash" size={14} /></button>
    {/if}
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px;
    border-radius: var(--radius);
  }
  .row:hover {
    background: var(--hover);
  }
  .file {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
  }
  .file:disabled {
    opacity: 1;
  }
  .ficon {
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: 9px;
    display: grid;
    place-items: center;
    background: var(--field);
    color: var(--text-2);
  }
  .ficon.bad,
  .status.bad,
  .ficon.live {
    color: var(--danger);
  }
  .ficon.seeding {
    color: var(--accent);
  }
  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font-size: 13px;
  }
  .status {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .bar {
    height: 3px;
    border-radius: 3px;
    background: var(--line);
    overflow: hidden;
    margin-top: 3px;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 0.25s;
  }
  .bar.paused span {
    background: var(--text-3);
  }
  .bar.indeterminate span {
    width: 35% !important;
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(300%);
    }
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
    opacity: 0.75;
  }
  .row:hover .actions {
    opacity: 1;
  }
  .choose {
    height: 26px;
    padding: 0 9px;
    font-size: 12px;
    gap: 5px;
    margin-right: 2px;
  }
</style>
