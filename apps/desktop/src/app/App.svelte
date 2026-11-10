<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { tabsApi } from '$features/browser/api';
  import { windowApi } from '$features/window/api';
  import { browser } from '$features/browser/browser.svelte';
  import { settings, SIDEBAR_MIN } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { barPinned } from '$features/bookmarks/bookmark-actions';
  import { bmDnd } from '$features/bookmarks/bookmark-dnd.svelte';
  import { followLayout, overlay, scheduleLayout, setLayoutRegistry } from '$platform/layout.svelte';
  import { incognito } from '$features/window/window.svelte';
  import { isStrip } from '$features/sidebar/sidebar-position';
  import Content from '$features/browser/Content.svelte';
  import { extensions } from '$features/extensions/extensions.svelte';
  import Onboarding from '$features/onboarding/OnboardingDialog.svelte';
  import { onboarding } from '$features/onboarding/onboarding.svelte';
  import { notes } from '$features/notes/notes.svelte';
  import SpaceTint from './SpaceTint.svelte';
  import ChromeSlots from './ChromeSlots.svelte';
  import CaptionReveal from './CaptionReveal.svelte';
  import CompactReveal from './CompactReveal.svelte';
  import Overlays from './Overlays.svelte';
  import { listenBackend } from './events';
  import { startup } from './startup';
  import { followMs, layoutMotion, sameLayout, STILL, type LayoutMotion, type LayoutState } from './layout-motion';

  setLayoutRegistry({
    isLive: (id) => {
      const t = browser.tabs[id];
      return !!t && t.live && !t.crashed;
    },
    isReady: (id) => !browser.tabs[id]?.creating,
  });

  const compact = $derived(settings.value.compact);
  const pageFullscreen = $derived(!!browser.fullscreenTabId);
  const mode = $derived(settings.layoutMode);
  /** The window edge the sidebar is on; at the top or bottom it is a horizontal tab strip. */
  const side = $derived(settings.value.sidebarPosition);
  const strip = $derived(isStrip(side));
  /** Zen's single toolbar: no toolbar above the page; window controls appear at the top edge. */
  const single = $derived(mode === 'single');
  const showSidebar = $derived(!compact && !pageFullscreen);
  /** Compact mode with the toolbar hidden too (Ctrl+Alt+T): it floats in from the top edge. */
  const hideToolbar = $derived(compact && settings.value.compactHideToolbar && !single && !pageFullscreen);
  /** A tab strip at the top is the window's top row, so it holds the window controls. */
  const stripHasControls = $derived(side === 'top' && showSidebar);
  /** The strip at the top is also the toolbar (navigation, address bar, menus): one row, no
   *  toolbar below it. */
  const stripIsToolbar = $derived(stripHasControls);
  const showTopBar = $derived(!single && !pageFullscreen && !hideToolbar && !stripIsToolbar);
  /** The bookmarks toolbar has a row of its own. In the single-toolbar layout, when it hasn't, it
   *  still appears next to the window controls while the pointer is at the top edge. */
  const barOn = $derived(!pageFullscreen && barPinned());
  const revealBar = $derived(single && !barOn && settings.value.bookmarksBar !== 'never');
  const space = $derived(browser.activeSpace);

  // The page keeps a usable width on narrow windows: the sidebar gives up width first (down to
  // its minimum), so the toolbar above the page always has room for its menu and window controls.
  const MIN_PAGE_W = { single: 280, multiple: 400, collapsed: 0 } as const;
  let winW = $state(window.innerWidth);
  const sideW = $derived(Math.max(SIDEBAR_MIN, Math.min(settings.value.sidebarWidth, winW - MIN_PAGE_W[mode])));

  // Layout changes (compact mode, single/multiple toolbars, collapsed sidebar) animate: the
  // layout variables in app.css transition while the page webviews follow frame by frame.
  // Compact mode moves on Zen's timing (layout-motion.ts).
  let motion = $state<LayoutMotion>(STILL);
  const animating = $derived(motion.side > 0 || motion.bar > 0);
  let motionReady = false;
  let lastLayout: LayoutState | null = null;
  let motionRevision = 0;
  $effect.pre(() => {
    const next: LayoutState = { compact, mode, barOn, hideToolbar, side };
    const prev = lastLayout;
    if (prev && sameLayout(prev, next)) return;
    lastLayout = next;

    if (!prev || !motionReady || pageFullscreen) return;
    // The floating sidebar is still flagged open here when compact mode was left from it.
    const m = layoutMotion(prev, next, untrack(() => ui.flyout));
    motion = m;
    const revision = ++motionRevision;
    const ms = followMs(m);
    if (!ms) return;

    void followLayout(ms).then(() => {
      if (revision === motionRevision) motion = STILL;
    });
  });


  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let lastSaved = '';

  $effect(() => {
    document.documentElement.style.setProperty('--accent', browser.activeSpace?.color ?? '#8b7cf6');
  });

  // Session autosave: written atomically by the backend, debounced. Incognito windows keep
  // their tabs in memory only.
  $effect(() => {
    if (!browser.restored || incognito) return;
    const json = browser.snapshot();
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      if (json !== lastSaved) {
        lastSaved = json;
        void tabsApi.sessionSave(json);
      }
    }, 400);
  });

  // Anything that moves page webviews schedules a layout pass.
  $effect(() => {
    void browser.visibleIds.join();
    void browser.glance?.tabId;
    void settings.value.compact;
    void settings.value.sidebarWidth;
    void settings.value.toolbarMode;
    void settings.value.sidebarPosition;
    void hideToolbar;
    void barOn;
    void ui.find.open;
    void pageFullscreen;
    void overlay.holds;
    void overlay.forced;
    scheduleLayout();
  });

  $effect(() => {
    overlay.forced = ui.dragging || bmDnd.active;
  });

  // The window tint is painted by the chrome, which sits below the page webviews, except while
  // floating UI raises it above them. It is cut out under the live pages, so they keep their
  // own colours (no dimming) whether or not a menu, popup or the compact sidebar is open.
  // One mask layer per page rectangle; the window tint keeps everything but them (the full
  // layer excludes their union).
  const paneMask = $derived({
    image: overlay.panes.map(() => 'linear-gradient(#000 0 0)').join(', '),
    position: overlay.panes.map((r) => `${r.x}px ${r.y}px`).join(', '),
    size: overlay.panes.map((r) => `${r.w}px ${r.h}px`).join(', '),
  });
  const hasPanes = $derived(overlay.panes.length > 0);

  // Close the find bar when its tab goes away.
  $effect(() => {
    if (ui.find.open && !browser.tabs[ui.find.tabId]?.live) ui.find.open = false;
  });

  /** The window is closing: save the session, then let the backend close it (or quit when it
   *  is the last window). */
  async function flushAndClose() {
    clearTimeout(saveTimer);
    try {
      await notes.flush().catch(() => {});
      if (!incognito) await tabsApi.sessionSave(browser.snapshot());
    } finally {
      await windowApi.windowClose();
    }
  }

  // The floating sidebar goes with compact mode. Leaving compact mode keeps it flagged until the
  // layout motion above has seen it, so it docks where it is, as in Zen.
  $effect(() => {
    if (compact) return;
    ui.flyout = false;
    ui.flyoutPinned = false;
  });

  $effect(() => {
    if (!hideToolbar) ui.topReveal = false;
  });

  onMount(() => {
    const unlisten = listenBackend(flushAndClose);

    void startup(() => (motionReady = true));

    return () => {
      for (const u of unlisten) void u.then((f) => f());
    };
  });

  /** A file dragged over the chrome (from the downloads button, or from Explorer) can't be
   *  dropped on it: the chrome's webview would navigate to the file. Pages below take it. */
  function refuseFileDrop(e: DragEvent) {
    if (e.defaultPrevented || !e.dataTransfer?.types.includes('Files')) return;
    e.preventDefault();
    if (e.type === 'dragover') e.dataTransfer.dropEffect = 'none';
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (ui.menu) ui.closeMenu();
      else if (ui.bookmarkDialog) ui.bookmarkDialog = null;
      else if (ui.bookmarkPopup) ui.bookmarkPopup = null;
      else if (ui.selectedTabs.length) ui.clearTabSelection();
      else if (ui.media) ui.media = null;
      else if (extensions.popover) extensions.close();
      else if (ui.downloadsOpen) ui.downloadsOpen = false;
      else if (browser.glance && !ui.palette.open) browser.closeGlance();
      else if (ui.topReveal) ui.topReveal = false;
      else if (ui.flyout) {
        ui.flyout = false;
        ui.flyoutPinned = false;
      }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} ondragover={refuseFileDrop} ondrop={refuseFileDrop} bind:innerWidth={winW} />

