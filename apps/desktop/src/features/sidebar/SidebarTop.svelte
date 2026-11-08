<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import ExtensionButtons from '$features/extensions/ExtensionButtons.svelte';
  import ProfileButton from '$features/profiles/ProfileButton.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { tabsApi } from '$features/browser/api';
  import { appMenu } from '../../app/menus';
  import { withHint } from '../../app/actions';
  import { incognito } from '$features/window/window.svelte';

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
  {#if !incognito}
    <ProfileButton />
  {/if}
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
    <ExtensionButtons maxPinned={2} />
  {/if}
  <button
    class="icon-btn"
    title={withHint(settings.value.compact ? 'Turn off compact mode' : 'Compact mode', 'toggleCompact')}
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
    <button class="icon-btn" title={withHint('Back', 'back')} disabled={!live?.canGoBack} onclick={() => live && tabsApi.tabCommand(live.id, 'back')}>
      <Icon name="back" />
    </button>
    <button class="icon-btn" title={withHint('Forward', 'forward')} disabled={!live?.canGoForward} onclick={() => live && tabsApi.tabCommand(live.id, 'forward')}>
      <Icon name="forward" />
    </button>
    {#if live?.loading}
      <button class="icon-btn" title="Stop" onclick={() => live && tabsApi.tabCommand(live.id, 'stop')}><Icon name="stop" /></button>
    {:else}
      <button
        class="icon-btn"
        title={withHint('Reload', 'reload')}
        disabled={!live && !active?.crashed}
        onclick={() => active && (active.crashed ? browser.reviveCrashed(active.id) : tabsApi.tabCommand(active.id, 'reload'))}
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
