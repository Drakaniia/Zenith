<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { extensions } from './extensions.svelte';
  import { tabsApi } from '$features/browser/api';
  import { extPopupPane, overlayLease, overlaySurface, scheduleLayout } from '$platform/layout.svelte';
  import { springEase } from '$shared/ui/motion';
  import type { PopupAnchor } from '$features/browser/types';

  // Dedicated floating card for extension toolbar popups (independent from Glance).
  // Auto-sizes to the popup's natural content (25x25 .. 800x600), like Chrome & Zen.
  const pop = $derived(extensions.activePopup);
  let lastAnchor: PopupAnchor = { x: 16, y: 44, below: true };
  let lastTabId = '';
  const tabId = $derived.by(() => (pop ? (lastTabId = pop.tabId) : lastTabId));
  const anchor = $derived.by(() => (pop ? (lastAnchor = pop.anchor) : lastAnchor));

  let fit = $state<{ id: string; w: number; h: number } | null>(null);
  const popLoaded = $derived(!!pop && !pop.loading);

  $effect(() => {
    const id = pop?.tabId;
    if (!id || !popLoaded) return;
    let stop = false;
    const measure = async () => {
      if (stop) return;
      const s = await tabsApi.tabContentSize(id).catch(() => null);
      if (stop || !s || s.x < 1 || s.y < 1) return;
      fit = { id, w: Math.min(800, Math.max(25, s.x)), h: Math.min(600, Math.max(25, s.y)) };
      scheduleLayout();
    };
    const timers = [0, 120, 350, 800, 1500].map((ms) => setTimeout(measure, ms));
    return () => {
      stop = true;
      timers.forEach(clearTimeout);
    };
  });

  const sized = $derived(fit && fit.id === tabId ? fit : null);
  const w = $derived(Math.min(sized?.w ?? 380, window.innerWidth - 16));
  const h = $derived(
    Math.min(
      sized?.h ?? 540,
      Math.max(120, anchor.below ? window.innerHeight - anchor.y - 8 : anchor.y - 8),
    ),
  );
  const left = $derived(Math.max(8, Math.min(anchor.x, window.innerWidth - w - 8)));
  const top = $derived(anchor.below ? anchor.y : Math.max(8, anchor.y - h));
</script>

{#if pop}
  <div class="root" use:overlayLease>
    <div
      class="backdrop"
      role="presentation"
      transition:fade={{ duration: 120 }}
      onpointerdown={() => extensions.closePopup()}
    ></div>
    <div
      class="card"
      data-ext-popup={pop.extId}
      use:overlaySurface={14}
      style:left="{left}px"
      style:top="{top}px"
      style:width="{w}px"
      style:height="{h}px"
      in:scale={{ start: 0.96, duration: 150, opacity: 0, easing: springEase }}
      onintroend={() => scheduleLayout()}
      out:scale={{ start: 0.94, duration: 110, opacity: 0 }}
    >
      <div class="body" use:extPopupPane={tabId}>
        {#if pop.loading}<span class="loading"></span>{/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 70;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    background: transparent;
  }
  .card {
    position: absolute;
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-xl);
    box-shadow: var(--zen-big-shadow);
    border: 1px solid var(--line-strong);
    overflow: hidden;
    background: #fff;
    transform-origin: top left;
  }
  .body {
    flex: 1;
    min-height: 0;
    position: relative;
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
    from { transform: translateX(-60%); }
    to { transform: translateX(60%); }
  }
</style>
