<!-- Paste-URL video downloader: probes any link (native HTTP/HLS/DASH/HTML first, optional
     yt-dlp fallback) and starts downloading video or audio directly into Zenith's downloads list. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { downloadsApi } from '../api';
  import { mediaMeta, mediaTitle } from '../media';
  import { openInternal } from '$features/browser/target';
  import { ui } from '$shared/ui/ui.svelte';
  import type { AudioFormat, MediaItem, UrlProbeResult } from '../types';

  let { onclose, compact = false }: { onclose: () => void; compact?: boolean } = $props();

  let url = $state('');
  let probing = $state(false);
  let result = $state<UrlProbeResult | null>(null);
  let error = $state('');
  let picks = $state<Record<number, { variant: string; audio: string; audioFormat: '' | AudioFormat }>>({});
  let busy = $state<Record<number, boolean>>({});
  let started = $state<Record<number, boolean>>({});
  let input: HTMLInputElement | undefined = $state();

  onMount(() => input?.focus());

  async function inspect(preferYtdlp = false) {
    const target = url.trim();
    if (!target || probing) return;
    probing = true;
    error = '';
    result = null;
    picks = {};
    started = {};
    try {
      const r = await downloadsApi.mediaProbeUrl(target, preferYtdlp);
      for (const item of r.items) {
        picks[item.id] = {
          variant: item.variants[0]?.id ?? '',
          audio: item.audioTracks[0]?.id ?? '',
          audioFormat: '',
        };
      }
      result = r;
    } catch (e) {
      error = String(e);
    } finally {
      probing = false;
    }
  }

  async function download(item: MediaItem) {
    if (!result || busy[item.id]) return;
    busy[item.id] = true;
    try {
      const p = picks[item.id];
      const fmt: AudioFormat | null = p?.audioFormat || null;
      const isAudio = !!fmt || !!item.variants.find((v) => v.id === p?.variant)?.audioOnly;
      await downloadsApi.mediaDownloadProbed(
        result.probeId,
        item.id,
        p?.variant || null,
        isAudio ? null : p?.audio || null,
        fmt,
      );
      started[item.id] = true;
    } catch (e) {
      const msg = String(e);
      ui.toast(msg, {
        icon: 'alert',
        timeout: 6500,
        action: msg.includes('Install FFmpeg')
          ? {
              label: 'Settings',
              run: () => {
                onclose();
                openInternal('settings', 'downloads');
              },
            }
          : undefined,
      });
    } finally {
      busy[item.id] = false;
    }
  }

  function openToolsSettings() {
    onclose();
    openInternal('settings', 'downloads');
  }
</script>

