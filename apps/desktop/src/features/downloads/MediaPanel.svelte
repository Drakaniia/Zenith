<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { downloadsApi } from './api';
  import { fly, fade } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { on } from '$platform/ipc';
  import { browser } from '$features/browser/browser.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { overlayLease } from '$platform/layout.svelte';
  import { copyText, openInternal } from '../../app/actions';
  import { kindLabel, mediaMeta, mediaTitle } from './media';
  import { displayHost } from '$shared/url';
  import type { MediaItem, MediaReport } from './types';
  import type { IconName } from '$shared/ui/icons';

  const open = $derived(ui.media);
  const tab = $derived(open ? browser.tabs[open.tabId] : undefined);
  let report = $state<MediaReport | null>(null);
  let loading = $state(false);
  let error = $state('');
  let picks = $state<Record<number, { variant: string; audio: string }>>({});
  let started = $state<Record<number, boolean>>({});
  let busy = $state<Record<number, boolean>>({});
  let anchor = $state<{ left: number; top: number } | null>(null);
  let listEl: HTMLDivElement | undefined = $state();
  let seq = 0;
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;

  async function load(scan: boolean) {
    const target = ui.media;
    if (!target) return;
    const mine = ++seq;
    loading = true;
    error = '';
    try {
      const r = await downloadsApi.mediaList(target.tabId, scan);
      if (mine !== seq) return;
      for (const item of r.items) {
        if (!picks[item.id]) picks[item.id] = { variant: item.variants[0]?.id ?? '', audio: item.audioTracks[0]?.id ?? '' };
      }
      report = r;
      if (target.hint) {
        await tick();
        listEl?.querySelector('.item.hinted')?.scrollIntoView({ block: 'nearest' });
      }
    } catch (e) {
      if (mine === seq) {
        error = String(e);
        report = null;
      }
    } finally {
      if (mine === seq) loading = false;
    }
  }

  function placement() {
    const pill = document.querySelector<HTMLElement>('.sidebar:not(.floating) .pill');
    if (pill && !settings.value.compact) {
      const r = pill.getBoundingClientRect();
      anchor = { left: Math.max(8, r.left), top: r.bottom + 8 };
    } else {
      anchor = null;
    }
  }

  // A new request (tab or hint) opens a fresh list.
  $effect(() => {
    const target = open;
    seq++;
    report = null;
    picks = {};
    started = {};
    busy = {};
    error = '';
    if (!target) return;
    placement();
    if (!browser.tabs[target.tabId]?.live) {
      report = { items: [], blobs: 0, drm: 0, ffmpeg: false };
      error = 'This tab is asleep. Open it, play the video, then look again.';
      return;
    }
    void load(true);
  });

  onMount(() => {
    // New media found while the panel is open (the player starts, a stream is read).
    const unlisten = on('media-changed', (e) => {
      if (ui.media && e.tabId === ui.media.tabId) {
        clearTimeout(refreshTimer);
        refreshTimer = setTimeout(() => void load(false), 250);
      }
    });
    return () => {
      clearTimeout(refreshTimer);
      void unlisten.then((f) => f());
    };
  });

  function close() {
    ui.media = null;
  }

  function hinted(item: MediaItem): boolean {
    const hint = open?.hint ?? '';
    return !!hint && /^https?:/i.test(hint) && item.url === hint;
  }

  function iconFor(item: MediaItem): IconName {
    if (item.live) return 'radio';
    return item.audioOnly ? 'music' : 'film';
  }

  function variantIsAudio(item: MediaItem): boolean {
    const id = picks[item.id]?.variant;
    return !!item.variants.find((v) => v.id === id)?.audioOnly;
  }

  async function download(item: MediaItem) {
    const target = ui.media;
    if (!target || busy[item.id]) return;
    busy[item.id] = true;
    try {
      const p = picks[item.id];
      await downloadsApi.mediaDownload(target.tabId, item.id, p?.variant || null, variantIsAudio(item) ? null : p?.audio || null);
      started[item.id] = true;
    } catch (e) {
      ui.toast(String(e), { icon: 'alert', timeout: 6000 });
    } finally {
      busy[item.id] = false;
    }
  }

  async function copyLink(item: MediaItem) {
    await copyText(item.url);
    ui.toast('Media address copied', { icon: 'link' });
  }
</script>

