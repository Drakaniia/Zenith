<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import { settings, type Settings } from '$features/settings/settings.svelte';
  import { downloadsApi } from './api';
  import { shell } from '$platform/shell';
  import { appInfo } from '$features/window/window.svelte';

  const s = $derived(settings.value);
  const info = $derived(appInfo.info);
  /** FFmpeg the media downloader would use (undefined while checking). */
  let ffmpeg = $state<string | null | undefined>(undefined);

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }

  async function pickDownloadDir() {
    const dir = await shell.pickFolder('Choose a download folder');
    if (dir) set('downloadDir', dir);
  }

  async function refreshTools() {
    ffmpeg = (await downloadsApi.mediaTools().catch(() => ({ ffmpeg: null }))).ffmpeg;
  }

  function setFfmpeg<K extends 'useFfmpeg' | 'ffmpegPath'>(key: K, value: Settings[K]) {
    set(key, value);
    // The backend sees the new value once the settings push lands.
    setTimeout(() => void refreshTools(), 400);
  }

  async function pickFfmpeg() {
    const path = await downloadsApi.mediaPickFfmpeg();
    if (path) setFfmpeg('ffmpegPath', path);
  }

  onMount(() => {
    void refreshTools();
  });
</script>

<!-- Downloads -->
<section id="settings-downloads">
  <h2>Downloads</h2>
  <div class="group">
    <div class="row">
      <div class="text"><b>Save files to</b><span class="ellipsis">{s.downloadDir ?? info?.defaultDownloadDir ?? 'Downloads'}</span></div>
      <div class="inline">
        {#if s.downloadDir}<button class="btn ghost" onclick={() => set('downloadDir', null)}>Reset</button>{/if}
        <button class="btn" onclick={pickDownloadDir}><Icon name="folder" size={15} />Change</button>
      </div>
    </div>
  </div>
  <h3>Video downloader</h3>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>Show a download button on videos</b>
        <span>Appears when you point at a video in a page. Applies to pages loaded after you change it. Videos can also be saved from the address bar, the page menu or by right-clicking a video.</span>
      </div>
      <Toggle checked={s.mediaButton} onchange={(v) => set('mediaButton', v)} label="Download button on videos" />
    </div>
    <div class="row">
      <div class="text">
        <b>Use FFmpeg when it is installed</b>
        <span
          >{#if ffmpeg === undefined}Checking…{:else if ffmpeg}Found: {ffmpeg}{:else}Not found. Streams are still saved; FFmpeg only adds .ts to .mp4 conversion and merging of audio and video that Zenith can't merge itself.{/if}</span
        >
      </div>
      <Toggle checked={s.useFfmpeg} onchange={(v) => setFfmpeg('useFfmpeg', v)} label="Use FFmpeg" />
    </div>
    <div class="row">
      <div class="text"><b>FFmpeg location</b><span class="ellipsis">{s.ffmpegPath ?? 'Found automatically (PATH)'}</span></div>
      <div class="inline">
        {#if s.ffmpegPath}<button class="btn ghost" onclick={() => setFfmpeg('ffmpegPath', null)}>Reset</button>{/if}
        <button class="btn" onclick={pickFfmpeg}><Icon name="folder" size={15} />Choose ffmpeg.exe</button>
      </div>
    </div>
    <p class="note">DRM-protected video (Netflix, Disney+ and similar) is never downloaded.</p>
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
  .inline {
    display: flex;
    gap: 6px;
  }
</style>
