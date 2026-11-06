<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import UrlPill from './UrlPill.svelte';
  import WindowControls from './WindowControls.svelte';
  import { browser } from '../lib/browser.svelte';
  import { settings } from '../lib/settings.svelte';
  import { ui, type MenuItem } from '../lib/ui.svelte';
  import { api } from '../lib/ipc';
  import { appMenu } from '../lib/menus';
  import { openInternal, runAction } from '../lib/actions';
  import { appInfo, incognito } from '../lib/app.svelte';

  // Zen's #nav-bar for the multiple-toolbars and collapsed layouts (38px): navigation buttons,
  // a centred address bar, page and app menus, and the window controls at the end.
  let { floating = false }: { floating?: boolean } = $props();

  const tab = $derived(browser.activeTab);
  const live = $derived(tab?.live ? tab : undefined);
  const collapsed = $derived(settings.value.toolbarMode === 'collapsed');

  // ---------------------------------------------------------------- overflow
  // On a narrow window the address bar shrinks first; then these buttons move, in this order,
  // into a » menu, and finally the Incognito/Test badges shrink to an icon. The app menu and the
  // window controls always stay on the bar.
  type Spill = 'extensions' | 'split' | 'reload' | 'forward' | 'back';
  const SPILL_ORDER: Spill[] = ['extensions', 'split', 'reload', 'forward', 'back'];
  const SPILL_MAX = SPILL_ORDER.length + 1;
  let bar: HTMLDivElement | undefined = $state();
  let spilled = $state(0);
  const hidden = $derived(new Set(SPILL_ORDER.slice(0, spilled)));
  const tightBadges = $derived(spilled > SPILL_ORDER.length);

  let fitting = false;
  async function fit() {
    if (!bar || fitting) return;
    fitting = true;
    try {
      spilled = 0;
      await tick();
      while (bar && bar.scrollWidth > bar.clientWidth + 1 && spilled < SPILL_MAX) {
        spilled++;
        await tick();
      }
    } finally {
      fitting = false;
    }
  }

  $effect(() => {
    if (!bar) return;
    const ro = new ResizeObserver(() => void fit());
    ro.observe(bar);
    return () => ro.disconnect();
  });
  // Content that changes the bar's width needs a new fit too.
  $effect(() => {
    void collapsed;
    void incognito;
    void appInfo.info?.testProfile;
    void fit();
  });

  function overflowItems(): MenuItem[] {
    const items: MenuItem[] = [];
    if (hidden.has('back'))
      items.push({ label: 'Back', icon: 'back', hint: 'Alt+Left', disabled: !live?.canGoBack, run: () => live && api.tabCommand(live.id, 'back') });
    if (hidden.has('forward'))
      items.push({ label: 'Forward', icon: 'forward', hint: 'Alt+Right', disabled: !live?.canGoForward, run: () => live && api.tabCommand(live.id, 'forward') });
    if (hidden.has('reload')) {
      if (live?.loading) items.push({ label: 'Stop', icon: 'stop', run: () => live && api.tabCommand(live.id, 'stop') });
      else
        items.push({
          label: 'Reload',
          icon: 'reload',
          hint: 'F5',
          disabled: !live && !tab?.crashed,
          run: () => tab && (tab.crashed ? browser.reviveCrashed(tab.id) : api.tabCommand(tab.id, 'reload')),
        });
    }
    if (hidden.has('split'))
      items.push({
        label: browser.activeSplit ? 'Exit split view' : 'Split view',
        icon: browser.activeSplit ? 'unsplit' : 'split',
        hint: 'Ctrl+Alt+S',
        disabled: !tab,
        run: () => browser.toggleSplit(),
      });
    if (hidden.has('extensions')) items.push({ label: 'Extensions', icon: 'puzzle', run: () => openInternal('extensions') });
    return items;
  }
</script>

