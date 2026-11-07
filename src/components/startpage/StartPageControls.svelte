<script lang="ts">
  import { scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import Icon from '../Icon.svelte';
  import StartPagePanel from './StartPagePanel.svelte';
  import { startPage } from '../../lib/startpage.svelte';
  import { HOVER_HIDE_MS } from '../../lib/startpage';

  // Hovering for 1 s clears the page until the pointer leaves the button.
  // Clicking opens the settings panel.
  let open = $state(false);


  let root: HTMLDivElement | undefined = $state();
  let button: HTMLButtonElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | undefined;

  function startHold(e: PointerEvent) {
    if (e.pointerType === 'touch' || !startPage.prefs.hoverToHide || open) return;
    clearTimeout(timer);
    timer = setTimeout(() => {
      startPage.cleared = true;
    }, HOVER_HIDE_MS);
  }

  function stopHold() {
    clearTimeout(timer);
    startPage.cleared = false;
  }

  function onclick() {
    stopHold();
    open = !open;
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

  $effect(() => () => stopHold());
</script>

<svelte:window onblur={stopHold} />

<div class="sp-controls" bind:this={root}>
  {#if open}
    <div class="panel-wrap" in:scale={{ start: 0.96, duration: 190, easing: cubicOut }} out:scale={{ start: 0.98, duration: 120 }}>
      <StartPagePanel onclose={() => close()} />
    </div>
  {/if}

  <button
    bind:this={button}
    type="button"
    class="fab"
    class:open
    aria-label="Customize start page"
    aria-expanded={open}
    aria-haspopup="dialog"
    title="Customize start page"
    onpointerenter={startHold}
    onpointerleave={stopHold}
    onpointercancel={stopHold}
    {onclick}
  >
    <Icon name="sliders" size={16} />
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
    background: transparent;
    border: none;
    box-shadow: none;
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

  .fab:focus-visible {
    outline: 2px solid var(--accent-button);
    outline-offset: 2px;
  }

</style>
