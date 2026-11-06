<script lang="ts">
  import Icon from './Icon.svelte';
  import { browser } from '../lib/browser.svelte';
  import { bookmarks } from '../lib/bookmarks.svelte';
  import { settings } from '../lib/settings.svelte';
  import { ui } from '../lib/ui.svelte';
  import { copyText, toggleBookmark, toggleSiteBlocking } from '../lib/actions';
  import { pageMenu } from '../lib/menus';
  import { displayHost, hostOf, isInternal, isNewTab, isSecure, isWebUrl, siteOf, titleFor } from '../lib/url';

  // Zen's address bar. In the sidebar (single toolbar) its page actions fade in while the
  // sidebar is hovered; in the top toolbar it sits centred and always shows them.
  let { variant = 'sidebar' }: { variant?: 'sidebar' | 'toolbar' } = $props();

  const tab = $derived(browser.activeTab);
  const url = $derived(tab?.url ?? '');
  const web = $derived(isWebUrl(url));
  const empty = $derived(!tab || isNewTab(url));
  const label = $derived(empty ? 'Search or enter address' : isInternal(url) ? titleFor(url, '') : displayHost(url));
  const blockingOff = $derived(web && settings.value.blockerDisabledSites.includes(siteOf(hostOf(url))));
  const starred = $derived(bookmarks.has(url));
  const zoom = $derived(tab?.live ? Math.round((tab.zoom || 1) * 100) : 100);
  let copied = $state(false);

  function open() {
    ui.openPalette(tab && web ? 'navigate' : 'newTab', web ? url : '');
  }
</script>