<div class="topbar" class:floating bind:this={bar} data-tauri-drag-region="deep">
  {#if collapsed}
    <button class="icon-btn" title="Expand sidebar (Ctrl+Shift+B)" data-action="toggle-sidebar" onclick={() => void runAction('toggleSidebar')}>
      <Icon name="sidebarExpand" />
    </button>
    <button
      class="icon-btn"
      title={settings.value.compact ? 'Turn off compact mode (Ctrl+B)' : 'Compact mode (Ctrl+B)'}
      data-action="compact"
      onclick={() => void runAction('toggleCompact')}
    >
      <Icon name="sidebar" />
    </button>
  {:else}
    <button class="icon-btn" title="Collapse sidebar (Ctrl+Shift+B)" data-action="toggle-sidebar" onclick={() => void runAction('toggleSidebar')}>
      <Icon name="sidebarCollapse" />
    </button>
  {/if}
  {#if !hidden.has('back')}
    <button class="icon-btn" title="Back (Alt+Left)" disabled={!live?.canGoBack} onclick={() => live && api.tabCommand(live.id, 'back')}>
      <Icon name="back" />
    </button>
  {/if}
  {#if !hidden.has('forward')}
    <button class="icon-btn" title="Forward (Alt+Right)" disabled={!live?.canGoForward} onclick={() => live && api.tabCommand(live.id, 'forward')}>
      <Icon name="forward" />
    </button>
  {/if}
  {#if !hidden.has('reload')}
    {#if live?.loading}
      <button class="icon-btn" title="Stop" onclick={() => live && api.tabCommand(live.id, 'stop')}><Icon name="stop" /></button>
    {:else}
      <button
        class="icon-btn"
        title="Reload (F5)"
        disabled={!live && !tab?.crashed}
        onclick={() => tab && (tab.crashed ? browser.reviveCrashed(tab.id) : api.tabCommand(tab.id, 'reload'))}
      >
        <Icon name="reload" />
      </button>
    {/if}
  {/if}

  <span class="spring" data-tauri-drag-region></span>
  {#if appInfo.info?.testProfile && !tightBadges}
    <span class="test-badge" title="Separate profile (ZENITH_PROFILE_DIR), used by automated tests">Test</span>
  {/if}
  {#if incognito}
    <span
      class="incognito-badge"
      class:tight={tightBadges}
      title="Incognito window: history, cookies and site data are deleted when it closes"
    >
      <Icon name="incognito" size={14} />{#if !tightBadges}<span class="badge-text">Incognito</span>{/if}
    </span>
  {/if}
  <UrlPill variant="toolbar" />
  <span class="spring" data-tauri-drag-region></span>

  {#if !hidden.has('split')}
    <button
      class="icon-btn"
      class:on={!!browser.activeSplit}
      title={browser.activeSplit ? 'Exit split view' : 'Split view (Ctrl+Alt+S)'}
      data-action="split"
      onclick={() => browser.toggleSplit()}
      disabled={!tab}
    >
      <Icon name={browser.activeSplit ? 'unsplit' : 'split'} />
    </button>
  {/if}
  {#if !hidden.has('extensions')}
    <button class="icon-btn" title="Extensions" onclick={() => openInternal('extensions')}>
      <Icon name="puzzle" />
    </button>
  {/if}
  {#if spilled > 0}
    <button
      class="icon-btn"
      title="More tools"
      data-action="overflow"
      onclick={(e) => {
        const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
        ui.openMenu(Math.max(4, r.right - 220), r.bottom + 6, overflowItems());
      }}
    >
      <Icon name="chevronsRight" />
    </button>
  {/if}
  <button
    class="icon-btn"
    title="Menu"
    data-action="app-menu"
    onclick={(e) => {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      ui.openMenu(Math.max(4, r.right - 240), r.bottom + 6, appMenu());
    }}
  >
    <Icon name="more" />
  </button>
  <WindowControls />
</div>

<style>
  .topbar {
    height: var(--zen-toolbar-h);
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    padding-left: var(--zen-pad);
  }
  /* Only the address bar and the drag springs give up width; buttons keep their size. */
  .topbar > :global(*) {
    flex-shrink: 0;
  }
  .topbar > :global(.pill.toolbar) {
    flex: 1 1 0%;
    min-width: 72px;
  }
  .topbar.floating {
    height: var(--zen-toolbar-h);
    padding-left: var(--zen-sep);
  }
  .spring {
    flex: 1 1 0;
    min-width: 4px;
    align-self: stretch;
  }
  .topbar > .spring {
    flex-shrink: 1;
  }
  .icon-btn.on {
    background: var(--element-bg);
  }
  .incognito-badge {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-right: 6px;
    padding: 3px 10px 3px 8px;
    border-radius: 12px;
    font-size: 12px;
    font-weight: 600;
    background: color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .incognito-badge.tight {
    margin-right: 2px;
    padding: 3px 5px;
  }
  .test-badge {
    flex: none;
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 11px;
    font-weight: 600;
    color: #fff;
    background: #d9480f;
  }
</style>
