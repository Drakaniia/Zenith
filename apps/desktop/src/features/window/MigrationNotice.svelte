<script lang="ts">
  import { fly } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { overlayLease, overlaySurface } from '$platform/layout.svelte';
  import { appInfo, incognito } from './window.svelte';
  import { windowApi } from './api';

  const visible = $derived(!incognito && appInfo.info?.migrationNotice === true);

  function dismiss() {
    if (appInfo.info) appInfo.info = { ...appInfo.info, migrationNotice: false };
    void windowApi.migrationNoticeDismiss().catch(() => {});
  }
</script>

{#if visible}
  <div class="notice glass" use:overlayLease use:overlaySurface={12} transition:fly={{ y: -10, duration: 160 }} role="dialog" aria-label="CEF engine upgrade notice">
    <div class="icon"><Icon name="info" size={20} /></div>
    <div class="body">
      <p class="title">Zenith upgraded to the bundled CEF engine</p>
      <p class="sub">
        Your bookmarks, passwords, history, settings, profiles, and open tabs were preserved (with a pre-migration backup).
        Website sign-ins, cookies, site storage, and extension data start fresh; your old WebView2 folder was left untouched.
      </p>
    </div>
    <div class="actions">
      <button class="btn primary" onclick={dismiss}>Got it</button>
    </div>
  </div>
{/if}

<style>
  .notice {
    position: fixed;
    top: calc(var(--topbar) + 14px);
    left: calc(var(--sidebar-w) + 16px);
    z-index: 81;
    width: 440px;
    max-width: calc(100vw - 32px);
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 12px 14px;
    padding: 16px;
    border-radius: var(--radius-xl);
    border: 1px solid var(--line);
  }
  .icon {
    width: 40px;
    height: 40px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .title {
    margin: 0 0 6px;
    font-weight: 500;
    line-height: 1.4;
  }
  .sub {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
  }
</style>
