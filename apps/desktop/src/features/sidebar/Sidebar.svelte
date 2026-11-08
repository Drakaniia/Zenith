<script lang="ts">
  import Favicon from '$shared/ui/Favicon.svelte';
  import UrlPill from '$features/toolbar/UrlPill.svelte';
  import SidebarTop from './SidebarTop.svelte';
  import Essentials from './Essentials.svelte';
  import SpacePane from './SpacePane.svelte';
  import MediaBar from './MediaBar.svelte';
  import SidebarFooter from './SidebarFooter.svelte';
  import { slide } from 'svelte/transition';
  import { onDestroy, untrack } from 'svelte';
  import { browser, tabTitle } from '$features/browser/browser.svelte';
  import { dnd } from './sidebar-dnd.svelte';
  import { settings, SIDEBAR_COLLAPSE_AT, SIDEBAR_DEFAULT, SIDEBAR_MAX, SIDEBAR_MIN } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { slideX } from '$shared/ui/motion';
  import { shortcutHint } from '$features/shortcuts/hints';
  import { sidebarMenu } from '../../app/menus';

  // Zen's sidebar (#navigator-toolbox): top buttons, address bar (single toolbar), Essentials,
  // the active space (sliding between spaces), media controls and the footer.
  let { floating = false }: { floating?: boolean } = $props();

  const mode = $derived(settings.layoutMode);
  /** On the right the splitter is on the sidebar's left edge, and dragging left widens it. */
  const right = $derived(settings.value.sidebarPosition === 'right');
  const wantCollapsed = $derived(mode === 'collapsed' && !floating);
  const single = $derived(mode === 'single');

  // Collapsing is two steps so the text never blinks away: the labels fade out while the sidebar
  // narrows (the width follows the layout transition), and only then do the rows switch to their
  // icon-only form. Expanding does the opposite: the rows get their labels back, which fade in.
  const SWAP_MS = 230;
  const initial = untrack(() => wantCollapsed);
  let collapsed = $state(initial);
  let fading = $state(false);
  let lastWant = initial;
  let phaseTimer: ReturnType<typeof setTimeout> | undefined;
  $effect.pre(() => {
    const want = wantCollapsed;
    if (want === lastWant) return;
    lastWant = want;
    clearTimeout(phaseTimer);
    if (want) {
      fading = true;
      phaseTimer = setTimeout(() => {
        collapsed = true;
        fading = false;
      }, SWAP_MS);
    } else {
      collapsed = false;
      fading = true;
      requestAnimationFrame(() => requestAnimationFrame(() => (fading = false)));
    }
  });
  onDestroy(() => clearTimeout(phaseTimer));
  const space = $derived(browser.activeSpace);

  // ---------------------------------------------------------------- space slide
  // Zen slides the whole space sideways on a switch (a zero-bounce 250 ms spring), taking the
  // shorter way round when navigation wraps.
  let lastIndex = 0;
  let dir = $state(1);
  $effect.pre(() => {
    const n = browser.spaces.length;
    const i = browser.spaces.findIndex((s) => s.id === browser.activeSpaceId);
    let diff = i - lastIndex;
    if (diff > Math.floor(n / 2)) diff -= n;
    else if (diff < -Math.floor(n / 2)) diff += n;
    dir = diff < 0 ? -1 : 1;
    lastIndex = i;
  });

  // The direction is read when each transition starts, so in and out agree with this switch.

  // ---------------------------------------------------------------- swipe between spaces
  // Horizontal trackpad scrolling (or Ctrl + wheel, Zen's scroll modifier) switches spaces; the
  // space follows the gesture a little before it commits.
  let swipe = $state(0);
  let swipeLock = 0;
  let swipeReset: ReturnType<typeof setTimeout> | undefined;
  function onwheel(e: WheelEvent) {
    const horizontal = Math.abs(e.deltaX) > Math.abs(e.deltaY) * 1.2;
    if (!horizontal && !e.ctrlKey) return;
    if (e.ctrlKey) e.preventDefault();
    if (Date.now() < swipeLock) return;
    const delta = horizontal ? e.deltaX : e.deltaY;
    swipe += delta;
    clearTimeout(swipeReset);
    swipeReset = setTimeout(() => (swipe = 0), 160);
    const threshold = e.ctrlKey && !horizontal ? 1 : 110;
    if (Math.abs(swipe) >= threshold && browser.spaces.length > 1) {
      browser.cycleSpace(swipe > 0 ? 1 : -1);
      swipe = 0;
      swipeLock = Date.now() + 320;
    }
  }
  const nudge = $derived(Math.max(-28, Math.min(28, -swipe / 4)));

  // ---------------------------------------------------------------- resizing (Zen splitter)
  let resizing = $state(false);
  function startResize(e: PointerEvent) {
    if (e.button !== 0) return;
    const startX = e.clientX;
    const el = e.currentTarget as HTMLElement;
    // The drawn width, which a narrow window may have clamped below the saved one.
    const startW = el.parentElement?.offsetWidth ?? settings.value.sidebarWidth;
    el.setPointerCapture(e.pointerId);
    resizing = true;
    let collapsedToCompact = false;
    const move = (ev: PointerEvent) => {
      const w = startW + (right ? startX - ev.clientX : ev.clientX - startX);
      if (w < SIDEBAR_COLLAPSE_AT && !settings.value.compact) {
        // Dragging the edge almost closed hides the sidebar: Zen turns on compact mode.
        collapsedToCompact = true;
        settings.value.sidebarWidth = startW;
        settings.update({ compact: true });
        if (!settings.value.compactHintSeen) {
          settings.update({ compactHintSeen: true });
          ui.toast(`Sidebar hidden — press ${shortcutHint('toggleCompact') ?? 'the compact mode shortcut'} to bring it back`, { icon: 'sidebar', timeout: 5000 });
        }
        up();
        return;
      }
      settings.value.sidebarWidth = Math.round(Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, w)));
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      resizing = false;
      if (!collapsedToCompact) settings.save(false);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }
