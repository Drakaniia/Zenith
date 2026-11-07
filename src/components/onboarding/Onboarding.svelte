<script lang="ts">
  // The setup assistant: a card over the live window on first launch. What the user picks is
  // applied immediately, so the window behind the card is the preview. See onboarding.svelte.ts
  // for the state and lib/onboarding-flow.ts for the steps.
  import { tick, type Component } from 'svelte';
  import { fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { onboarding } from '../../lib/onboarding.svelte';
  import { STEPS, altitude, type StepId } from '../../lib/onboarding-flow';
  import { overlayLease } from '../../lib/layout.svelte';
  import WindowControls from '../WindowControls.svelte';
  import Horizon from './Horizon.svelte';
  import Welcome from './steps/Welcome.svelte';
  import Appearance from './steps/Appearance.svelte';
  import Search from './steps/Search.svelte';
  import StartPage from './steps/StartPage.svelte';
  import Import from './steps/Import.svelte';
  import Privacy from './steps/Privacy.svelte';
  import Ready from './steps/Ready.svelte';

  const VIEWS: Record<StepId, { view: Component; title: string; lede: string }> = {
    welcome: {
      view: Welcome,
      title: 'Welcome to Zenith',
      lede: "A minute of setup and it's yours. Everything you choose here can be changed later in Settings.",
    },
    appearance: {
      view: Appearance,
      title: 'Make it look like yours',
      lede: 'The window behind this card changes as you choose.',
    },
    search: {
      view: Search,
      title: 'Choose a search engine',
      lede: "Used when you type something into the address bar that isn't an address.",
    },
    start: {
      view: StartPage,
      title: 'Decide what opens first',
      lede: 'What you see each time Zenith starts.',
    },
    import: {
      view: Import,
      title: 'Bring your bookmarks and history',
      lede: 'Copy them from another browser on this PC.',
    },
    privacy: {
      view: Privacy,
      title: 'Set your privacy defaults',
      lede: 'Protection is already on. Adjust anything you like.',
    },
    ready: {
      view: Ready,
      title: 'Zenith is ready',
      lede: 'Here is what you chose. Change anything now, or later in Settings.',
    },
  };

  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const step = $derived(onboarding.step);
  const meta = $derived(VIEWS[step.id]);
  const first = $derived(onboarding.index === 0);
  const last = $derived(onboarding.index === STEPS.length - 1);
  const primary = $derived(first ? 'Get started' : last ? 'Start browsing' : 'Continue');
  let title: HTMLElement | undefined = $state();
  let body: HTMLElement | undefined = $state();

  // Each new step starts at the top, with focus on its heading so screen readers announce it.
  $effect(() => {
    void onboarding.index;
    void tick().then(() => {
      body?.scrollTo({ top: 0 });
      title?.focus({ preventScroll: true });
    });
  });

  /** New steps slide in from the side they came from. */
  function enter(_node: Element, { dir }: { dir: number }) {
    return {
      duration: reduced ? 0 : 280,
      easing: cubicOut,
      css: (t: number, u: number) => `opacity: ${t}; transform: translateX(${u * dir * 20}px)`,
    };
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!onboarding.hold) onboarding.next();
  }
</script>