<div class="pill {variant}" class:empty class:loading={tab?.loading && tab.live}>
  <button class="main" onclick={open} title={web ? url : 'Search or enter address (Ctrl+L)'}>
    <span class="lead" class:chrome={!web && !empty}>
      {#if empty}
        <Icon name="search" size={15} />
      {:else if !web}
        <Icon name="info" size={15} />
      {:else if isSecure(url)}
        <Icon name="lock" size={14} />
      {:else}
        <Icon name="unlock" size={14} />
      {/if}
    </span>
    <span class="host ellipsis">{label}</span>
  </button>
  <div class="actions">
    {#if zoom !== 100}
      <button class="zoom" title="Reset zoom (Ctrl+0)" onclick={() => tab && void browser.resetZoom(tab.id)}>{zoom}%</button>
    {/if}
    {#if web && tab && tab.media > 0}
      <button
        class="icon-btn sm media"
        title={tab.media === 1 ? 'Download the video on this page' : `Download videos on this page (${tab.media} found)`}
        onclick={() => ui.openMedia(tab.id)}
      >
        <Icon name="download" size={14} />
        {#if tab.media > 1}<span class="count">{tab.media > 9 ? '9+' : tab.media}</span>{/if}
      </button>
    {/if}
    {#if web}
      <button
        class="icon-btn sm copy"
        title="Copy link"
        onclick={() => {
          void copyText(url);
          copied = true;
          ui.toast('Link copied', { icon: 'link' });
          setTimeout(() => (copied = false), 1200);
        }}
      >
        <Icon name={copied ? 'check' : 'link'} size={14} />
      </button>
      <button
        class="icon-btn sm shield"
        class:off={blockingOff || !settings.value.blockerEnabled}
        title={blockingOff
          ? 'Ad and tracker blocking is off for this site'
          : `${tab?.blocked ?? 0} ads and trackers blocked on this page`}
        onclick={() => toggleSiteBlocking(tab)}
      >
        <Icon name={blockingOff || !settings.value.blockerEnabled ? 'shieldOff' : 'shieldCheck'} size={14} />
        {#if (tab?.blocked ?? 0) > 0 && !blockingOff}
          <span class="count">{tab!.blocked > 99 ? '99+' : tab!.blocked}</span>
        {/if}
      </button>
      <button class="icon-btn sm star" class:on={starred} title={starred ? 'Remove bookmark' : 'Bookmark (Ctrl+D)'} onclick={() => void toggleBookmark(tab)}>
        <Icon name="star" size={14} fill={starred} />
      </button>
    {/if}
    {#if tab}
      <button
        class="icon-btn sm"
        title="Page actions"
        data-action="page-menu"
        onclick={(e) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          ui.openMenu(r.left, r.bottom + 6, pageMenu());
        }}
      >
        <Icon name="more" size={15} />
      </button>
    {/if}
  </div>
  <span class="progress" aria-hidden="true"></span>
</div>

<style>
  .pill {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    height: 40px;
    padding: 0 4px;
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    background: var(--element-bg);
    overflow: hidden;
    transition: background-color 0.1s;
  }
  .pill.sidebar {
    margin: 0 calc(var(--zen-pad) + 1px) 2px;
  }
  .pill.toolbar {
    height: 32px;
    flex: 1;
    max-width: 760px;
    min-width: 160px;
  }
  .pill:hover {
    background: var(--element-bg-hover);
  }
  .main {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 6px 0 2px;
    text-align: left;
  }
  .lead {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    flex: none;
    border-radius: 8px;
    color: var(--toolbar-text);
  }
  .lead.chrome {
    background: var(--element-bg-hover);
  }
  .host {
    font-size: 13px;
    font-weight: 500;
    color: var(--toolbar-text);
  }
  .pill.empty .host {
    color: color-mix(in srgb, var(--toolbar-text) 60%, transparent);
    font-weight: 400;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 1px;
  }
  /* Page actions fade in while the sidebar is hovered (zen-has-implicit-hover). */
  /* Hidden actions take no width (Zen: visibility: collapse), so the host gets the full pill. */
  .pill.sidebar:not(.empty) .actions {
    max-width: 0;
    overflow: hidden;
    opacity: 0;
    visibility: hidden;
    transition:
      opacity 0.15s,
      max-width 0s linear 0.15s,
      visibility 0s linear 0.15s;
  }
  :global(.sidebar:hover) .pill.sidebar .actions,
  .pill.sidebar:focus-within .actions {
    max-width: 200px;
    opacity: 1;
    visibility: visible;
    transition: opacity 0.15s;
  }
  .actions .icon-btn {
    border-radius: 8px;
    color: var(--toolbar-text);
  }
  .shield,
  .media {
    position: relative;
    width: auto;
    min-width: 24px;
    padding: 0 5px;
    gap: 3px;
    display: inline-flex;
  }
  .shield {
    color: var(--ok) !important;
  }
  .shield.off {
    color: var(--text-3) !important;
  }
  .count {
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .star.on {
    color: var(--warn) !important;
  }
  .media {
    color: var(--accent-fg) !important;
    animation: media-in 0.3s var(--ease-out);
  }
  .zoom {
    height: 22px;
    padding: 0 7px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    background: var(--element-bg-hover);
  }
  @keyframes media-in {
    from {
      opacity: 0;
      transform: scale(0.8);
    }
  }
  /* Loading indicator: a soft bar sweeping along the bottom edge while the page loads. */
  .progress {
    position: absolute;
    left: 12%;
    right: 12%;
    bottom: 0;
    height: 2px;
    overflow: hidden;
    opacity: 0;
    transition: opacity 0.2s;
    pointer-events: none;
  }
  .progress::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    width: 75%;
    left: 12.5%;
    border-radius: 2px;
    background: linear-gradient(90deg, transparent, var(--accent-button), transparent);
    animation: long-load 1.4s ease-in-out infinite;
  }
  .pill.loading .progress {
    opacity: 1;
  }
  @keyframes long-load {
    from {
      transform: translateX(calc(-400% / 3));
    }
    to {
      transform: translateX(calc(400% / 3));
    }
  }
</style>
