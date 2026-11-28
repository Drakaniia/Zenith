<script lang="ts">
  import { onMount } from 'svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import { settings, type ClickTrigger, type PinnedCloseBehavior, type Settings } from '../settings.svelte';
  import { formatBytes } from '$shared/format';
  import { settingsApi } from '../api';
  import type { MemoryStats } from '../types';

  const s = $derived(settings.value);
  let memory = $state<MemoryStats | null>(null);

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }

  async function refreshMemory() {
    memory = await settingsApi.memoryStats().catch(() => null);
  }

  onMount(() => {
    void refreshMemory();
    const t = setInterval(refreshMemory, 5000);
    return () => clearInterval(t);
  });

  const sleepOptions = [
    { value: 0, label: 'Never' },
    { value: 1, label: '1 minute' },
    { value: 2, label: '2 minutes' },
    { value: 5, label: '5 minutes' },
    { value: 10, label: '10 minutes' },
    { value: 15, label: '15 minutes' },
    { value: 30, label: '30 minutes' },
    { value: 60, label: '1 hour' },
  ];
  const triggerOptions: { value: ClickTrigger; label: string }[] = [
    { value: 'shift', label: 'Shift + Click' },
    { value: 'alt', label: 'Alt + Click' },
    { value: 'ctrl', label: 'Ctrl + Click' },
    { value: 'ctrl+alt', label: 'Ctrl + Alt + Click' },
    { value: 'shift+alt', label: 'Shift + Alt + Click' },
    { value: 'none', label: 'Disabled' },
  ];

  const pinnedCloseOptions: { value: PinnedCloseBehavior; label: string }[] = [
    { value: 'reset-unload-switch', label: 'Reset URL, unload & switch' },
    { value: 'unload-switch', label: 'Unload & switch' },
    { value: 'reset-switch', label: 'Reset URL & switch' },
    { value: 'close', label: 'Close tab' },
    { value: 'none', label: 'Do nothing' },
  ];
</script>

