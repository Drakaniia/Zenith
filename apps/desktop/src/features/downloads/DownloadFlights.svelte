<!-- Files flying into the downloads button as their downloads start (flights.svelte.ts). -->
<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { layoutApplied } from '$platform/layout.svelte';
  import { downloadsTarget, flights, pulseDownloads, type Flight } from './flights.svelte';
  import { CHIP, flightFrames, flightMs } from './flight';

  function fly(node: HTMLElement, f: Flight) {
    let anim: Animation | undefined;
    let gone = false;
    // The chrome goes above the pages first, or the file would start hidden under its page.
    void layoutApplied().then(() => {
      if (gone) return;
      const to = downloadsTarget();
      const keyframes = flightFrames(f.from, to).map((k) => ({
        offset: k.offset,
        opacity: k.opacity,
        transform: `translate(${k.x - CHIP.w / 2}px, ${k.y - CHIP.h / 2}px) rotate(${k.rotate}deg) scale(${k.scale})`,
        filter: `drop-shadow(0 ${2 + 12 * k.lift}px ${3 + 9 * k.lift}px rgba(0, 0, 0, ${0.2 + 0.2 * k.lift}))`,
      }));
      anim = node.animate(keyframes, { duration: flightMs(f.from, to), easing: 'linear', fill: 'forwards' });
      anim.onfinish = () => {
        pulseDownloads();
        flights.land(f.id);
      };
      anim.oncancel = () => flights.land(f.id);
    });
    return {
      destroy() {
        gone = true;
        anim?.cancel();
      },
    };
  }
</script>

{#if flights.list.length}
  <div class="layer" aria-hidden="true">
    {#each flights.list as f (f.id)}
      <div class="file" style:width="{CHIP.w}px" style:height="{CHIP.h}px" use:fly={f}>
        <span class="fold"></span>
        <span class="glyph" style:color={f.color}><Icon name={f.icon} size={18} /></span>
        {#if f.label}<span class="ext">{f.label}</span>{/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .layer {
    position: fixed;
    inset: 0;
    z-index: 110;
    pointer-events: none;
    overflow: hidden;
  }
  /* A sheet of paper with its top-right corner folded over. */
  .file {
    position: absolute;
    left: 0;
    top: 0;
    opacity: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    border-radius: 7px;
    background: linear-gradient(225deg, transparent 0 9px, var(--surface-2) 9px);
    color: var(--text-2);
    transform-origin: 50% 50%;
    will-change: transform, opacity, filter;
  }
  .fold {
    position: absolute;
    top: 0;
    right: 0;
    width: 13px;
    height: 13px;
    border-bottom-left-radius: 3px;
    background: linear-gradient(225deg, transparent 50%, color-mix(in srgb, var(--text) 24%, var(--surface-2)) 50%);
  }
  .glyph {
    display: grid;
  }
  .ext {
    max-width: 38px;
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--accent-button);
    color: var(--accent-contrast);
    font: 700 8.5px/1.2 var(--font);
    letter-spacing: 0.04em;
    overflow: hidden;
  }
</style>