</script>

<aside
  class="sidebar"
  class:floating
  class:collapsed
  class:fading
  class:single
  class:has-top={!collapsed}
  class:right
  {onwheel}
  oncontextmenu={(e) => {
    // Tabs, spaces and folders open their own menus; the rest of the sidebar opens this one.
    if (e.defaultPrevented || (e.target as HTMLElement).closest('input, textarea')) return;
    e.preventDefault();
    ui.openMenu(e.clientX, e.clientY, sidebarMenu());
  }}
>
  {#if !collapsed}
    <div class="top-wrap" transition:slide={{ duration: 200 }}>
      <SidebarTop {single} />
    </div>
  {/if}
  {#if single && !collapsed}
    <UrlPill />
  {/if}

  <Essentials {collapsed} />

  <div class="space-area" style:transform={nudge ? `translateX(${nudge}px)` : undefined}>
    {#if space}
      {#key space.id}
        <div class="slot" in:slideX={{ from: dir }} out:slideX={{ from: -dir }}>
          <SpacePane {space} {collapsed} />
        </div>
      {/key}
    {/if}
  </div>

  {#if !collapsed}<MediaBar />{/if}
  <SidebarFooter {collapsed} />

  {#if !floating && !collapsed}
    <div
      class="splitter"
      class:resizing
      role="separator"
      aria-orientation="vertical"
      title="Drag to resize, double-click to reset"
      onpointerdown={startResize}
      ondblclick={() => settings.update({ sidebarWidth: SIDEBAR_DEFAULT })}
    ></div>
  {/if}
</aside>

{#if dnd.drag?.started && browser.tabs[dnd.drag.id]}
  {@const t = browser.tabs[dnd.drag.id]}
  <div class="ghost" style:transform="translate({dnd.drag.x + 12}px, {dnd.drag.y + 8}px)">
    <Favicon url={t.url} favicon={t.favicon} />
    <span class="ellipsis">{tabTitle(t)}</span>
  </div>
{/if}

<style>
  .sidebar {
    position: relative;
    width: var(--sidebar-w);
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding-bottom: var(--zen-pad);
    color: var(--text);
  }
  .sidebar.collapsed {
    padding-top: 2px;
    padding-bottom: 6px;
  }
  /* The collapsed layout narrows the sidebar; the width eases with the page while the layout
   * animates (App.svelte), like the rest of the layout variables. */
  :global(.app.collapsed-mode) .sidebar:not(.floating) {
    width: calc(var(--collapsed-tab-w) + 12px);
  }
  :global(.app.animating) .sidebar:not(.floating) {
    transition: width 300ms var(--zen-spring);
  }
  .top-wrap {
    flex: none;
    display: flow-root;
  }
  /* Labels fade instead of blinking when the sidebar collapses or expands. */
  .sidebar :global(.labels),
  .sidebar :global(.space-name),
  .sidebar :global(.new-tab-label),
  .sidebar :global(.folder .name) {
    transition: opacity 150ms ease;
  }
  .sidebar.fading :global(.labels),
  .sidebar.fading :global(.space-name),
  .sidebar.fading :global(.new-tab-label),
  .sidebar.fading :global(.folder .name) {
    opacity: 0;
  }
  .sidebar.floating {
    width: 100%;
    height: 100%;
  }
  .space-area {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    transition: transform 0.2s var(--ease-out);
  }
  .slot {
    position: absolute;
    inset: 0;
  }

  /* Zen's splitter: invisible until hovered for a moment, then a soft accent bar. */
  .splitter {
    position: absolute;
    top: var(--zen-sep);
    bottom: var(--zen-sep);
    right: -3px;
    width: 6px;
    z-index: 4;
    cursor: ew-resize;
    border-radius: 4px;
    background: var(--accent-button);
    opacity: 0;
    transition: opacity 0.2s ease-in-out;
  }
  .splitter:hover,
  .splitter.resizing {
    opacity: 0.8;
    transition-delay: 0.2s;
  }
  .splitter.resizing {
    transition-delay: 0s;
  }
  .sidebar.right .splitter {
    right: auto;
    left: -3px;
  }

  .ghost {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 240px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    background: var(--tab-selected);
    backdrop-filter: blur(20px);
    box-shadow: var(--zen-big-shadow);
    pointer-events: none;
    font-size: 13px;
  }
</style>
