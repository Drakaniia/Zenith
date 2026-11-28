<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { browser } from './browser.svelte';
  import { followLayout, glancePane, overlayLease, overlaySurface, scheduleLayout } from '$platform/layout.svelte';
  import { copyText } from '$shared/clipboard';
  import { ui } from '$shared/ui/ui.svelte';
  import { springEase } from '$shared/ui/motion';
  import { displayHost } from '$shared/url';
  import { tabsApi } from './api';

  // Zen's Glance (src/zen/glance): a page previewed in a card over the current tab, 80 % of the
  // content area wide, with round action buttons beside it. Opens with a 350 ms spring.
  const g = $derived(browser.glance);
  const tab = $derived(g ? browser.tabs[g.tabId] : undefined);
  // The card keeps its last shape while it plays its exit transition (`browser.glance` is
  // already null by then).
  let lastPopup = false;
  let lastTabId = '';
  const tabId = $derived.by(() => (g ? (lastTabId = g.tabId) : lastTabId));
  const popup = $derived.by(() => (g ? (lastPopup = g.popup) : lastPopup));

  let area = $state({ left: 0, top: 0, width: window.innerWidth, height: window.innerHeight });
  function measure() {
    const stage = document.querySelector('.stage .panes') ?? document.querySelector('.stage');
    if (!stage) return;
    const r = stage.getBoundingClientRect();
    area = { left: r.left, top: r.top, width: r.width, height: r.height };
  }
  onMount(() => {
    measure();
    const ro = new ResizeObserver(measure);
    const stage = document.querySelector('.stage');
    if (stage) ro.observe(stage);
    window.addEventListener('resize', measure);
    return () => {
      ro.disconnect();
      window.removeEventListener('resize', measure);
    };
  });
  $effect(() => {
    if (g) measure();
  });

  const frame = $derived.by(() => {
    const w = Math.round(area.width * (popup ? 0.55 : 0.8));
    const h = Math.round(popup ? area.height * 0.84 : area.height);
    return { left: Math.round(area.left + (area.width - w) / 2), top: Math.round(area.top + (area.height - h) / 2), w, h };
  });

  // "Open as tab": the card grows into the tab area (the webview follows it frame by frame),
  // then becomes the tab with no exit transition, so the page never blinks.
  const MORPH_MS = 340;
  let morph = $state<{ left: number; top: number; w: number; h: number } | null>(null);
  let expanding = $state(false);
  let morphTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    if (!g) return;
    // A new card starts in its normal shape.
    morph = null;
    expanding = false;
  });
  const box = $derived(morph ?? frame);
  const originCss = $derived.by(() => {
    const o = g?.origin;
    if (!o) return undefined;
    const ox = Math.max(0, Math.min(frame.w, o.x - frame.left));
    const oy = Math.max(0, Math.min(frame.h, o.y - frame.top));
    return `${Math.round(ox)}px ${Math.round(oy)}px`;
  });

  function openAsTab() {
    if (!g || morph) return;
    const id = g.tabId;
    const finish = () => {
      if (browser.glance?.tabId !== id) return;
      expanding = true;
      browser.expandGlance();
    };
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return finish();
    measure();
    morph = { left: Math.round(area.left), top: Math.round(area.top), w: Math.round(area.width), h: Math.round(area.height) };
    followLayout(MORPH_MS + 60);
    clearTimeout(morphTimer);
    morphTimer = setTimeout(finish, MORPH_MS);
  }
</script>

