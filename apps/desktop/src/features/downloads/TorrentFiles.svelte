<!-- A torrent's files: choose which to download (before it starts, or later), see each file's
     progress and play video or audio while it downloads. -->
<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { overlayLease } from '$platform/layout.svelte';
  import { formatBytes } from '$shared/format';
  import { downloadsApi } from './api';
  import { downloads } from './downloads.svelte';
  import { torrents } from './torrents.svelte';
  import { isPlayable, selectionSummary } from './torrent';
  import type { TorrentFile } from './types';

  const id = $derived(torrents.pickerId);
  const item = $derived(id === null ? undefined : downloads.items.find((d) => d.id === id));
  const waiting = $derived(item?.torrent?.stage === 'choosing');
  const streamable = $derived(!!item?.torrent?.streamable);

  let files = $state<TorrentFile[]>([]);
  let chosen = $state(new Set<number>());
  let error = $state('');
  let busy = $state(false);
  let filter = $state('');
  /** The choice was edited here, so progress refreshes keep it. */
  let edited = false;

  async function load(first: boolean) {
    if (id === null) return;
    try {
      const list = await downloadsApi.torrentFiles(id);
      files = list;
      if (first || !edited) chosen = new Set(list.filter((f) => f.selected).map((f) => f.index));
      error = '';
    } catch (e) {
      if (first) error = String(e);
    }
  }

  $effect(() => {
    if (id === null) return;
    files = [];
    chosen = new Set();
    error = '';
    filter = '';
    edited = false;
    void load(true);
    // Each file's progress, while the picker is open.
    const timer = setInterval(() => void load(false), 2000);
    return () => clearInterval(timer);
  });

  // The torrent left the list (canceled, removed): nothing to choose any more.
  $effect(() => {
    if (id !== null && downloads.loaded && !item) torrents.closePicker();
  });

  const shown = $derived(filter ? files.filter((f) => f.name.toLowerCase().includes(filter.toLowerCase())) : files);
  const summary = $derived(selectionSummary(files, chosen));
  const changed = $derived(files.some((f) => f.selected !== chosen.has(f.index)));

  function toggle(index: number) {
    edited = true;
    const next = new Set(chosen);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    chosen = next;
  }

  function setAll(on: boolean) {
    edited = true;
    const next = new Set(chosen);
    for (const f of shown) {
      if (on) next.add(f.index);
      else next.delete(f.index);
    }
    chosen = next;
  }

  async function apply() {
    if (id === null || !chosen.size) return;
    busy = true;
    const ok = await torrents.choose(id, [...chosen].sort((a, b) => a - b));
    busy = false;
    if (ok) torrents.closePicker();
  }

  function pct(f: TorrentFile) {
    return f.size > 0 ? Math.min(100, Math.round((f.done / f.size) * 100)) : 0;
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (id !== null && e.key === 'Escape') torrents.closePicker();
  }}
/>

{#if id !== null}
  <div class="root" use:overlayLease>
    <div class="catcher" role="presentation" transition:fade={{ duration: 100 }} onpointerdown={() => torrents.closePicker()}></div>
    <div class="panel glass" role="dialog" aria-label="Torrent files" transition:fly={{ y: -8, duration: 160 }}>
      <header>
        <span class="badge"><Icon name="magnet" size={16} /></span>
        <div class="heading">
          <h3 class="ellipsis">{item?.fileName ?? 'Torrent'}</h3>
          <span class="sub">{waiting ? 'Choose the files to download' : summary}</span>
        </div>
        <button class="icon-btn sm" title="Close (Esc)" onclick={() => torrents.closePicker()}><Icon name="close" size={14} /></button>
      </header>

      {#if error}
        <div class="state"><Icon name="alert" size={22} /><p>{error}</p></div>
      {:else if !files.length}
        <div class="state"><span class="spinner"></span><p>Reading the file list…</p></div>
      {:else}
        {#if files.length > 8}
          <div class="filter">
            <Icon name="search" size={14} />
            <input placeholder="Filter files" bind:value={filter} spellcheck="false" />
          </div>
        {/if}
        <div class="tools">
          <button class="link" onclick={() => setAll(true)}>Select all</button>
          <button class="link" onclick={() => setAll(false)}>Select none</button>
        </div>
        <div class="list">
          {#each shown as f (f.index)}
            <label class="file" class:off={!chosen.has(f.index)}>
              <input type="checkbox" checked={chosen.has(f.index)} onchange={() => toggle(f.index)} />
              <span class="fmeta">
                <span class="fname ellipsis" title={f.name}>{f.name}</span>
                <span class="fsize">
                  {formatBytes(f.size)}{#if !waiting && f.selected && f.done > 0} · {pct(f)}%{/if}
                </span>
                {#if !waiting && f.selected && f.done < f.size}
                  <span class="bar"><span style:width="{pct(f)}%"></span></span>
                {/if}
              </span>
              {#if streamable && f.selected && isPlayable(f.name)}
                <button
                  class="icon-btn sm"
                  title={f.done >= f.size ? 'Play' : 'Play while it downloads'}
                  onclick={(e) => {
                    e.preventDefault();
                    if (id !== null) void torrents.play(id, f.index);
                  }}><Icon name="play" size={14} /></button
                >
              {/if}
            </label>
          {/each}
        </div>
      {/if}

      <footer>
        <span class="summary">{files.length ? summary : ''}</span>
        <button class="btn ghost" onclick={() => torrents.closePicker()}>{waiting ? 'Later' : 'Close'}</button>
        <button class="btn primary" disabled={busy || !chosen.size || (!waiting && !changed)} onclick={() => void apply()}>
          <Icon name="download" size={14} />{waiting ? 'Download' : 'Apply'}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 87;
  }
  .catcher {
    position: absolute;
    inset: 0;
    background: rgb(0 0 0 / 0.18);
  }
  .panel {
    position: absolute;
    left: 50%;
    top: 64px;
    transform: translateX(-50%);
    width: min(560px, calc(100vw - 24px));
    max-height: calc(100vh - 110px);
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-xl);
    padding: 12px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 2px 10px 4px;
  }
  .badge {
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 9px;
    display: grid;
    place-items: center;
    background: var(--field);
    color: var(--text-2);
  }
  .heading {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  h3 {
    margin: 0;
    font: 600 14px/1.2 var(--font-display);
  }
  .sub {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .filter {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    margin-bottom: 6px;
    padding: 0 10px;
    border-radius: var(--radius);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-3);
  }
  .filter input {
    flex: 1;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 13px;
  }
  .tools {
    display: flex;
    gap: 12px;
    padding: 0 6px 6px;
  }
  .link {
    font-size: 12px;
    color: var(--accent);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
    min-height: 0;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px;
    border-radius: var(--radius);
    cursor: pointer;
  }
  .file:hover {
    background: var(--hover);
  }
  .file.off .fname {
    color: var(--text-3);
  }
  .fmeta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .fname {
    font-size: 12.5px;
  }
  .fsize {
    font-size: 11px;
    color: var(--text-3);
  }
  .bar {
    height: 3px;
    border-radius: 3px;
    background: var(--line);
    overflow: hidden;
    margin-top: 2px;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.25s;
  }
  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 24px 18px;
    text-align: center;
    color: var(--text-3);
  }
  .state p {
    margin: 0;
  }
  .spinner {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 10px;
    margin-top: 8px;
    border-top: 1px solid var(--line);
  }
  .summary {
    flex: 1;
    font-size: 12px;
    color: var(--text-3);
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
