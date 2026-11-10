<!-- One file in the downloads popup, laid out like Firefox's and Zen's downloads panel: the file's
     type, its name, a status line with speed and time left, and one main action on the right
     (cancel, resume, retry or show in folder). Everything else is in its right-click menu. -->
<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { shell } from '$platform/shell';
  import { timeAgo } from '$shared/format';
  import { copyText } from '$shared/clipboard';
  import type { MenuItem } from '$shared/ui/menu';
  import type { IconName } from '$shared/ui/icons';
  import { downloadsApi } from './api';
  import { downloads } from './downloads.svelte';
  import { fileType } from './file-type';
  import { fileNameOf } from './recent';
  import { panelStatus } from './rate';
  import { canDownloadAgain } from './media';
  import type { DownloadItem } from './types';

  let { d }: { d: DownloadItem } = $props();

  const name = $derived(fileNameOf(d));
  const type = $derived(fileType(name, d.mime));
  const running = $derived(d.state === 'progress');
  const failed = $derived(d.state === 'interrupted');
  const done = $derived(d.state === 'completed');
  const pct = $derived(d.total > 0 ? Math.min(100, (d.received / d.total) * 100) : 0);
  const status = $derived(panelStatus(d, downloads.rates[d.id] ?? 0, timeAgo));
  const webUrl = $derived(/^https?:\/\//i.test(d.url));
  const again = $derived(canDownloadAgain(d));

  async function act(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      ui.toast(String(e), { icon: 'alert' });
    }
  }
  const action = (a: 'pause' | 'resume' | 'cancel') => act(() => downloadsApi.downloadAction(d.id, a));
  const open = () => act(() => shell.openPath(d.path));
  const reveal = () => act(() => shell.showInFolder(d.path));
  const remove = () => act(() => downloads.remove(d.id));
  const downloadAgain = () => act(() => downloads.again(d.id));

  /** The one button on the right, as in Firefox: what you most likely want next. */
  const main = $derived.by((): { icon: IconName; title: string; run: () => void } => {
    if (running && d.paused) return { icon: 'play', title: 'Resume', run: () => action('resume') };
    if (running) return { icon: 'close', title: 'Cancel', run: () => action('cancel') };
    if (failed && d.canResume) return { icon: 'reload', title: 'Retry', run: () => action('resume') };
    if (again) return { icon: 'reload', title: 'Download again', run: downloadAgain };
    if (done) return { icon: 'folder', title: 'Show in folder', run: reveal };
    return { icon: 'trash', title: 'Remove from history', run: remove };
  });

  function menu(e: MouseEvent) {
    e.preventDefault();
    const items: MenuItem[] = [];
    if (done) {
      items.push({ label: 'Open', icon: 'external', run: open });
      items.push({ label: 'Show in folder', icon: 'folder', run: reveal });
    }
    if (running && !d.paused) items.push({ label: 'Pause', icon: 'pause', run: () => action('pause') });
    if (running && d.paused) items.push({ label: 'Resume', icon: 'play', run: () => action('resume') });
    if (failed && d.canResume) items.push({ label: 'Retry', icon: 'reload', run: () => action('resume') });
    if (again) items.push({ label: 'Download again', icon: 'reload', run: downloadAgain });
    if (running) items.push({ label: 'Cancel', icon: 'close', run: () => action('cancel') });
    if (webUrl) {
      if (items.length) items.push({ separator: true });
      items.push({ label: 'Copy download link', icon: 'link', run: () => void copyText(d.url) });
    }
    if (!running) {
      items.push({ separator: true });
      items.push({ label: 'Remove from history', icon: 'trash', run: remove });
    }
    items.push({ label: 'Clear finished downloads', icon: 'eraser', run: () => act(() => downloads.remove(null)) });
    ui.openMenu(e.clientX, e.clientY, items);
  }
</script>

<div class="item" class:failed class:done role="listitem" oncontextmenu={menu}>
  <button class="main" title={d.path || d.url} disabled={!done} onclick={open}>
    <span class="ficon" style:color={failed ? 'var(--danger)' : type.color}>
      <Icon name={failed ? 'alert' : type.icon} size={20} />
      {#if type.label && !failed}
        <span class="ext" style:background={type.color ?? 'var(--text-3)'}>{type.label}</span>
      {/if}
    </span>
    <span class="meta">
      <span class="name ellipsis">{name}</span>
      <span class="status ellipsis">
        {#if d.fast && running}<span class="fast" title="Fast download: several connections at once"><Icon name="zap" size={11} /></span>{/if}{status}
      </span>
      {#if running}
        <span class="bar" class:indeterminate={d.total <= 0} class:paused={d.paused}><span style:width="{pct}%"></span></span>
      {/if}
    </span>
  </button>
  <button class="act" title={main.title} aria-label={main.title} onclick={main.run}><Icon name={main.icon} size={15} /></button>
</div>

<style>
  .item {
    position: relative;
    display: flex;
    align-items: center;
    border-radius: var(--radius);
    transition: background 0.12s;
  }
  .item:hover {
    background: var(--hover);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 6px 8px 10px;
    text-align: left;
    border-radius: var(--radius);
  }
  .main:disabled {
    opacity: 1;
    cursor: default;
  }
  .ficon {
    position: relative;
    flex: none;
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  .ext {
    position: absolute;
    left: 50%;
    bottom: -1px;
    transform: translateX(-50%);
    padding: 0 3px;
    border-radius: 3px;
    color: #fff;
    font: 700 8px/11px var(--font);
    letter-spacing: 0.02em;
    white-space: nowrap;
  }
  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .name {
    font-size: 13px;
    font-weight: 500;
  }
  .status {
    font-size: 11.5px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .failed .status {
    color: var(--danger);
  }
  .fast {
    display: inline-flex;
    vertical-align: -1px;
    margin-right: 3px;
    color: var(--accent);
  }
  .bar {
    height: 4px;
    border-radius: 4px;
    background: var(--line);
    overflow: hidden;
    margin-top: 2px;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
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
  /* Firefox's split button: the action sits in its own cell, divided from the file. */
  .act {
    flex: none;
    align-self: stretch;
    width: 40px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    border-radius: 0 var(--radius) var(--radius) 0;
    border-left: 1px solid transparent;
    transition: background 0.12s, color 0.12s;
  }
  .item:hover .act {
    border-left-color: var(--line);
  }
  .act:hover {
    background: var(--hover);
    color: var(--text);
  }
  @media (prefers-reduced-motion: reduce) {
    .bar.indeterminate span {
      animation: none;
    }
  }
</style>