<!-- Tabs & memory -->
<section id="settings-tabs">
  <h2>Tabs & memory</h2>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>Put background tabs to sleep after</b>
        <span>Sleeping tabs free their memory and reload when you open them. Tabs playing audio, downloading or in split view stay awake.</span>
      </div>
      <select class="field select" value={s.sleepMinutes} onchange={(e) => set('sleepMinutes', Number((e.currentTarget as HTMLSelectElement).value))}>
        {#each sleepOptions as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
    </div>
    <div class="row">
      <div class="text"><b>Suspend background tabs after a minute</b><span>Pauses page scripts without unloading the page.</span></div>
      <Toggle checked={s.suspendHidden} onchange={(v) => set('suspendHidden', v)} label="Suspend background tabs" />
    </div>
    <div class="row">
      <div class="text"><b>Sleep tabs when switching spaces</b></div>
      <Toggle checked={s.sleepOnSpaceSwitch} onchange={(v) => set('sleepOnSpaceSwitch', v)} label="Sleep on space switch" />
    </div>
    <div class="row">
      <div class="text"><b>Unload tabs when Windows is low on memory</b></div>
      <Toggle checked={s.memoryPressure} onchange={(v) => set('memoryPressure', v)} label="Memory pressure" />
    </div>
    <p class="note">Unloading a tab discards anything you typed into it that the page has not saved.</p>
  </div>

  <h3>Glance, Split View & Pinned tabs</h3>
  <div class="group">
    <div class="row">
      <div class="text"><b>Open link in Glance</b><span>Click modifier for previewing links in a floating card.</span></div>
      <select class="field select" aria-label="Glance click trigger" value={s.glanceTrigger} onchange={(e) => set('glanceTrigger', (e.currentTarget as HTMLSelectElement).value as ClickTrigger)}>
        {#each triggerOptions as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
    </div>
    <div class="row">
      <div class="text"><b>Open link in Split View</b><span>Click modifier for opening links side by side with the current tab.</span></div>
      <select class="field select" aria-label="Split View click trigger" value={s.splitTrigger} onchange={(e) => set('splitTrigger', (e.currentTarget as HTMLSelectElement).value as ClickTrigger)}>
        {#each triggerOptions as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
    </div>
    <div class="row">
      <div class="text"><b>Auto-Glance external links in Essentials</b><span>Clicking a link to another site inside an Essential opens it in Glance instead of navigating away.</span></div>
      <Toggle checked={s.glanceEssentialExternal} onchange={(v) => set('glanceEssentialExternal', v)} label="Auto-Glance from Essentials" />
    </div>
    <div class="row">
      <div class="text"><b>Open external links from pinned tabs in a new tab</b><span>Keeps pinned tabs on their own site when you click outbound links.</span></div>
      <Toggle checked={s.openPinnedExternalInNewTab} onchange={(v) => set('openPinnedExternalInNewTab', v)} label="Open external links from pinned tabs in new tab" />
    </div>
    <div class="row">
      <div class="text"><b>Closing a pinned or Essential tab</b><span>What Ctrl+W or clicking close does on pinned tabs.</span></div>
      <select class="field select" aria-label="Pinned tab close behavior" value={s.pinnedCloseBehavior} onchange={(e) => set('pinnedCloseBehavior', (e.currentTarget as HTMLSelectElement).value as PinnedCloseBehavior)}>
        {#each pinnedCloseOptions as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
    </div>
    <div class="row">
      <div class="text"><b>Restore pinned tabs to their pinned URL on startup</b></div>
      <Toggle checked={s.restorePinnedUrlOnStartup} onchange={(v) => set('restorePinnedUrlOnStartup', v)} label="Restore pinned URL on startup" />
    </div>
    <div class="row">
      <div class="text"><b>Middle-click unloaded pinned tab to remove it</b></div>
      <Toggle checked={s.wheelClosePendingPinned} onchange={(v) => set('wheelClosePendingPinned', v)} label="Middle-click unloaded pinned tab removes it" />
    </div>
    <div class="row">
      <div class="text"><b>Show unread dot on Essentials</b><span>Highlights background Essentials when their page title changes.</span></div>
      <Toggle checked={s.essentialsUnreadDot} onchange={(v) => set('essentialsUnreadDot', v)} label="Essentials unread dot" />
    </div>
    <div class="row">
      <div class="text"><b>Close tab on Back when there is no history</b><span>Returns to the opener tab if you press Back at the start of a newly opened tab.</span></div>
      <Toggle checked={s.closeOnBackNoHistory} onchange={(v) => set('closeOnBackNoHistory', v)} label="Close tab on Back with no history" />
    </div>
  </div>
  <h3>Memory in use</h3>
  <div class="group" data-search-whole>
    {#if memory}
      <div class="mem-total">
        <div><b>{formatBytes(memory.privateBytes)}</b><span>private memory, all Zenith processes</span></div>
        <div><b>{memory.systemLoad}%</b><span>of system memory in use</span></div>
      </div>
      <div class="procs">
        {#each memory.processes.slice(0, 8) as p (p.pid)}
          <div class="proc"><span class="ellipsis">{p.name}</span><span class="faint">{p.pid}</span><span>{formatBytes(p.privateBytes)}</span></div>
        {/each}
      </div>
    {:else}
      <p class="note">Measuring…</p>
    {/if}
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
  .select {
    height: 32px;
    padding: 0 8px;
  }
  .note {
    margin: 10px 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .mem-total {
    display: flex;
    gap: 40px;
    padding: 14px 0 10px;
  }
  .mem-total div {
    display: flex;
    flex-direction: column;
  }
  .mem-total b {
    font: 600 20px/1.2 var(--font-display);
  }
  .mem-total span {
    font-size: 12px;
    color: var(--text-3);
  }
  .procs {
    display: flex;
    flex-direction: column;
    padding-bottom: 10px;
  }
  .proc {
    display: grid;
    grid-template-columns: 1fr 70px 90px;
    gap: 10px;
    font-size: 12px;
    padding: 3px 0;
    font-variant-numeric: tabular-nums;
  }
  .proc span:last-child {
    text-align: right;
  }
</style>