<div
  class="app"
  data-layout-root
  role="application"
  class:compact
  class:fullscreen={pageFullscreen}
  class:single
  class:has-bar={barOn}
  class:collapsed-mode={mode === 'collapsed'}
  class:no-toolbar={hideToolbar}
  class:side-right={side === 'right'}
  class:side-top={side === 'top'}
  class:side-bottom={side === 'bottom'}
  class:incognito
  class:animating
  inert={onboarding.open}
  style:--sidebar-w="{sideW}px"
  style:--lay-side-ms="{motion.side}ms"
  style:--lay-bar-ms="{motion.bar}ms"
  style:--lay-bar-ease={motion.fold ? 'ease-in-out' : 'var(--zen-spring)'}
  oncontextmenu={(e) => {
    if (!(e.target as HTMLElement).closest('input, textarea, [contenteditable]')) e.preventDefault();
  }}
>
  <!-- The space's colour tints the window (Zen's workspace gradient), cross-fading on a switch. -->
  <div
    class="space-bg-layer"
    class:masked={hasPanes}
    aria-hidden="true"
    style:mask-image={hasPanes ? `linear-gradient(#000 0 0), ${paneMask.image}` : null}
    style:mask-position={hasPanes ? `0 0, ${paneMask.position}` : null}
    style:mask-size={hasPanes ? `100% 100%, ${paneMask.size}` : null}
    style:mask-composite={hasPanes ? `exclude, ${overlay.panes.map(() => 'add').join(', ')}` : null}
  >
    <SpaceTint {space} />
  </div>

  <ChromeSlots {showSidebar} {strip} {side} {mode} {stripHasControls} {stripIsToolbar} {showTopBar} {barOn} motion={() => motion} />
  <main class="stage">
    <Content />
  </main>

  <CaptionReveal {single} {pageFullscreen} {barOn} {revealBar} />

  <CompactReveal {compact} {pageFullscreen} {hideToolbar} {side} {strip} {mode} />

  <Overlays />
