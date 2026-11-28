<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import { settings, type Settings } from '$features/settings/settings.svelte';
  import { downloadsApi } from './api';
  import { on } from '$platform/ipc';
  import { shell } from '$platform/shell';
  import { formatBytes } from '$shared/format';
  import { appInfo } from '$features/window/window.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { FAST_CONNECTIONS_LABEL, tapCounter } from './secret';
  import type { MediaToolName, MediaToolProgress, MediaTools } from './types';

  const s = $derived(settings.value);
  const info = $derived(appInfo.info);
  let tools = $state<MediaTools | null | undefined>(undefined);
  let installing = $state<Record<MediaToolName, boolean>>({ ffmpeg: false, ytdlp: false });
  let progress = $state<Partial<Record<MediaToolName, MediaToolProgress>>>({});

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }

  // Clicking the heading quickly reveals the hidden fast mode switch, or hides it (and turns
  // fast mode off) again.
  const secret = tapCounter();
  function tapHeading() {
    if (!secret.tap(Date.now())) return;
    const unlock = !s.fastDownloadUnlocked;
    settings.update(unlock ? { fastDownloadUnlocked: true } : { fastDownloadUnlocked: false, fastDownload: false });
    ui.toast(unlock ? 'Fast downloads unlocked' : 'Fast downloads hidden', { icon: 'zap' });
  }

  const seedOptions = [
    { value: 0, label: "Don't seed" },
    { value: 0.5, label: 'Half its size' },
    { value: 1, label: 'Its size (ratio 1)' },
    { value: 2, label: 'Twice its size' },
    { value: 5, label: 'Five times its size' },
  ];

  function setKib(key: 'torrentDownKib' | 'torrentUpKib', raw: string) {
    const n = Math.round(Number(raw));
    set(key, Number.isFinite(n) && n > 0 ? n : 0);
  }

  async function pickDownloadDir() {
    const dir = await shell.pickFolder('Choose a download folder');
    if (dir) set('downloadDir', dir);
  }

  async function refreshTools() {
    tools = await downloadsApi.mediaTools().catch(() => null);
  }

  function setTool<K extends 'useFfmpeg' | 'ffmpegPath' | 'useYtdlp' | 'ytdlpPath'>(key: K, value: Settings[K]) {
    set(key, value);
    // The backend sees the new value once the settings push lands.
    setTimeout(() => void refreshTools(), 400);
  }

  async function pickFfmpeg() {
    const path = await downloadsApi.mediaPickFfmpeg();
    if (path) setTool('ffmpegPath', path);
  }

  async function pickYtdlp() {
    const path = await downloadsApi.mediaPickYtdlp();
    if (path) setTool('ytdlpPath', path);
  }

  async function installTool(tool: MediaToolName) {
    if (installing[tool]) return;
    installing[tool] = true;
    try {
      await downloadsApi.mediaInstallTool(tool);
      if (tool === 'ffmpeg' && !s.useFfmpeg) set('useFfmpeg', true);
      if (tool === 'ytdlp' && !s.useYtdlp) set('useYtdlp', true);
      await refreshTools();
      ui.toast(`${tool === 'ffmpeg' ? 'FFmpeg' : 'yt-dlp'} installed`, { icon: 'check' });
    } catch (e) {
      ui.toast(String(e), { icon: 'alert', timeout: 6000 });
    } finally {
      installing[tool] = false;
      delete progress[tool];
    }
  }

  async function removeTool(tool: MediaToolName) {
    if (installing[tool]) return;
    try {
      await downloadsApi.mediaRemoveTool(tool);
      await refreshTools();
      ui.toast(`${tool === 'ffmpeg' ? 'FFmpeg' : 'yt-dlp'} removed`, { icon: 'check' });
    } catch (e) {
      ui.toast(String(e), { icon: 'alert', timeout: 5000 });
    }
  }

  function progressText(tool: MediaToolName): string {
    const p = progress[tool];
    if (!p || !p.stage) return 'Preparing…';
    if (p.total > 0) return `${p.stage} ${formatBytes(p.received)} / ${formatBytes(p.total)}`;
    if (p.received > 0) return `${p.stage} ${formatBytes(p.received)}`;
    return p.stage;
  }

  function unhideSite(site: string) {
    set(
      'mediaDisabledSites',
      s.mediaDisabledSites.filter((x) => x !== site),
    );
  }

  onMount(() => {
    void refreshTools();
    const unlisten = on('media-tool-progress', (p) => {
      if (p.stage) progress[p.tool] = p;
      else delete progress[p.tool];
    });
    return () => void unlisten.then((f) => f());
  });