{#if onboarding.open}
  <div class="fre" use:overlayLease transition:fade={{ duration: reduced ? 0 : 220, easing: cubicOut }}>
    <div class="caption" data-tauri-drag-region>
      <WindowControls />
    </div>

    <div class="card" role="dialog" aria-modal="true" aria-labelledby="fre-title" aria-describedby="fre-lede">
    <!-- novalidate: steps validate their own fields (a bare host is a valid home page). -->
    <form class="frame" novalidate onsubmit={submit}>
      <header class="sky">
        <Horizon />
      </header>

      <div class="body" bind:this={body}>
        {#key onboarding.index}
          <div class="step" class:centered={first} in:enter={{ dir: onboarding.dir }}>
            <p class="eyebrow">
              <span class="alt" aria-hidden="true">{Math.round(altitude(onboarding.index))}°</span>
              <span aria-hidden="true">{last ? 'Zenith' : step.label}</span>
              <span class="sr">Step {onboarding.index + 1} of {STEPS.length}: {step.label}</span>
            </p>
            <h2 id="fre-title" class:hero={first} tabindex="-1" bind:this={title}>{meta.title}</h2>
            <p id="fre-lede" class="lede">{meta.lede}</p>
            <div class="content">
              <meta.view />
            </div>
          </div>
        {/key}
      </div>

      <footer class="nav">
        {#if !first}
          <button type="button" class="btn ghost" onclick={() => onboarding.back()}>Back</button>
        {/if}
        <span class="hold" role="status">{onboarding.hold}</span>
        {#if step.optional}
          <button type="button" class="btn ghost" onclick={() => onboarding.skip()}>Skip</button>
        {/if}
        <button type="submit" class="btn primary go" disabled={!!onboarding.hold}>{primary}</button>
      </footer>
    </form>
    </div>
  </div>
{/if}

<style>
  .fre {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    padding: 40px 24px 24px;
    background: color-mix(in srgb, var(--tint-solid) 50%, transparent);
  }
  .caption {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 32px;
    display: flex;
    justify-content: flex-end;
  }

  .card {
    position: relative;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    width: min(760px, 100%);
    height: min(680px, 100%);
    margin: 0;
    overflow: hidden;
    border-radius: var(--radius-xl);
    corner-shape: var(--sq);
    background: var(--dialog-bg);
    box-shadow:
      0 0 0 1px var(--line-strong),
      var(--shadow-lg);
    animation: arrive 520ms cubic-bezier(0.23, 1, 0.32, 1) both;
  }
  /* The form only provides Enter-to-continue; the card lays out its parts. */
  .frame {
    display: contents;
  }
  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.98);
    }
  }

  /* The sky deepens as the sun climbs (--p on .horizon runs 0 to 1). */
  .sky {
    height: 124px;
    background: linear-gradient(to bottom, color-mix(in srgb, var(--accent) 10%, transparent), transparent);
  }

  .body {
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 24px 48px 28px;
    scrollbar-gutter: stable;
  }
  /* The welcome step is short: it sits in the middle of the card, slightly above centre. */
  .step.centered {
    min-height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding-bottom: 6%;
  }
  .eyebrow {
    display: flex;
    gap: 8px;
    margin: 0 0 8px;
    font: 500 11px/1 var(--mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .alt {
    color: var(--accent-fg);
    font-variant-numeric: tabular-nums;
    min-width: 3ch;
  }
  h2 {
    margin: 0;
    font: 600 26px/1.15 var(--font-display);
    letter-spacing: -0.015em;
    outline: none;
    text-wrap: balance;
  }
  h2.hero {
    font-size: 40px;
    font-weight: 600;
    letter-spacing: -0.025em;
  }
  .lede {
    margin: 8px 0 0;
    max-width: 58ch;
    color: var(--text-2);
    font-size: 14px;
    line-height: 1.5;
    text-wrap: pretty;
  }
  .content {
    margin-top: 22px;
  }

  .nav {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
  }
  .hold {
    flex: 1;
    min-width: 0;
    text-align: right;
    font-size: 12px;
    color: var(--text-3);
  }
  .nav .btn {
    height: 34px;
  }
  .go {
    min-width: 118px;
    justify-content: center;
  }
  .btn:active {
    scale: 0.97;
  }

  /* Shared by the steps. */
  .fre :global(fieldset) {
    margin: 0 0 22px;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  .fre :global(legend) {
    display: block;
    margin-bottom: 10px;
    padding: 0;
    font-weight: 600;
    font-size: 13px;
  }
  .fre :global(legend .hint) {
    display: block;
    margin-top: 2px;
    font-weight: 400;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .fre :global(.grid) {
    display: grid;
    gap: 8px;
  }
  .fre :global(.grid.two) {
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  }
  .fre :global(.grid.three) {
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
  }
  .fre :global(.rows) {
    display: grid;
    border-radius: var(--radius-lg);
    corner-shape: var(--sq);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .fre :global(.rows .row) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 14px;
  }
  .fre :global(.rows .row + .row) {
    border-top: 1px solid var(--line);
  }
  .fre :global(.rows .text) {
    display: grid;
    gap: 2px;
  }
  .fre :global(.rows .text b) {
    font-weight: 600;
    font-size: 13.5px;
  }
  .fre :global(.rows .text span) {
    color: var(--text-2);
    font-size: 12.5px;
  }
  .fre :global(.note) {
    margin: 14px 0 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .fre :global(.link) {
    color: var(--accent-fg);
    font-weight: 500;
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, currentColor 35%, transparent);
    text-underline-offset: 3px;
    border-radius: 3px;
  }
  .fre :global(.link:hover) {
    text-decoration-color: currentColor;
  }
  .fre :global(.sr) {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  /* Small windows: the card fills the window. */
  @media (max-width: 640px), (max-height: 560px) {
    .fre {
      padding: 32px 0 0;
    }
    .card {
      width: 100%;
      height: 100%;
      border-radius: 0;
      box-shadow: none;
      border-top: 1px solid var(--line);
    }
    .body {
      padding: 20px 22px 24px;
    }
    h2.hero {
      font-size: 32px;
    }
  }
  @media (max-height: 560px) {
    .sky {
      height: 84px;
    }
  }
</style>