</div>

<Onboarding />

<style>
  /* Layout (Zen): the sidebar on the left, the page inset by the element separation (8px) on
   * the other sides. With multiple toolbars the nav bar sits above the page; in the collapsed
   * layout it spans the whole window above the icon-only sidebar. */
  /* The pieces are placed with the layout variables registered in app.css (--lay-*), so a
   * layout change can transition them; the page webviews follow the stage frame by frame.
   *   --lay-left      left edge of the page      --lay-top       top edge of the page
   *   --lay-right     inset of its right edge    --lay-bottom    inset of its bottom edge
   *   --lay-bar-left  left edge of the toolbar   --lay-side-top  top edge of the sidebar
   *   --lay-bar-top   top edge of the toolbar (below a tab strip at the top)
   *   --lay-glass     width of the sidebar area (0 when the sidebar floats)
   * The sidebar can sit on the right (mirrored) or, as a horizontal tab strip, at the top or the
   * bottom (.side-* classes below). */
  .app {
    position: fixed;
    inset: 0;
    overflow: hidden;
    --lay-left: var(--sidebar-w);
    --lay-right: var(--zen-sep);
    --lay-top: var(--zen-toolbar-h);
    --lay-bottom: var(--zen-sep);
    --lay-bar-left: var(--sidebar-w);
    --lay-bar-top: 0px;
    --lay-side-top: 0px;
    --lay-glass: var(--sidebar-w);
    --strip-bottom: 0px;
  }
  .app.incognito {
    --glass: var(--glass-incognito);
  }
  .app.single {
    --lay-top: var(--zen-sep);
  }
  .app.collapsed-mode {
    --lay-left: calc(var(--collapsed-tab-w) + 12px);
    --lay-bar-left: 0px;
    --lay-side-top: var(--zen-toolbar-h);
    --lay-glass: calc(var(--collapsed-tab-w) + 12px);
  }
  /* The floating sidebar is drawn over the page and paints its own background. */
  .app.compact {
    --lay-left: var(--zen-sep);
    --lay-bar-left: 0px;
    --lay-glass: 0px;
  }
  /* The bookmarks toolbar takes its own row above the page. */
  .app.has-bar {
    --lay-top: calc(var(--zen-toolbar-h) + var(--bm-bar-h));
  }
  .app.single.has-bar {
    --lay-top: var(--bm-bar-h);
  }
  /* Compact mode with the toolbar hidden as well: the page reaches the top edge. */
  .app.no-toolbar {
    --lay-top: var(--zen-sep);
  }

  /* Sidebar on the right: the page on the left. With a toolbar (multiple, collapsed) the toolbar
   * spans the window, so the window controls stay in its corner, and the sidebar starts below. */
  .app.side-right {
    --lay-left: var(--zen-sep);
    --lay-right: var(--sidebar-w);
    --lay-bar-left: 0px;
    --lay-side-top: var(--zen-toolbar-h);
  }
  .app.side-right.single {
    --lay-side-top: 0px;
  }
  .app.side-right.collapsed-mode {
    --lay-right: calc(var(--collapsed-tab-w) + 12px);
  }
  .app.side-right.compact {
    --lay-right: var(--zen-sep);
  }

  /* Tab strip at the top (above the toolbar) or the bottom: the page spans the width. */
  .app.side-top,
  .app.side-bottom {
    --lay-left: var(--zen-sep);
    --lay-bar-left: 0px;
    --lay-side-top: 0px;
    --lay-glass: 0px;
  }
  /* One row: the strip holds the toolbar too, so only the bookmarks bar can sit below it. */
  .app.side-top {
    --lay-bar-top: var(--strip-h);
    --lay-top: var(--strip-h);
  }
  .app.side-top.has-bar {
    --lay-top: calc(var(--strip-h) + var(--bm-bar-h));
  }
  .app.side-top.compact {
    --lay-bar-top: 0px;
    --lay-top: var(--zen-toolbar-h);
  }
  .app.side-top.compact.has-bar {
    --lay-top: calc(var(--zen-toolbar-h) + var(--bm-bar-h));
  }
  .app.side-top.compact.no-toolbar {
    --lay-top: var(--zen-sep);
  }
  .app.side-bottom {
    --lay-bottom: var(--strip-h);
    --strip-bottom: var(--strip-h);
  }
  .app.side-bottom.compact {
    --lay-bottom: var(--zen-sep);
    --strip-bottom: 0px;
  }

  .app.animating {
    transition:
      --lay-left var(--lay-side-ms) var(--zen-spring),
      --lay-right var(--lay-side-ms) var(--zen-spring),
      --lay-top var(--lay-bar-ms) var(--lay-bar-ease),
      --lay-bottom var(--lay-side-ms) var(--zen-spring),
      --lay-bar-top var(--lay-side-ms) var(--zen-spring),
      --lay-bar-left var(--lay-side-ms) var(--zen-spring),
      --lay-side-top var(--lay-side-ms) var(--zen-spring),
      --lay-glass var(--lay-side-ms) var(--zen-spring);
  }
  .stage {
    position: absolute;
    left: var(--lay-left);
    top: var(--lay-top);
    right: var(--lay-right);
    bottom: var(--lay-bottom);
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    z-index: 1;
  }
  .app.fullscreen .stage {
    inset: 0;
  }

  /* Window background. The sidebar gets --glass (darker in incognito windows); the toolbar and
   * the frame around the page get the solid window colour. Three non-overlapping rectangles: the
   * sidebar, the strip above it (collapsed layout) and everything to its right. */
  .space-bg-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 0;
    background:
      linear-gradient(var(--glass), var(--glass)) 0 var(--lay-side-top) / var(--lay-glass)
        calc(100% - var(--lay-side-top)) no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) 0 0 / var(--lay-glass) var(--lay-side-top)
        no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) var(--lay-glass) 0 /
        calc(100% - var(--lay-glass)) 100% no-repeat;
  }
  /* The same three rectangles mirrored, for the sidebar on the right. */
  .app.side-right .space-bg-layer {
    background:
      linear-gradient(var(--glass), var(--glass)) right 0 top var(--lay-side-top) / var(--lay-glass)
        calc(100% - var(--lay-side-top)) no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) right 0 top 0 / var(--lay-glass)
        var(--lay-side-top) no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) 0 0 / calc(100% - var(--lay-glass)) 100%
        no-repeat;
  }
  /* A tab strip gets the sidebar background across the window's top or bottom row. */
  .app.side-top .space-bg-layer {
    background:
      linear-gradient(var(--glass), var(--glass)) 0 0 / 100% var(--lay-bar-top) no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) 0 var(--lay-bar-top) / 100%
        calc(100% - var(--lay-bar-top)) no-repeat;
  }
  .app.side-bottom .space-bg-layer {
    background:
      linear-gradient(var(--glass), var(--glass)) 0 100% / 100% var(--strip-bottom) no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) 0 0 / 100% calc(100% - var(--strip-bottom))
        no-repeat;
  }
  /* Above the page area, below every floating panel (the flyout sits at z-index 40). */
  .space-bg-layer.masked {
    mask-repeat: no-repeat;
  }
</style>
