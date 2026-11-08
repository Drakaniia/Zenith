<script lang="ts">
  import Toggle from '$shared/ui/Toggle.svelte';
  import { settings, type Settings } from '../settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { appInfo } from '$features/window/window.svelte';
  import { updates, updateLine } from '$features/updates/updates.svelte';
  import { timeAgo } from '$shared/format';

  const s = $derived(settings.value);
  const info = $derived(appInfo.info);

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }
</script>

<!-- About -->
<section id="settings-about">
  <h2>About</h2>
  <div class="group about">
    <div class="row">
      <div class="text"><b>Zenith {info?.version ?? ''}</b><span>A lightweight Zen-style browser for Windows, built on Tauri and Microsoft Edge WebView2.</span></div>
    </div>
    <div class="row">
      <div class="text">
        <b>Updates</b>
        <span>{updateLine(updates.status, timeAgo)}</span>
      </div>
      {#if updates.ready}
        <button class="btn primary" disabled={updates.restarting} onclick={() => void updates.restart()}>Restart to update</button>
      {:else}
        <button
          class="btn"
          disabled={updates.status.state === 'checking' || updates.status.state === 'downloading'}
          onclick={() => void updates.check().catch((e) => ui.toast(String(e), { icon: 'alert' }))}>Check for updates</button
        >
      {/if}
    </div>
    <div class="row">
      <div class="text">
        <b>Update automatically</b>
        <span>Downloads new versions in the background and installs them silently when you close Zenith. Your tabs are restored.</span>
      </div>
      <Toggle checked={s.autoUpdate} onchange={(v) => set('autoUpdate', v)} label="Update automatically" />
    </div>
    <div class="kv"><span>WebView2 runtime</span><span>{info?.webviewVersion ?? '…'}</span></div>
    <div class="kv"><span>Profile folder</span><span class="ellipsis">{info?.dataDir ?? '…'}</span></div>
    <div class="kv"><span>Telemetry</span><span>None. Zenith sends no usage data.</span></div>
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
  .kv {
    display: flex;
    justify-content: space-between;
    gap: 20px;
    padding: 10px 0;
    border-top: 1px solid var(--line);
    font-size: 12.5px;
  }
  .kv span:first-child {
    color: var(--text-2);
  }
</style>
