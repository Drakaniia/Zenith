<script lang="ts">
  // The setup assistant's progress indicator: the sun climbs an arc from the horizon (the first
  // step) to the zenith (the last). Each step is a marker on the arc; steps already reached can
  // be revisited by clicking their marker.
  import { Tween } from 'svelte/motion';
  import { cubicInOut } from 'svelte/easing';
  import { onboarding } from './onboarding.svelte';
  import { STEPS, altitude, arcPoint } from './onboarding-flow';

  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const alt = new Tween(altitude(onboarding.index), { duration: reduced ? 0 : 720, easing: cubicInOut });
  $effect(() => {
    alt.target = altitude(onboarding.index);
  });

  const sun = $derived(arcPoint(alt.current));
  const marks = STEPS.map((s, i) => ({ ...s, i, p: arcPoint(altitude(i)) }));
  const atZenith = $derived(onboarding.index === STEPS.length - 1);

  // SVG in a 100 x 100 box stretched over the track: the arc is half an ellipse through the
  // zenith (50, 0); the lit part runs from the horizon to the sun.
  const lit = $derived(`M0 100 A50 100 0 0 1 ${sun.x * 100} ${sun.y * 100}`);
</script>

<div class="horizon" style:--p={alt.current / 90}>
  <div class="track">
    <div class="glow" style:left="{sun.x * 100}%" style:top="{sun.y * 100}%" aria-hidden="true"></div>
    <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
      <path class="day" d="M0 100 A50 100 0 0 1 100 100" vector-effect="non-scaling-stroke" />
      <path class="lit" d={lit} vector-effect="non-scaling-stroke" />
    </svg>

    <ol class="marks" aria-label="Setup steps">
      {#each marks as m (m.id)}
        {@const current = m.i === onboarding.index}
        {@const reachable = m.i <= onboarding.reached && !current}
        <li style:left="{m.p.x * 100}%" style:top="{m.p.y * 100}%">
          <button
            type="button"
            class="mark"
            class:done={m.i < onboarding.index || (m.i <= onboarding.reached && !current)}
            disabled={!reachable}
            aria-current={current ? 'step' : undefined}
            aria-label="{m.label}, step {m.i + 1} of {STEPS.length}{current ? ', current' : ''}"
            title={reachable ? `Back to ${m.label}` : m.label}
            onclick={() => onboarding.go(m.i)}
          >
            <span class="dot"></span>
          </button>
        </li>
      {/each}
    </ol>

    <div class="sun" class:zenith={atZenith} style:left="{sun.x * 100}%" style:top="{sun.y * 100}%" aria-hidden="true"></div>
  </div>
</div>

<style>
  /* The track is the arc's box: horizon along its bottom edge, zenith at the top centre. The
   * container clips at the horizon, so at 0° the sun is half risen. */
  .horizon {
    position: relative;
    height: 100%;
    padding: 18px 56px 0;
    overflow: hidden;
    border-bottom: 1px solid var(--line-strong);
  }
  .track {
    position: relative;
    height: 100%;
  }
  svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
    fill: none;
  }
  .day {
    stroke: var(--line-strong);
    stroke-width: 1;
  }
  .lit {
    stroke: color-mix(in srgb, var(--accent) 85%, var(--text) 15%);
    stroke-width: 2;
    stroke-linecap: round;
  }
  .glow {
    position: absolute;
    width: 320px;
    height: 200px;
    translate: -50% -50%;
    background: radial-gradient(closest-side, color-mix(in srgb, var(--accent) calc(18% + var(--p) * 22%), transparent), transparent);
    pointer-events: none;
  }
  .marks {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .marks li {
    position: absolute;
    translate: -50% -50%;
  }
  .mark {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    transition: background-color var(--fast) ease;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--dialog-bg);
    box-shadow: inset 0 0 0 1.5px var(--line-strong);
    transition:
      background-color 200ms ease,
      box-shadow 200ms ease,
      scale 160ms ease-out;
  }
  .mark.done .dot {
    background: color-mix(in srgb, var(--accent) 70%, var(--text) 30%);
    box-shadow: none;
  }
  .mark:not(:disabled):hover {
    background: var(--hover);
  }
  .mark:not(:disabled):hover .dot {
    scale: 1.3;
  }
  .mark:not(:disabled):active .dot {
    scale: 0.9;
  }
  .mark:disabled {
    pointer-events: none;
  }
  .sun {
    position: absolute;
    width: 20px;
    height: 20px;
    translate: -50% -50%;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 35%, color-mix(in srgb, var(--accent) 45%, white), var(--accent) 70%);
    box-shadow:
      0 0 0 5px color-mix(in srgb, var(--accent) 20%, transparent),
      0 0 26px 4px color-mix(in srgb, var(--accent) 55%, transparent);
    pointer-events: none;
  }
  /* Reaching the zenith: one soft ring, once. */
  .sun.zenith::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: 50%;
    box-shadow: 0 0 0 2px var(--accent);
    animation: ring 1100ms cubic-bezier(0.23, 1, 0.32, 1) 700ms both;
  }
  @keyframes ring {
    from {
      opacity: 0.7;
      scale: 1;
    }
    to {
      opacity: 0;
      scale: 3.2;
    }
  }
  @media (max-width: 640px) {
    .horizon {
      padding-inline: 28px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .sun.zenith::after {
      animation: none;
    }
  }
</style>
