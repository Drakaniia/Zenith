<script lang="ts">
  import Icon from '../Icon.svelte';
  import { browser } from '../../lib/browser.svelte';
  import { settings } from '../../lib/settings.svelte';
  import { ui } from '../../lib/ui.svelte';
  import { api } from '../../lib/ipc';
  import { appMenu } from '../../lib/menus';
  import { openInternal } from '../../lib/actions';
  import { incognito } from '../../lib/app.svelte';

  // Zen's #zen-sidebar-top-buttons. In the single-toolbar layout it holds the app menu, the
  // extensions button and the compact-mode toggle, then (after a spacer) back/forward/reload.
  // With multiple toolbars only the compact-mode toggle stays here.
  let { single = true }: { single?: boolean } = $props();

  const active = $derived(browser.activeTab);
  const live = $derived(active?.live ? active : undefined);

  function toggleCompact() {
    ui.flyout = false;
    settings.update({ compact: !settings.value.compact });
  }
</script>

<header class="top" data-tauri-drag-region="deep">
  {#if single}
    <button
      class="icon-btn"
      title="Menu"
      data-action="app-menu"
      onclick={(e) => {
        const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
        ui.openMenu(r.left, r.bottom + 4, appMenu());
      }}
    >
      <Icon name="more" />
    </button>
    <button class="icon-btn" title="Extensions" onclick={() => openInternal('extensions')}>
      <Icon name="puzzle" />
    </button>
  {/if}
  <button
    class="icon-btn"
    title={settings.value.compact ? 'Turn off compact mode (Ctrl+B)' : 'Compact mode (Ctrl+B)'}
    data-action="compact"
    onclick={toggleCompact}
  >
    <Icon name="sidebar" />
  </button>
  {#if incognito}
    <span
      class="incognito-badge"
      class:labelled={!single}
      title="Incognito window: history, cookies and site data are deleted when it closes"
      data-tauri-drag-region
    >
      <Icon name="incognito" />
      {#if !single}<span>Incognito</span>{/if}
    </span>
  {/if}
  <span class="spacer" data-tauri-drag-region></span>
  {#if single}
    <button class="icon-btn" title="Back (Alt+Left)" disabled={!live?.canGoBack} onclick={() => live && api.tabCommand(live.id, 'back')}>
      <Icon name="back" />
    </button>
    <button class="icon-btn" title="Forward (Alt+Right)" disabled={!live?.canGoForward} onclick={() => live && api.tabCommand(live.id, 'forward')}>
      <Icon name="forward" />
    </button>
    {#if live?.loading}
      <button class="icon-btn" title="Stop" onclick={() => live && api.tabCommand(live.id, 'stop')}><Icon name="stop" /></button>
    {:else}
      <button
        class="icon-btn"
        title="Reload (F5)"
        disabled={!live && !active?.crashed}
        onclick={() => active && (active.crashed ? browser.reviveCrashed(active.id) : api.tabCommand(active.id, 'reload'))}
      >
        <Icon name="reload" />
      </button>
    {/if}
  {/if}
</header>

<style>
  .top {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    height: var(--zen-top-buttons-h);
    margin: calc(var(--zen-pad) / 2) 0;
    padding: 0 var(--zen-pad);
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .incognito-badge {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 6px;
    margin-left: 2px;
    border-radius: 13px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-1, currentColor);
    background: color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .incognito-badge.labelled {
    padding: 0 10px 0 8px;
  }
</style>