{#if open}
  <div class="root" use:overlayLease>
    <div class="catcher" role="presentation" transition:fade={{ duration: 100 }} onpointerdown={close}></div>
    <div
      class="panel glass"
      class:centered={!anchor}
      style:left={anchor ? `${anchor.left}px` : null}
      style:top={anchor ? `${anchor.top}px` : null}
      transition:fly={{ y: -8, duration: 160 }}
      role="dialog"
      aria-label="Videos on this page"
    >
      <header>
        <div class="heading">
          <h3>Videos on this page</h3>
          <span class="sub ellipsis">{tab ? displayHost(tab.url) : ''}</span>
        </div>
        <button class="icon-btn sm" title="Search the page again" disabled={loading} onclick={() => void load(true)}>
          <span class:spin={loading}><Icon name="reload" size={14} /></span>
        </button>
        <button class="icon-btn sm" title="Close (Esc)" onclick={close}><Icon name="close" size={14} /></button>
      </header>

      {#if !report && loading}
        <div class="state"><span class="spinner"></span><p>Looking for videos…</p></div>
      {:else if error}
        <div class="state"><Icon name="alert" size={22} /><p>{error}</p></div>
      {:else if report && report.items.length}
        <div class="list" bind:this={listEl}>
          {#each report.items as item (item.id)}
            <div class="item" class:hinted={hinted(item)} class:locked={item.protected}>
              <div class="thumb" class:live={item.live}><Icon name={iconFor(item)} size={18} /></div>
              <div class="info">
                <div class="name ellipsis" title={item.url}>{mediaTitle(item)}</div>
                <div class="meta ellipsis" title={mediaMeta(item)}>{mediaMeta(item)}</div>
                {#if !item.protected && (item.variants.length > 1 || item.audioTracks.length > 1) && picks[item.id]}
                  <div class="choices">
                    {#if item.variants.length > 1}
                      <select class="field" aria-label="Quality" bind:value={picks[item.id].variant}>
                        {#each item.variants as v (v.id)}<option value={v.id}>{v.label}</option>{/each}
                      </select>
                    {/if}
                    {#if item.audioTracks.length > 1 && !variantIsAudio(item)}
                      <select class="field" aria-label="Audio language" bind:value={picks[item.id].audio}>
                        {#each item.audioTracks as a (a.id)}<option value={a.id}>{a.label}</option>{/each}
                      </select>
                    {/if}
                  </div>
                {/if}
                {#if item.protected}
                  <p class="warn"><Icon name="shield" size={12} /> DRM-protected: it can't be saved</p>
                {:else if item.note}
                  <p class="note">{item.note}</p>
                {/if}
              </div>
              <div class="actions">
                <button class="icon-btn sm" title="Copy media address" onclick={() => void copyLink(item)}><Icon name="link" size={14} /></button>
                <button
                  class="btn primary go"
                  disabled={item.protected || busy[item.id] || (item.live && item.kind === 'dash')}
                  title={item.live ? 'Record from now until you stop it in Downloads' : `Download ${kindLabel(item).toLowerCase()}`}
                  onclick={() => void download(item)}
                >
                  {#if started[item.id]}
                    <Icon name="check" size={14} />{item.live ? 'Recording' : 'Started'}
                  {:else if item.live}
                    <Icon name="dot" size={14} />Record
                  {:else}
                    <Icon name="download" size={14} />Download
                  {/if}
                </button>
              </div>
            </div>
          {/each}
        </div>
        {#if report.drm > 0}
          <p class="foot-note"><Icon name="shield" size={12} /> A player on this page uses DRM; its video can't be saved.</p>
        {/if}
      {:else}
        <div class="state">
          <Icon name="film" size={24} />
          {#if report && report.drm > 0}
            <p>This video is DRM-protected, so it can't be saved.</p>
          {:else if report && report.blobs > 0}
            <p>Nothing to save yet. Play the video for a few seconds, then search again. Some sites stream in a way that can't be saved.</p>
          {:else}
            <p>No videos found yet. Start playing the video, then search again.</p>
          {/if}
          <button class="btn" disabled={loading} onclick={() => void load(true)}><Icon name="reload" size={14} />Search again</button>
        </div>
      {/if}

      <footer>
        <button
          class="btn ghost sm"
          onclick={() => {
            close();
            openInternal('downloads');
          }}><Icon name="download" size={14} />Downloads</button
        >
        {#if report && !report.ffmpeg && report.items.some((i) => i.kind !== 'file')}
          <button
            class="hint"
            title="Streams are saved as they are sent (often .ts). With FFmpeg installed, Zenith converts them to .mp4 and can merge any separate audio."
            onclick={() => {
              close();
              openInternal('settings', 'downloads');
            }}>Get .mp4 output with FFmpeg</button
          >
        {/if}
      </footer>
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 86;
  }
  .catcher {
    position: absolute;
    inset: 0;
  }
  .panel {
    position: absolute;
    width: min(430px, calc(100vw - 24px));
    max-height: calc(100vh - 110px);
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-xl);
    padding: 10px;
  }
  .panel.centered {
    left: 50%;
    top: 56px;
    transform: translateX(-50%);
  }
  header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 2px 8px 8px;
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
  .spin {
    display: grid;
    animation: spin 0.9s linear infinite;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    min-height: 0;
  }
  .item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 8px 6px 8px 8px;
    border-radius: var(--radius);
  }
  .item:hover {
    background: var(--hover);
  }
  .item.hinted {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .item.locked .thumb,
  .item.locked .name {
    opacity: 0.6;
  }
  .thumb {
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: 9px;
    display: grid;
    place-items: center;
    background: var(--field);
    color: var(--text-2);
  }
  .thumb.live {
    color: var(--danger);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .name {
    font-size: 13px;
  }
  .meta {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .choices select {
    height: 28px;
    max-width: 100%;
    padding: 0 8px;
    font-size: 12px;
  }
  .warn,
  .note,
  .foot-note {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 2px 0 0;
    font-size: 11.5px;
  }
  .warn {
    color: var(--warn);
  }
  .note {
    color: var(--text-3);
  }
  .foot-note {
    padding: 8px 8px 0;
    color: var(--text-3);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-top: 2px;
  }
  .go {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
    gap: 5px;
  }
  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 22px 18px 24px;
    text-align: center;
    color: var(--text-3);
  }
  .state p {
    margin: 0;
    max-width: 320px;
    line-height: 1.45;
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
    justify-content: space-between;
    gap: 8px;
    padding: 8px 2px 0;
    margin-top: 6px;
    border-top: 1px solid var(--line);
  }
  .btn.sm {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
  .hint {
    font-size: 11.5px;
    color: var(--text-3);
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }
  .hint:hover {
    color: var(--text);
    background: var(--hover);
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
