<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { browser } from '../lib/browser.svelte';
  import { glancePane, overlayLease, scheduleLayout } from '../lib/layout.svelte';
  import { copyText } from '../lib/actions';
  import { ui } from '../lib/ui.svelte';
  import { springEase } from '../lib/motion';

  // Zen's Glance (src/zen/glance): a page previewed in a card over the current tab, 80 % of the
  // content area wide, with round action buttons beside it. Opens with a 350 ms spring.
  const g = $derived(browser.glance);
  const tab = $derived(g ? browser.tabs[g.tabId] : undefined);

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
    const popup = g?.popup;
    const w = Math.round(area.width * (popup ? 0.55 : 0.8));
    const h = Math.round(popup ? area.height * 0.84 : area.height);
    return { left: Math.round(area.left + (area.width - w) / 2), top: Math.round(area.top + (area.height - h) / 2), w, h };
  });
</script>

{#if g && tab}
  <div class="root" use:overlayLease>
    <div class="backdrop" role="presentation" transition:fade={{ duration: 200 }} onpointerdown={() => browser.closeGlance()}></div>
    <div
      class="frame"
      class:popup={g.popup}
      style:left="{frame.left}px"
      style:top="{frame.top}px"
      style:width="{frame.w}px"
      style:height="{frame.h}px"
      in:scale={{ start: 0.86, duration: 350, opacity: 0, easing: springEase }}
      onintroend={() => scheduleLayout()}
      out:scale={{ start: 0.92, duration: 200, opacity: 0 }}
    >
      <div class="body" use:glancePane={g.tabId}>
        {#if !tab.live}<div class="placeholder"></div>{/if}
        {#if tab.loading}<span class="loading"></span>{/if}
      </div>
    </div>
    <div class="side" style:left="{frame.left + frame.w}px" style:top="{frame.top + 3}px" transition:fade={{ duration: 160 }}>
      <button class="action close" title="Close (Esc)" onclick={() => browser.closeGlance()}><Icon name="close" size={16} /></button>
      <button class="action" title="Open as tab" onclick={() => browser.expandGlance()}><Icon name="expand" size={16} /></button>
      {#if g.openerId && browser.tabs[g.openerId]}
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