<div class="wrap" class:compact>
  <form
    class="bar"
    onsubmit={(e) => {
      e.preventDefault();
      void inspect(false);
    }}
  >
    <Icon name="film" size={16} />
    <input
      bind:this={input}
      bind:value={url}
      placeholder={compact ? 'Paste a video URL…' : 'Paste a video URL (MP4, HLS, DASH, YouTube, TikTok, X…)'}
      spellcheck="false"
      aria-label="Video URL"
      onkeydown={(e) => {
        if (e.key === 'Escape') onclose();
      }}
    />
    <button class="btn primary sm" type="submit" disabled={probing || !url.trim()}>
      {probing ? 'Checking…' : 'Inspect'}
    </button>
    <button class="icon-btn sm" type="button" title="Close" onclick={onclose}><Icon name="close" size={14} /></button>
  </form>

  {#if error}
    <p class="msg warn"><Icon name="alert" size={13} /> {error}</p>
  {:else if result && result.items.length}
    <div class="results">
      {#each result.items as item (item.id)}
        {@const p = picks[item.id]}
        <div class="item">
          <div class="info">
            <div class="title-row">
              <span class="name ellipsis" title={item.url}>{mediaTitle(item)}</span>
              <span class="badge">{result.engine === 'ytdlp' ? 'yt-dlp' : 'Native'}</span>
            </div>
            <div class="meta ellipsis">{mediaMeta(item)}</div>
            {#if !item.protected && p}
              <div class="choices">
                {#if item.variants.length > 1 && !p.audioFormat}
                  <select class="field" aria-label="Quality" bind:value={p.variant}>
                    {#each item.variants as v (v.id)}<option value={v.id}>{v.label}</option>{/each}
                  </select>
                {/if}
                {#if item.audioTracks.length > 1 && (!item.variants.find((v) => v.id === p.variant)?.audioOnly || p.audioFormat)}
                  <select class="field" aria-label="Audio track" bind:value={p.audio}>
                    {#each item.audioTracks as a (a.id)}<option value={a.id}>{a.label}</option>{/each}
                  </select>
                {/if}
                {#if !item.live}
                  <select class="field" aria-label="Format" bind:value={p.audioFormat}>
                    <option value="">{item.audioOnly ? 'Original audio' : 'Video'}</option>
                    <option value="m4a">Audio · M4A</option>
                    <option value="mp3">Audio · MP3</option>
                    <option value="opus">Audio · Opus</option>
                  </select>
                {/if}
              </div>
            {/if}
            {#if item.protected}
              <p class="msg warn"><Icon name="shield" size={12} /> DRM-protected: it can't be saved</p>
            {/if}
          </div>
          <button
            class="btn primary sm"
            disabled={item.protected || busy[item.id]}
            onclick={() => void download(item)}
          >
            {#if started[item.id]}
              <Icon name="check" size={14} />Started
            {:else}
              <Icon name="download" size={14} />Download
            {/if}
          </button>
        </div>
      {/each}
    </div>
  {:else if result}
    <div class="empty-hint">
      <p class="msg">{result.hint}</p>
      {#if !result.ytdlp}
        <button class="btn sm" onclick={openToolsSettings}><Icon name="download" size={13} />Install yt-dlp</button>
      {/if}
    </div>
  {:else if !compact}
    <p class="hint">Uses Zenith's built-in native extractor first, with optional yt-dlp fallback for complex sites.</p>
  {/if}
</div>

<style>
  .wrap { margin-top: 14px; }
  .wrap.compact { margin: 2px 4px 8px; }
  .bar {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 6px 5px 12px; border-radius: var(--radius-lg);
    background: var(--field); border: 1px solid var(--line); color: var(--text-3);
  }
  .bar input {
    flex: 1; min-width: 0; height: 30px; border: 0; outline: none;
    background: transparent; color: var(--text); font-size: 13px;
  }
  .btn.sm { height: 28px; padding: 0 10px; font-size: 12px; gap: 5px; flex: none; }
  .results {
    display: flex; flex-direction: column; gap: 6px; margin-top: 8px;
    padding: 8px 10px; border-radius: var(--radius-lg);
    background: var(--field); border: 1px solid var(--line);
  }
  .item { display: flex; align-items: flex-start; gap: 10px; }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .title-row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .name { font-size: 13px; font-weight: 500; }
  .badge {
    flex: none; font-size: 10.5px; padding: 1px 6px; border-radius: 999px;
    background: var(--hover); color: var(--text-2);
  }
  .meta { font-size: 11.5px; color: var(--text-3); }
  .choices { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px; }
  .choices select { height: 26px; max-width: 100%; padding: 0 8px; font-size: 11.5px; }
  .empty-hint {
    display: flex; align-items: center; justify-content: space-between; gap: 10px;
    margin-top: 8px; padding: 8px 10px; border-radius: var(--radius);
    background: var(--field); border: 1px solid var(--line);
  }
  .msg { display: flex; align-items: center; gap: 6px; margin: 6px 2px 0; font-size: 12px; color: var(--text-2); }
  .empty-hint .msg { margin: 0; }
  .msg.warn { color: var(--warn); }
  .hint { margin: 6px 4px 0; font-size: 12px; color: var(--text-3); }
</style>