{#if g && tab}
  <div class="root" class:morphing={!!morph} use:overlayLease>
    <div
      class="backdrop"
      role="presentation"
      transition:fade={{ duration: expanding ? 0 : 200 }}
      onpointerdown={() => browser.closeGlance()}
    ></div>
    <div
      class="frame"
      class:popup
      class:morphing={!!morph}
      use:overlaySurface={6}
      style:left="{box.left}px"
      style:top="{box.top}px"
      style:width="{box.w}px"
      style:height="{box.h}px"
      style:transform-origin={originCss}
      style:--morph-ms="{MORPH_MS}ms"
      in:scale={{ start: 0.86, duration: 350, opacity: 0, easing: springEase }}
      onintroend={() => scheduleLayout()}
      out:scale={{ start: 0.92, duration: expanding ? 0 : 200, opacity: 0 }}
    >
      <div class="glance-bar">
        <button type="button" class="gbtn" title="Back" disabled={!tab.canGoBack} onclick={() => void tabsApi.tabCommand(tab.id, 'back')}>
          <Icon name="back" size={13} />
        </button>
        <button type="button" class="gbtn" title="Forward" disabled={!tab.canGoForward} onclick={() => void tabsApi.tabCommand(tab.id, 'forward')}>
          <Icon name="forward" size={13} />
        </button>
        <button type="button" class="gbtn" title={tab.loading ? 'Stop' : 'Reload'} onclick={() => void tabsApi.tabCommand(tab.id, tab.loading ? 'stop' : 'reload')}>
          <Icon name={tab.loading ? 'stop' : 'reload'} size={13} />
        </button>
        <span class="ghost-url ellipsis" title={tab.url}>{displayHost(tab.url) || tab.url}</span>
      </div>
      <div class="body" use:glancePane={tabId}>
        {#if !tab.live}<div class="placeholder"></div>{/if}
        {#if tab.loading}<span class="loading"></span>{/if}
      </div>
    </div>
    <div class="side" use:overlaySurface={17} style:left="{frame.left + frame.w}px" style:top="{frame.top + 3}px" transition:fade={{ duration: expanding ? 0 : 160 }}>
      <button class="action close" title="Close (Esc)" onclick={() => browser.closeGlance()}><Icon name="close" size={16} /></button>
      <button class="action" title="Open as tab" onclick={openAsTab}><Icon name="expand" size={16} /></button>
      {#if g?.openerId && browser.tabs[g.openerId]}
        <button class="action" title="Open in split view" onclick={() => browser.expandGlance(true)}><Icon name="split" size={16} /></button>
      {/if}
      <button
        class="action"
        title="Copy link"
        onclick={() => {
          void copyText(tab.url);
          ui.toast('Link copied', { icon: 'link' });
        }}><Icon name="link" size={16} /></button
      >
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 60;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    background: var(--backdrop);
  }
  .morphing .backdrop {
    opacity: 0;
    transition: opacity 240ms ease-out;
  }
  .morphing .side {
    opacity: 0;
    pointer-events: none;
    transition: opacity 120ms ease-out;
  }
  .frame.morphing {
    transition:
      left var(--morph-ms) var(--zen-spring),
      top var(--morph-ms) var(--zen-spring),
      width var(--morph-ms) var(--zen-spring),
      height var(--morph-ms) var(--zen-spring);
  }
  .frame {
    position: absolute;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-content);
    box-shadow: var(--zen-big-shadow);
    overflow: hidden;
    background: var(--page-bg);
    transform-origin: 50% 40%;
  }
  .glance-bar {
    flex: none;
    height: 30px;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px;
    background: var(--surface-solid);
    border-bottom: 1px solid var(--line);
    color: var(--text-2);
    font-size: 12px;
  }
  .gbtn {
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 6px;
  }
  .gbtn:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .gbtn:disabled {
    opacity: 0.35;
  }
  .ghost-url {
    flex: 1;
    min-width: 0;
    margin-left: 6px;
    opacity: 0.8;
  }
  .body {
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .placeholder {
    position: absolute;
    inset: 0;
    background: var(--page-bg);
  }
  .loading {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 2px;
    background: linear-gradient(90deg, transparent, var(--accent-button), transparent);
    animation: sweep 1.2s ease-in-out infinite;
  }
  @keyframes sweep {
    from {
      transform: translateX(-60%);
    }
    to {
      transform: translateX(60%);
    }
  }
  /* .zen-glance-sidebar-container: 12px from the card, 34px round buttons in the opposite tone. */
  .side {
    position: absolute;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
  }
  .action {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 999px;
    color: color-mix(in srgb, var(--accent) 70%, rgb(24, 24, 24));
    background: color-mix(in srgb, rgb(231, 231, 231) 96%, var(--accent));
    box-shadow: 0 0 12px 1px rgba(0, 0, 0, 0.07);
    transition:
      background 0.05s ease,
      scale 0.05s ease;
  }
  :global(:root[data-theme='light']) .action {
    color: color-mix(in srgb, var(--accent) 60%, white);
    background: color-mix(in srgb, rgb(24, 24, 24) 96%, var(--accent));
  }
  .action:hover {
    background: rgb(204, 204, 204);
    scale: 1.02;
  }
  :global(:root[data-theme='light']) .action:hover {
    background: rgb(41, 41, 41);
  }
  .action:hover:active {
    scale: 0.98;
  }
</style>
