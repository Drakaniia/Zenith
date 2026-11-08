<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { browser } from './browser.svelte';
  import { followLayout, glancePane, overlayLease, scheduleLayout } from '$platform/layout.svelte';
  import { copyText } from '../../app/actions';
  import { ui } from '$shared/ui/ui.svelte';
  import { springEase } from '$shared/ui/motion';
  import type { GlanceState } from './types';
  import { api } from '$platform/ipc';

  // Zen's Glance (src/zen/glance): a page previewed in a card over the current tab, 80 % of the
  // content area wide, with round action buttons beside it. Opens with a 350 ms spring.
  const g = $derived(browser.glance);
  const tab = $derived(g ? browser.tabs[g.tabId] : undefined);
  // The card keeps its last shape while it plays its exit transition (`browser.glance` is
  // already null by then).
  let lastAnchor: GlanceState['anchor'] = undefined;
  let lastPopup = false;
  let lastTabId = '';
  const tabId = $derived.by(() => (g ? (lastTabId = g.tabId) : lastTabId));
  const anchor = $derived.by(() => (g ? (lastAnchor = g.anchor) : lastAnchor));
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

  // Extension popups size to their content like Chrome's (25 x 25 to 800 x 600); until the
  // page has been measured the card has a typical popup's size.
  let fit = $state<{ id: string; w: number; h: number } | null>(null);
  const extTabId = $derived(g?.anchor ? g.tabId : null);
  const extLoaded = $derived(!!extTabId && browser.tabs[extTabId]?.loading === false);
  $effect(() => {
    const id = extTabId;
    if (!id || !extLoaded) return;
    let stop = false;
    const measure = async () => {
      if (stop) return;
      const s = await api.tabContentSize(id).catch(() => null);
      if (stop || !s || s.x < 1 || s.y < 1) return;
      fit = { id, w: Math.min(800, Math.max(25, s.x)), h: Math.min(600, Math.max(25, s.y)) };
      scheduleLayout();
    };
    // Re-measured while the popup settles: many build their UI after load.
    const timers = [0, 150, 400, 900, 1600].map((ms) => setTimeout(measure, ms));
    return () => {
      stop = true;
      timers.forEach(clearTimeout);
    };
  });
  const sized = $derived(fit && fit.id === tabId ? fit : null);
  const EXT_W = $derived(sized?.w ?? 380);
  const EXT_H = $derived(sized?.h ?? 560);

  const frame = $derived.by(() => {
    if (anchor) {
      const w = Math.min(EXT_W, window.innerWidth - 16);
      const h = Math.min(EXT_H, anchor.below ? window.innerHeight - anchor.y - 8 : anchor.y - 8);
      const left = Math.max(8, Math.min(anchor.x, window.innerWidth - w - 8));
      return { left, top: anchor.below ? anchor.y : anchor.y - h, w, h };
    }
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

  function openAsTab() {
    if (!g || morph) return;
    const id = g.tabId;
    const finish = () => {
      if (browser.glance?.tabId !== id) return;
      expanding = true;
      browser.expandGlance();
    };
    if (anchor || matchMedia('(prefers-reduced-motion: reduce)').matches) return finish();
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
      class:clear={!!anchor}
      role="presentation"
      transition:fade={{ duration: expanding ? 0 : 200 }}
      onpointerdown={() => browser.closeGlance()}
    ></div>
    <div
      class="frame"
      class:popup
      class:ext={!!anchor}
      class:morphing={!!morph}
      style:left="{box.left}px"
      style:top="{box.top}px"
      style:width="{box.w}px"
      style:height="{box.h}px"
      style:--morph-ms="{MORPH_MS}ms"
      in:scale={{ start: anchor ? 0.96 : 0.86, duration: anchor ? 160 : 350, opacity: 0, easing: springEase }}
      onintroend={() => scheduleLayout()}
      out:scale={{ start: 0.92, duration: expanding ? 0 : anchor ? 120 : 200, opacity: 0 }}
    >
      <div class="body" use:glancePane={tabId}>
        {#if !tab.live}<div class="placeholder"></div>{/if}
        {#if tab.loading}<span class="loading"></span>{/if}
      </div>
    </div>
    {#if !anchor}
    <div class="side" style:left="{frame.left + frame.w}px" style:top="{frame.top + 3}px" transition:fade={{ duration: expanding ? 0 : 160 }}>
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
    {/if}
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
  .backdrop.clear {
    background: transparent;
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
  /* Extension popups render on white like in Chrome (most assume a light page). */
  .frame.ext {
    border-radius: var(--radius-xl);
    background: #fff;
    transform-origin: 0 0;
  }
  .frame.ext .placeholder {
    background: #fff;
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
