<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '../Icon.svelte';
  import Toggle from '../Toggle.svelte';
  import { api } from '../../lib/ipc';
  import { browser } from '../../lib/browser.svelte';
  import { ui } from '../../lib/ui.svelte';
  import type { ExtensionInfo } from '../../lib/types';
  import { incognito } from '../../lib/app.svelte';

  let items = $state<ExtensionInfo[]>([]);
  const userItems = $derived(items.filter((e) => !e.builtin));
  const builtinItems = $derived(items.filter((e) => e.builtin));
  let loading = $state(true);
  let busy = $state(false);
  let storeInput = $state('');
  let error = $state('');

  async function refresh() {
    try {
      items = await api.extList();
      error = '';
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function install(fn: () => Promise<ExtensionInfo | null>) {
    busy = true;
    try {
      const ext = await fn();
      if (ext) ui.toast(`Added ${ext.name}`, { icon: 'puzzle' });
      await refresh();
    } catch (e) {
      ui.toast(String(e), { icon: 'alert', timeout: 7000 });
    } finally {
      busy = false;
    }
  }

  async function fromStore() {
    const input = storeInput.trim();
    if (!input) return;
    await install(() => api.extInstallStore(input));
    storeInput = '';
  }

  async function setEnabled(ext: ExtensionInfo, enabled: boolean) {
    try {
      await api.extSetEnabled(ext.id, enabled);
      ext.enabled = enabled;
    } catch (e) {
      ui.toast(String(e), { icon: 'alert' });
      await refresh();
    }
  }

  async function remove(ext: ExtensionInfo) {
    try {
      await api.extRemove(ext.id);
      ui.toast(`Removed ${ext.name}`, { icon: 'trash' });
    } catch (e) {
      ui.toast(String(e), { icon: 'alert' });
    }
    await refresh();
  }

  onMount(() => void refresh());
</script>

<div class="page">
  <div class="inner">
    <header>
      <div>
        <h1>Extensions</h1>
        <p class="lead">Chromium extensions run through WebView2's extension support. Content scripts and background logic work; some browser APIs (toolbar popups, some tab APIs) are limited by WebView2.</p>
        {#if incognito}
          <p class="lead"><strong>Extensions don't run in incognito windows.</strong> Manage them from a normal Zenith window.</p>
        {/if}
      </div>
    </header>

    <div class="install">
      <div class="store">
        <Icon name="globe" size={16} />
        <input placeholder="Paste a Chrome Web Store link or extension ID" bind:value={storeInput} onkeydown={(e) => e.key === 'Enter' && fromStore()} spellcheck="false" />
        <button class="btn primary" disabled={busy || !storeInput.trim()} onclick={fromStore}>{busy ? 'Installing…' : 'Install'}</button>
      </div>
      <div class="buttons">
        <button class="btn" disabled={busy} onclick={() => install(api.extInstallFolder)}><Icon name="folder" size={15} />Load unpacked folder</button>
        <button class="btn" disabled={busy} onclick={() => install(api.extInstallCrx)}><Icon name="file" size={15} />Install .crx file</button>
      </div>
    </div>

    {#if error}
      <div class="error"><Icon name="alert" size={16} /> {error}</div>
    {/if}

    <div class="list">
      {#each userItems as ext (ext.id)}
        <div class="card" class:off={!ext.enabled}>
          <div class="icon">
            {#if ext.icon}<img src={ext.icon} alt="" width="32" height="32" />{:else}<Icon name="puzzle" size={22} />{/if}
          </div>
          <div class="meta">
            <div class="name"><b class="ellipsis">{ext.name}</b>{#if ext.version}<span class="ver">{ext.version}</span>{/if}</div>
            {#if ext.description}<p class="desc">{ext.description}</p>{/if}
            <div class="links">
              {#if ext.popupUrl}<button class="btn ghost sm" onclick={() => browser.openGlance(ext.popupUrl!, browser.activeTab?.id)}><Icon name="eye" size={13} />Open popup</button>{/if}
              {#if ext.optionsUrl}<button class="btn ghost sm" onclick={() => browser.openTab(ext.optionsUrl!)}><Icon name="settings" size={13} />Options</button>{/if}
              <button class="btn ghost sm danger" onclick={() => remove(ext)}><Icon name="trash" size={13} />Remove</button>
            </div>
          </div>
          <Toggle checked={ext.enabled} label={`Enable ${ext.name}`} onchange={(v) => setEnabled(ext, v)} />
        </div>
      {:else}
        {#if !loading && !error}
          <div class="empty">
            <Icon name="puzzle" size={28} />
            <p>No extensions yet. Install one from the Chrome Web Store, a .crx file or an unpacked folder.</p>
          </div>
        {/if}
      {/each}
    </div>

    {#if builtinItems.length}
      <h2>Built into WebView2</h2>
      <div class="builtins">
        {#each builtinItems as ext (ext.id)}
          <div class="builtin"><Icon name="puzzle" size={14} /><span>{ext.name}</span></div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .page {
    height: 100%;
    overflow-y: auto;
  }
  .inner {
    max-width: 860px;
    margin: 0 auto;
    padding: 28px 32px 60px;
  }
  h1 {
    margin: 0;
    font: 600 22px/1.2 var(--font-display);
  }
  .lead {
    margin: 8px 0 0;
    color: var(--text-2);
    max-width: 640px;
  }
  .install {
    margin: 20px 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .store {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 46px;
    padding: 0 6px 0 14px;
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line-strong);
    color: var(--text-3);
  }
  .store input {
    flex: 1;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 13.5px;
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .error {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 10px 14px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .card {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .card.off .icon,
  .card.off .meta {
    opacity: 0.55;
  }
  .icon {
    width: 44px;
    height: 44px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .meta {
    flex: 1;
    min-width: 0;
  }
  .name {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .ver {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .desc {
    margin: 4px 0 8px;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .links {
    display: flex;
    gap: 4px;
    margin-left: -8px;
  }
  .btn.sm {
    height: 26px;
    padding: 0 8px;
    font-size: 12px;
  }
  h2 {
    margin: 28px 0 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-2);
  }
  .builtins {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .builtin {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border-radius: 14px;
    background: var(--field);
    border: 1px solid var(--line);
    font-size: 12px;
    color: var(--text-2);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 60px 0;
    color: var(--text-3);
    text-align: center;
  }
</style>