</script>

<!-- Downloads -->
<section id="settings-downloads">
  <h2><button class="secret" tabindex="-1" onclick={tapHeading}>Downloads</button></h2>
  <div class="group">
    <div class="row">
      <div class="text"><b>Save files to</b><span class="ellipsis">{s.downloadDir ?? info?.defaultDownloadDir ?? 'Downloads'}</span></div>
      <div class="inline">
        {#if s.downloadDir}<button class="btn ghost" onclick={() => set('downloadDir', null)}>Reset</button>{/if}
        <button class="btn" onclick={pickDownloadDir}><Icon name="folder" size={15} />Change</button>
      </div>
    </div>
    {#if s.fastDownloadUnlocked}
      <div class="row">
        <div class="text">
          <b class="fast"><Icon name="zap" size={13} />Fast downloads</b>
          <span>Large files download over {FAST_CONNECTIONS_LABEL} connections at once when the server allows it, and video streams fetch more segments at a time. Downloads from incognito windows are left alone.</span>
        </div>
        <Toggle checked={s.fastDownload} onchange={(v) => set('fastDownload', v)} label="Fast downloads" />
      </div>
    {/if}
  </div>
  <h3>Video downloader</h3>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>Show a download button on videos</b>
        <span>Appears when you point at a video in a page, with an inline quality and audio picker. Videos can also be saved from the address bar, the page menu or by right-clicking a video.</span>
      </div>
      <Toggle checked={s.mediaButton} onchange={(v) => set('mediaButton', v)} label="Download button on videos" />
    </div>
    {#if s.mediaDisabledSites.length > 0}
      <div class="row">
        <div class="text">
          <b>Button hidden on sites</b>
          <div class="chips">
            {#each s.mediaDisabledSites as site (site)}
              <button class="chip" title="Show video button on {site} again" onclick={() => unhideSite(site)}>
                {site} <Icon name="close" size={11} />
              </button>
            {/each}
          </div>
        </div>
        <button class="btn ghost" onclick={() => set('mediaDisabledSites', [])}>Clear all</button>
      </div>
    {/if}
    <div class="row">
      <div class="text">
        <b>FFmpeg (audio extraction &amp; stream muxing)</b>
        <span class="ellipsis">
          {#if installing.ffmpeg}
            {progressText('ffmpeg')}
          {:else if tools === undefined}
            Checking…
          {:else if tools?.ffmpeg}
            Found: {tools.ffmpeg}
          {:else}
            Not installed. Native fMP4 merging works without it; FFmpeg adds MP3/audio extraction, .ts to .mp4 conversion and WebM/mixed-track merging.
          {/if}
        </span>
      </div>
      <div class="inline">
        {#if tools?.ffmpegManaged && !s.ffmpegPath}
          <button class="btn ghost" disabled={installing.ffmpeg} onclick={() => void removeTool('ffmpeg')}>Remove</button>
        {:else if !tools?.ffmpeg}
          <button class="btn primary" disabled={installing.ffmpeg} onclick={() => void installTool('ffmpeg')}>
            <Icon name="download" size={14} />{installing.ffmpeg ? 'Installing…' : 'Install'}
          </button>
        {/if}
        {#if s.ffmpegPath}<button class="btn ghost" onclick={() => setTool('ffmpegPath', null)}>Reset</button>{/if}
        <button class="btn" disabled={installing.ffmpeg} onclick={pickFfmpeg}><Icon name="folder" size={14} />Choose</button>
        <Toggle checked={s.useFfmpeg} onchange={(v) => setTool('useFfmpeg', v)} label="Use FFmpeg" />
      </div>
    </div>
    <div class="row">
      <div class="text">
        <b>yt-dlp (complex sites &amp; pasted URLs)</b>
        <span class="ellipsis">
          {#if installing.ytdlp}
            {progressText('ytdlp')}
          {:else if tools === undefined}
            Checking…
          {:else if tools?.ytdlp}
            Found: {tools.ytdlp}
          {:else}
            Not installed. Zenith uses its built-in native extractor first; optional yt-dlp adds fallback support for sites like YouTube, TikTok, X and Twitch.
          {/if}
        </span>
      </div>
      <div class="inline">
        {#if tools?.ytdlpManaged && !s.ytdlpPath}
          <button class="btn ghost" disabled={installing.ytdlp} onclick={() => void removeTool('ytdlp')}>Remove</button>
        {:else if !tools?.ytdlp}
          <button class="btn primary" disabled={installing.ytdlp} onclick={() => void installTool('ytdlp')}>
            <Icon name="download" size={14} />{installing.ytdlp ? 'Installing…' : 'Install'}
          </button>
        {/if}
        {#if s.ytdlpPath}<button class="btn ghost" onclick={() => setTool('ytdlpPath', null)}>Reset</button>{/if}
        <button class="btn" disabled={installing.ytdlp} onclick={pickYtdlp}><Icon name="folder" size={14} />Choose</button>
        <Toggle checked={s.useYtdlp} onchange={(v) => setTool('useYtdlp', v)} label="Use yt-dlp" />
      </div>
    </div>
    <p class="note">DRM-protected video (Netflix, Disney+ and similar) is never downloaded.</p>
  </div>
  <h3>Torrents</h3>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>Download torrents in Zenith</b>
        <span>Magnet links you click and .torrent files you download start in the downloads list, where you can also add them yourself.</span>
      </div>
      <Toggle checked={s.torrentsEnabled} onchange={(v) => set('torrentsEnabled', v)} label="Download torrents in Zenith" />
    </div>
    <div class="row">
      <div class="text"><b>Ask which files to download</b><span>When a torrent has several files. You can change the choice later from its Files button.</span></div>
      <Toggle checked={s.torrentChooseFiles} onchange={(v) => set('torrentChooseFiles', v)} label="Ask which files to download" />
    </div>
    <div class="row">
      <div class="text"><b>Seed after downloading</b><span>Keep sharing a finished torrent with others until it uploaded this much of its size.</span></div>
      <select class="field select" value={String(s.torrentSeedRatio)} onchange={(e) => set('torrentSeedRatio', Number((e.currentTarget as HTMLSelectElement).value))}>
        {#each seedOptions as o (o.value)}<option value={String(o.value)}>{o.label}</option>{/each}
      </select>
    </div>
    <div class="row">
      <div class="text"><b>Download speed limit</b><span>KB/s for all torrents together. 0 means no limit.</span></div>
      <input class="field num" type="number" min="0" step="64" value={s.torrentDownKib} onchange={(e) => setKib('torrentDownKib', e.currentTarget.value)} aria-label="Download speed limit in KB/s" />
    </div>
    <div class="row">
      <div class="text"><b>Upload speed limit</b><span>KB/s for all torrents together. 0 means no limit.</span></div>
      <input class="field num" type="number" min="0" step="64" value={s.torrentUpKib} onchange={(e) => setKib('torrentUpKib', e.currentTarget.value)} aria-label="Upload speed limit in KB/s" />
    </div>
    <p class="note">To open magnet links and .torrent files from other apps in Zenith, choose Zenith for them in Windows Settings &gt; Apps &gt; Default apps. Only download what you have the right to share.</p>
  </div>
</section>

<style>
  section {
    padding-top: 8px;
    margin-bottom: 28px;
  }
  h2 {
    margin: 0 0 12px;
    font: 600 18px/1.3 var(--font-display);
  }
  /* The heading's hidden gesture: looks and behaves like plain heading text. */
  .secret {
    font: inherit;
    color: inherit;
    cursor: default;
    user-select: none;
  }
  .fast {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  h3 {
    margin: 22px 0 10px;
    font: 600 14px/1.3 var(--font-display);
    color: var(--text-2);
  }
  .group {
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    padding: 4px 16px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    min-height: 58px;
    padding: 10px 0;
    border-bottom: 1px solid var(--line);
  }
  .row:last-child {
    border-bottom: 0;
  }
  /* Controls keep their natural width; the description column gives way instead. */
  .row > :not(.text) {
    flex: none;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .text b {
    font-weight: 500;
  }
  .text span {
    font-size: 12px;
    color: var(--text-3);
    line-height: 1.4;
  }
  .note {
    margin: 10px 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .num {
    width: 110px;
    height: 32px;
    padding: 0 10px;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    border-radius: 999px;
    background: var(--hover);
    color: var(--text-2);
    font-size: 11.5px;
  }
  .chip:hover {
    color: var(--text);
    background: var(--line-strong);
  }
</style>
