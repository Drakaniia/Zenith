<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { openInternal } from '$features/browser/target';
  import { overlayLease, overlaySurface } from '$platform/layout.svelte';
  import { displayHost } from '$shared/url';
  import { extensions } from './extensions.svelte';
  import { extensionMenu } from './menus';
  import { siteAccessStatus } from './site-access';
  import type { ExtensionInfo } from './types';

  // Zen-style unified extensions panel: per-site permission status, inline enable/disable,
  // per-row gear menu (pin/options/inspect/manage/remove), and direct popup/action launch.
  const pos = $derived(extensions.popover);
  let last = { x: 0, y: 0, below: true };
  const at = $derived.by(() => (pos ? (last = pos) : last));
  const W = 340;
  const left = $derived(Math.max(8, Math.min(at.x, window.innerWidth - W - 8)));
  const activeUrl = $derived(browser.activeTab?.url ?? '');
  const activeSite = $derived(displayHost(activeUrl));

  function close() {
    extensions.close();
  }

  function openRowMenu(e: MouseEvent, ext: ExtensionInfo) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    ui.openMenu(r.right, r.bottom + 4, extensionMenu(ext, at));
  }
</script>

{#if pos}
  <div class="root" use:overlayLease>
    <div class="catcher" role="presentation" transition:fade={{ duration: 100 }} onpointerdown={close}></div>
    <div
      class="pop glass"
      use:overlaySurface={10}
      style:left="{left}px"
      style:top={at.below ? `${at.y}px` : null}
      style:bottom={at.below ? null : `${window.innerHeight - at.y}px`}
      transition:fly={{ y: at.below ? -8 : 8, duration: 150 }}
      role="dialog"
      aria-label="Extensions"
    >
      <div class="head">
        <div class="title-wrap">
          <h3>Extensions</h3>
          {#if activeSite}<span class="site ellipsis" title={activeUrl}>{activeSite}</span>{/if}
        </div>
        <button
          class="icon-btn"
          title="Manage extensions"
          onclick={() => {
            close();
            openInternal('extensions');
          }}
        >
          <Icon name="settings" size={14} />
        </button>
      </div>
      {#if extensions.usable.length}
        <div class="list">
          {#each extensions.usable as ext (ext.id)}
            {@const badge = extensions.badgeFor(ext.id, browser.activeTab?.id)}
            {@const access = siteAccessStatus(ext, activeUrl)}
            <div class="row" class:off={!ext.enabled} data-ext-row={ext.id}>
              <button
                class="open"
                disabled={!ext.enabled}
                title={ext.popupUrl ? 'Open popup' : ext.hasAction ? 'Run extension' : ext.optionsUrl ? 'Options' : ''}
                onclick={() => void extensions.open(ext, at)}
              >
                <span class="ico">
                  {#if ext.icon}<img src={ext.icon} alt="" width="20" height="20" />{:else}<Icon name="puzzle" size={16} />{/if}
                  {#if badge?.text}
                    <span
                      class="ico-badge"
                      style:background={badge.backgroundColor ?? undefined}
                      style:color={badge.textColor ?? undefined}
                    >{badge.text}</span>
                  {/if}
                </span>
                <span class="info">
                  <span class="name ellipsis">{ext.name}</span>
                  <span class="status ellipsis">
                    <span class="dot {access.state}" aria-hidden="true"></span>
                    {access.label}
                  </span>
                </span>
              </button>
              <div class="controls">
                <Toggle
                  checked={ext.enabled}
                  label={`Enable ${ext.name}`}
                  onchange={(v) => void extensions.setEnabled(ext, v)}
                />
                <button
                  class="icon-btn gear"
                  title="Extension actions"
                  aria-label={`Actions for ${ext.name}`}
                  onclick={(e) => openRowMenu(e, ext)}
                >
                  <Icon name="more" size={14} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="empty">
          <p>No extensions installed yet.</p>
        </div>
      {/if}
      <div class="foot">
        <button class="btn ghost" onclick={() => extensions.openStore()}><Icon name="globe" size={14} />Extension catalogue</button>
        <button
          class="btn ghost"
          onclick={() => {
            close();
            openInternal('extensions');
          }}><Icon name="settings" size={14} />Manage</button
        >
      </div>
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 85;
  }
  .catcher {
    position: absolute;
    inset: 0;
  }
  .pop {
    position: absolute;
    width: 340px;
    max-height: min(500px, calc(100vh - 80px));
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-xl);
    padding: 10px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin: 0 4px 8px 8px;
  }
  .title-wrap {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font: 600 14px/1.2 var(--font-display);
  }
  .site {
    font-size: 11px;
    color: var(--text-3);
    margin-top: 2px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    min-height: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: 4px;
    border-radius: var(--radius);
  }
  .row:hover {
    background: var(--element-bg);
  }
  .open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 6px 6px 6px 8px;
    border: 0;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .open:disabled {
    cursor: default;
  }
  .row.off .ico,
  .row.off .info {
    opacity: 0.5;
  }
  .ico {
    position: relative;
    width: 24px;
    height: 24px;
    flex: none;
    display: grid;
    place-items: center;
  }
  .ico img {
    display: block;
    object-fit: contain;
    border-radius: 4px;
  }
  .ico-badge {
    position: absolute;
    right: -3px;
    bottom: -2px;
    min-width: 13px;
    height: 13px;
    padding: 0 3px;
    border-radius: 7px;
    background: var(--accent);
    color: #fff;
    font: 700 8.5px/13px var(--font-sans, system-ui);
    text-align: center;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .name {
    font-size: 13px;
    line-height: 1.25;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-3);
    line-height: 1.2;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex: none;
    background: var(--text-3);
  }
  .dot.allowed {
    background: #34c759;
  }
  .dot.on-click {
    background: var(--accent);
  }
  .dot.restricted {
    background: #ff9f0a;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  .gear {
    opacity: 0.65;
  }
  .row:hover .gear {
    opacity: 1;
  }
  .empty {
    padding: 16px 8px 20px;
    color: var(--text-3);
    text-align: center;
  }
  .empty p {
    margin: 0;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }
  .foot .btn {
    gap: 6px;
  }
</style>
