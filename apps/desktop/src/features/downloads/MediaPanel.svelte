<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { downloadsApi } from './api';
  import { fly, fade } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { on } from '$platform/ipc';
  import { browser } from '$features/browser/browser.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { ui, type MediaAnchor } from '$shared/ui/ui.svelte';
  import { overlayLease, overlaySurface, paneRectOf } from '$platform/layout.svelte';
  import { flights } from './flights.svelte';
  import { copyText } from '$shared/clipboard';
  import { openInternal } from '$features/browser/target';
  import { kindLabel, mediaMeta, mediaTitle } from './media';
  import { displayHost } from '$shared/url';
  import type { AudioFormat, MediaItem, MediaReport } from './types';
  import type { IconName } from '$shared/ui/icons';

  const open = $derived(ui.media);
  const tab = $derived(open ? browser.tabs[open.tabId] : undefined);
  const tabHost = $derived(tab && /^https?:/i.test(tab.url) ? displayHost(tab.url).toLowerCase() : '');
  const siteHidden = $derived(
    !!tabHost && settings.value.mediaDisabledSites.some((s) => s.toLowerCase() === tabHost),
  );

  let report = $state<MediaReport | null>(null);
  let probed = $state<{ probeId: string; items: MediaItem[] } | null>(null);
  let loading = $state(false);
  let probing = $state(false);
  let error = $state('');
  let picks = $state<Record<string, { variant: string; audio: string; audioFormat: '' | AudioFormat }>>({});
  let started = $state<Record<string, boolean>>({});
  let busy = $state<Record<string, boolean>>({});
  let anchor = $state<{ left: number; top: number; inline: boolean } | null>(null);
  let listEl: HTMLDivElement | undefined = $state();
  let seq = 0;
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;

  const shownItems = $derived.by(() => {
    const out: { key: string; item: MediaItem; probeId: string | null }[] = [];
    for (const item of report?.items ?? []) out.push({ key: `n:${item.id}`, item, probeId: null });
    if (probed) {
      for (const item of probed.items) out.push({ key: `p:${probed.probeId}:${item.id}`, item, probeId: probed.probeId });
    }
    return out;
  });

  function ensurePick(key: string, item: MediaItem) {
    if (!picks[key]) {
      picks[key] = {
        variant: item.variants[0]?.id ?? '',
        audio: item.audioTracks[0]?.id ?? '',
        audioFormat: '',
      };
    }
  }

  async function load(scan: boolean) {
    const target = ui.media;
    if (!target) return;
    const mine = ++seq;
    loading = true;
    error = '';
    try {
      const r = await downloadsApi.mediaList(target.tabId, scan);
      if (mine !== seq) return;
      for (const item of r.items) ensurePick(`n:${item.id}`, item);
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

  async function inspectWithYtdlp() {
    if (!tab || !/^https?:/i.test(tab.url) || probing) return;
    probing = true;
    error = '';
    try {
      const r = await downloadsApi.mediaProbeUrl(tab.url, true);
      if (!r.items.length) {
        ui.toast(r.hint || 'No downloadable video found on this page', { icon: 'alert', timeout: 5000 });
        return;
      }
      for (const item of r.items) ensurePick(`p:${r.probeId}:${item.id}`, item);
      probed = { probeId: r.probeId, items: r.items };
    } catch (e) {
      ui.toast(String(e), { icon: 'alert', timeout: 6000 });
    } finally {
      probing = false;
    }
  }

  function placement(tabId: string, videoAnchor?: MediaAnchor | null) {
    const panelW = Math.min(410, window.innerWidth - 24);
    if (videoAnchor) {
      const pane = paneRectOf(tabId);
      if (pane) {
        const scale = videoAnchor.dpr / (window.devicePixelRatio || 1);
        const right = pane.x + (videoAnchor.x + videoAnchor.w) * scale - 12;
        const left = Math.max(8, Math.min(right - panelW, window.innerWidth - panelW - 8));
        const top = Math.max(8, Math.min(pane.y + videoAnchor.y * scale + 48, window.innerHeight - 260));
        anchor = { left, top, inline: true };
        return;
      }
    }
    const pill = document.querySelector<HTMLElement>('.sidebar:not(.floating) .pill');
    if (pill && !settings.value.compact) {
      const r = pill.getBoundingClientRect();
      anchor = { left: Math.max(8, r.left), top: r.bottom + 8, inline: false };
    } else {
      anchor = null;
    }
  }

  $effect(() => {
    const target = open;
    seq++;
    report = null;
    probed = null;
    picks = {};
    started = {};
    busy = {};
    error = '';
    if (!target) return;
    placement(target.tabId, target.anchor);
    if (!browser.tabs[target.tabId]?.live) {
      report = { items: [], blobs: 0, drm: 0, ffmpeg: false, ytdlp: false };
      error = 'This tab is asleep. Open it, play the video, then look again.';
      return;
    }
    void load(true);
  });

  onMount(() => {
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

  function iconFor(item: MediaItem, fmt: '' | AudioFormat): IconName {
    if (item.live) return 'radio';
    return item.audioOnly || !!fmt ? 'music' : 'film';
  }

  function variantIsAudio(key: string, item: MediaItem): boolean {
    if (picks[key]?.audioFormat) return true;
    const id = picks[key]?.variant;
    return !!item.variants.find((v) => v.id === id)?.audioOnly;
  }

  async function download(key: string, item: MediaItem, probeId: string | null, button: HTMLElement) {
    const target = ui.media;
    if (!target || busy[key]) return;
    busy[key] = true;
    const thumb = button.closest('.item')?.querySelector<HTMLElement>('.thumb')?.getBoundingClientRect();
    try {
      const p = picks[key];
      const fmt: AudioFormat | null = p?.audioFormat || null;
      const audioOnly = variantIsAudio(key, item);
      if (probeId) {
        await downloadsApi.mediaDownloadProbed(probeId, item.id, p?.variant || null, audioOnly ? null : p?.audio || null, fmt, target.tabId);
      } else {
        await downloadsApi.mediaDownload(target.tabId, item.id, p?.variant || null, audioOnly ? null : p?.audio || null, fmt);
      }
      started[key] = true;
      if (thumb) {
        flights.launch(
          { x: thumb.left, y: thumb.top, w: thumb.width, h: thumb.height },
          { icon: iconFor(item, fmt ?? ''), label: (fmt || (audioOnly ? '' : item.ext)).slice(0, 4).toUpperCase(), color: null },
        );
      }
    } catch (e) {
      const message = String(e);
      const already = message.startsWith('Already downloaded');
      const needsFfmpeg = message.includes('Install FFmpeg');
      ui.toast(message, {
        icon: already ? 'check' : 'alert',
        timeout: already ? 4000 : 6500,
        action: needsFfmpeg
          ? {
              label: 'Settings',
              run: () => {
                close();
                openInternal('settings', 'downloads');
              },
            }
          : undefined,
      });
    } finally {
      busy[key] = false;
    }
  }

  async function copyLink(item: MediaItem) {
    await copyText(item.url);
    ui.toast('Media address copied', { icon: 'link' });
  }

  function toggleHideOnSite() {
    if (!tabHost) return;
    const list = settings.value.mediaDisabledSites;
    if (siteHidden) {
      settings.update({ mediaDisabledSites: list.filter((s) => s.toLowerCase() !== tabHost) });
      ui.toast(`Video button enabled on ${tabHost}`, { icon: 'check' });
    } else {
      settings.update({ mediaDisabledSites: [...list, tabHost] });
      close();
      ui.toast(`Video button hidden on ${tabHost}`, {
        icon: 'eyeOff',
        timeout: 5500,
        action: {
          label: 'Undo',
          run: () =>
            settings.update({
              mediaDisabledSites: settings.value.mediaDisabledSites.filter((s) => s.toLowerCase() !== tabHost),
            }),
        },
      });
    }
  }
</script>

{#if open}
  <div class="root" use:overlayLease>
    <div class="catcher" role="presentation" transition:fade={{ duration: 100 }} onpointerdown={close}></div>
    <div
      class="panel glass"
      use:overlaySurface={10}
      class:centered={!anchor}
      class:inline={anchor?.inline}
      style:left={anchor ? `${anchor.left}px` : null}
      style:top={anchor ? `${anchor.top}px` : null}
      transition:fly={{ y: -8, duration: 160 }}
      role="dialog"
      aria-label="Videos on this page"
    >
      <header>
        <div class="heading">
          <h3>{anchor?.inline ? 'Download video' : 'Videos on this page'}</h3>
          <span class="sub ellipsis">{tabHost}</span>
        </div>
        {#if report?.ytdlp && tabHost && shownItems.length > 0 && !probed}
          <button class="btn ghost sm" title="Inspect this page with yt-dlp for more qualities" disabled={probing} onclick={() => void inspectWithYtdlp()}>
            {probing ? 'Checking…' : 'yt-dlp'}
          </button>
        {/if}
        <button class="icon-btn sm" title="Search the page again" disabled={loading} onclick={() => void load(true)}>
          <span class:spin={loading}><Icon name="reload" size={14} /></span>
        </button>
        <button class="icon-btn sm" title="Close (Esc)" onclick={close}><Icon name="close" size={14} /></button>
      </header>

      {#if (!report && loading) || (probing && !shownItems.length)}
        <div class="state"><span class="spinner"></span><p>{probing ? 'Inspecting with yt-dlp…' : 'Looking for videos…'}</p></div>
      {:else if error}
        <div class="state"><Icon name="alert" size={22} /><p>{error}</p></div>
      {:else if shownItems.length}
        <div class="list" bind:this={listEl}>
          {#each shownItems as { key, item, probeId } (key)}
            {@const p = picks[key]}
            <div class="item" class:hinted={hinted(item)} class:locked={item.protected}>
              <div class="thumb" class:live={item.live}><Icon name={iconFor(item, p?.audioFormat ?? '')} size={18} /></div>
              <div class="info">
                <div class="name ellipsis" title={item.url}>{mediaTitle(item)}</div>
                <div class="meta ellipsis" title={mediaMeta(item)}>{mediaMeta(item)}</div>
                {#if !item.protected && p}
                  <div class="choices">
                    {#if item.variants.length > 1 && !p.audioFormat}
                      <select class="field" aria-label="Quality" bind:value={p.variant}>
                        {#each item.variants as v (v.id)}<option value={v.id}>{v.label}</option>{/each}
                      </select>
                    {/if}
                    {#if item.audioTracks.length > 1 && (!variantIsAudio(key, item) || p.audioFormat)}
                      <select class="field" aria-label="Audio language" bind:value={p.audio}>
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
                  <p class="warn"><Icon name="shield" size={12} /> DRM-protected: it can't be saved</p>
                {:else if item.note}
                  <p class="note">{item.note}</p>
                {/if}
              </div>
              <div class="actions">
                <button class="icon-btn sm" title="Copy media address" onclick={() => void copyLink(item)}><Icon name="link" size={14} /></button>
                <button
                  class="btn primary go"
                  disabled={item.protected || busy[key] || (item.live && item.kind === 'dash')}
                  title={item.live ? 'Record from now until you stop it in Downloads' : `Download ${p?.audioFormat ? p.audioFormat.toUpperCase() + ' audio' : kindLabel(item).toLowerCase()}`}
                  onclick={(e) => void download(key, item, probeId, e.currentTarget)}
                >
                  {#if started[key]}
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
        {#if report && report.drm > 0}
          <p class="foot-note"><Icon name="shield" size={12} /> A player on this page uses DRM; its video can't be saved.</p>
        {/if}
      {:else}
        <div class="state">
          <Icon name="film" size={24} />
          {#if report && report.drm > 0}
            <p>This video is DRM-protected, so it can't be saved.</p>
          {:else if report && report.blobs > 0}
            <p>This site streams via Media Source. Play the video briefly, or inspect it with yt-dlp.</p>
          {:else}
            <p>No direct streams found yet. Start playing the video, or inspect the page with yt-dlp.</p>
          {/if}
          <div class="state-actions">
            {#if report?.ytdlp && tabHost}
              <button class="btn primary" disabled={probing} onclick={() => void inspectWithYtdlp()}>
                <Icon name="download" size={14} />Inspect with yt-dlp
              </button>
            {:else if tabHost && (!report || report.drm === 0)}
              <button
                class="btn primary"
                onclick={() => {
                  close();
                  openInternal('settings', 'downloads');
                }}>Install yt-dlp</button
              >
            {/if}
            <button class="btn" disabled={loading} onclick={() => void load(true)}><Icon name="reload" size={14} />Search again</button>
          </div>
        </div>
      {/if}

      <footer>
        <div class="foot-left">
          <button
            class="btn ghost sm"
            onclick={() => {
              close();
              openInternal('downloads');
            }}><Icon name="download" size={14} />Downloads</button
          >
          {#if tabHost}
            <button class="hint" title={siteHidden ? `Show the hover button on ${tabHost} again` : `Hide the floating video button on ${tabHost}`} onclick={toggleHideOnSite}>
              {siteHidden ? `Show on ${tabHost}` : `Hide on ${tabHost}`}
            </button>
          {/if}
        </div>
        {#if report && !report.ffmpeg && shownItems.some(({ item }) => item.kind !== 'file')}
          <button
            class="hint"
            title="Install FFmpeg in Settings for MP3/audio extraction, .ts conversion and stream merging."
            onclick={() => {
              close();
              openInternal('settings', 'downloads');
            }}>Install FFmpeg</button
          >
        {/if}
      </footer>
    </div>
  </div>
{/if}

<style>
  .root { position: fixed; inset: 0; z-index: 86; }
  .catcher { position: absolute; inset: 0; }
  .panel {
    position: absolute;
    width: min(430px, calc(100vw - 24px));
    max-height: calc(100vh - 110px);
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-xl);
    padding: 10px;
  }
  .panel.inline { width: min(410px, calc(100vw - 24px)); }
  .panel.centered { left: 50%; top: 56px; transform: translateX(-50%); }
  header { display: flex; align-items: center; gap: 4px; padding: 2px 2px 8px 8px; }
  .heading { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  h3 { margin: 0; font: 600 14px/1.2 var(--font-display); }
  .sub { font-size: 11.5px; color: var(--text-3); }
  .spin { display: grid; animation: spin 0.9s linear infinite; }
  .list { display: flex; flex-direction: column; gap: 2px; overflow-y: auto; min-height: 0; }
  .item { display: flex; align-items: flex-start; gap: 10px; padding: 8px 6px 8px 8px; border-radius: var(--radius); }
  .item:hover { background: var(--hover); }
  .item.hinted { background: color-mix(in srgb, var(--accent) 14%, transparent); }
  .item.locked .thumb, .item.locked .name { opacity: 0.6; }
  .thumb {
    width: 34px; height: 34px; flex: none; border-radius: 9px;
    display: grid; place-items: center; background: var(--field); color: var(--text-2);
  }
  .thumb.live { color: var(--danger); }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .name { font-size: 13px; }
  .meta { font-size: 11.5px; color: var(--text-3); }
  .choices { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px; }
  .choices select { height: 28px; max-width: 100%; padding: 0 8px; font-size: 12px; }
  .warn, .note, .foot-note { display: flex; align-items: center; gap: 5px; margin: 2px 0 0; font-size: 11.5px; }
  .warn { color: var(--warn); }
  .note { color: var(--text-3); }
  .foot-note { padding: 8px 8px 0; color: var(--text-3); }
  .actions { display: flex; align-items: center; gap: 4px; padding-top: 2px; }
  .go { height: 28px; padding: 0 10px; font-size: 12.5px; gap: 5px; }
  .state {
    display: flex; flex-direction: column; align-items: center; gap: 10px;
    padding: 22px 18px 24px; text-align: center; color: var(--text-3);
  }
  .state p { margin: 0; max-width: 320px; line-height: 1.45; }
  .state-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 8px; }
  .spinner {
    width: 22px; height: 22px; border-radius: 50%;
    border: 2px solid var(--line-strong); border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  footer {
    display: flex; align-items: center; justify-content: space-between;
    gap: 8px; padding: 8px 2px 0; margin-top: 6px; border-top: 1px solid var(--line);
  }
  .foot-left { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .btn.sm { height: 28px; padding: 0 10px; font-size: 12.5px; }
  .hint { font-size: 11.5px; color: var(--text-3); padding: 4px 6px; border-radius: var(--radius-sm); }
  .hint:hover { color: var(--text); background: var(--hover); }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
