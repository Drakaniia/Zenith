<script lang="ts">
  import { scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import Icon from '../Icon.svelte';
  import StartPagePanel from './StartPagePanel.svelte';
  import { startPage } from '../../lib/startpage.svelte';
  import { HOVER_HIDE_MS } from '../../lib/startpage';

  // The start page's small settings button. Holding the pointer over it for 3 s clears the page
  // (a ring fills while it waits); clicking it opens the settings panel, or brings the page back.
  let open = $state(false);
  let holding = $state(false);
  let hint = $state(false);
  let root: HTMLDivElement | undefined = $state();
  let button: HTMLButtonElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | undefined;

  function startHold(e: PointerEvent) {
    if (e.pointerType === 'touch' || !startPage.prefs.hoverToHide || open) return;
    clearTimeout(timer);
    holding = true;
    timer = setTimeout(() => {
      holding = false;
      startPage.cleared = !startPage.cleared;
    }, HOVER_HIDE_MS);
  }

  function stopHold() {
    clearTimeout(timer);
    holding = false;
  }

  function onclick() {
    stopHold();
    if (startPage.cleared) startPage.cleared = false;
    else open = !open;
  }

  function close(refocus = true) {
    open = false;
    if (refocus) button?.focus();
  }

  // Outside clicks and Escape close the panel.
  $effect(() => {
    if (!open) return;
    const down = (e: PointerEvent) => {
      if (root && !root.contains(e.target as Node)) close(false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.stopPropagation();
      close();
    };
    window.addEventListener('pointerdown', down, true);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointerdown', down, true);
      window.removeEventListener('keydown', key, true);
    };
  });

  // After the page clears, say once how to bring it back.
  $effect(() => {
    if (!startPage.cleared) {
      hint = false;
      return;
    }
    hint = true;
    const t = setTimeout(() => (hint = false), 3800);
    return () => clearTimeout(t);
  });

  $effect(() => () => clearTimeout(timer));
</script>

<div class="sp-controls" bind:this={root}>
  {#if open}
    <div class="panel-wrap" in:scale={{ start: 0.96, duration: 190, easing: cubicOut }} out:scale={{ start: 0.98, duration: 120 }}>
      <StartPagePanel onclose={() => close()} />
    </div>
  {/if}
  {#if hint}
    <p class="hint" role="status" in:scale={{ start: 0.97, duration: 180, easing: cubicOut }} out:scale={{ start: 0.98, duration: 160 }}>
      Click anywhere or press Esc to bring everything back
    </p>
  {/if}
  <button
    bind:this={button}
    type="button"
    class="fab"
    class:holding
    class:open
    class:quiet={startPage.cleared}
    aria-label={startPage.cleared ? 'Show search and sites' : 'Customize start page'}
    aria-expanded={open}
    aria-haspopup="dialog"
    title={startPage.cleared ? 'Show search and sites' : 'Customize start page'}
    onpointerenter={startHold}
    onpointerleave={stopHold}
    {onclick}
  >
    <svg class="ring" viewBox="0 0 40 40" aria-hidden="true"><circle cx="20" cy="20" r="18.25" /></svg>
    <Icon name={startPage.cleared ? 'eye' : 'sliders'} size={16} />
  </button>
</div>

<style>
  .sp-controls {
    position: absolute;
    right: 16px;
    bottom: 16px;
    z-index: 5;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 10px;
  }
  .panel-wrap {
    transform-origin: bottom right;
  }
  .fab {
    position: relative;
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text-2);
    background: color-mix(in srgb, var(--dialog-bg) 78%, transparent);
    border: 1px solid var(--line-strong);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.16);
    opacity: 0.72;
    transition:
      opacity 160ms ease,
      color 160ms ease,
      transform 140ms var(--ease-out);
  }
  .fab:hover,
  .fab:focus-visible,
  .fab.open {
    opacity: 1;
    color: var(--text);
  }
  .fab:active {
    transform: scale(0.94);
  }
  .fab.quiet:not(:hover):not(:focus-visible) {
    opacity: 0.45;
  }
  .fab:focus-visible {
    outline: 2px solid var(--accent-button);
    outline-offset: 2px;
  }
  /* Progress ring for the 3 s hold. It starts after a short pause so passing over the button
   * doesn't flash it, fills linearly (it measures time), and snaps back fast on leave. */
  .ring {
    position: absolute;
    inset: -3px;
    width: calc(100% + 6px);
    height: calc(100% + 6px);
    rotate: -90deg;
    pointer-events: none;
  }
  .ring circle {
    fill: none;
    stroke: var(--accent-button);
    stroke-width: 2;
    stroke-linecap: round;
    stroke-dasharray: 114.7;
    stroke-dashoffset: 114.7;
    opacity: 0;
    transition:
      stroke-dashoffset 180ms var(--ease-out),
      opacity 120ms ease;
  }
  .fab.holding .ring circle {
    stroke-dashoffset: 0;
    opacity: 1;
    transition:
      stroke-dashoffset 2600ms linear 400ms,
      opacity 120ms ease 400ms;
  }
  .hint {
    margin: 0;
    padding: 7px 12px;
    border-radius: 9px;
    font-size: 12px;
    color: var(--text-2);
    background: color-mix(in srgb, var(--dialog-bg) 86%, transparent);
    border: 1px solid var(--line);
    box-shadow: var(--zen-big-shadow);
    transform-origin: bottom right;
  }
  @media (prefers-reduced-motion: reduce) {
    .fab.holding .ring circle {
      transition: opacity 120ms ease 400ms;
      stroke-dasharray: none;
      stroke-dashoffset: 0;
    }
  }
</style>
