<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { compactEase, slidePx, slideX, springEase } from './lib/motion';
  import WindowControls from './components/WindowControls.svelte';
  import { browser } from './lib/browser.svelte';
  import { settings, SIDEBAR_MIN } from './lib/settings.svelte';
  import { bookmarks } from './lib/bookmarks.svelte';
  import { downloads, permissions, ui } from './lib/ui.svelte';
  import { api, on } from './lib/ipc';
  import { runAction } from './lib/actions';
  import { barPinned } from './lib/bookmark-actions';
  import { bmDnd } from './lib/bookmark-dnd.svelte';
  import { followLayout, overlay, overlayHold, overlayLease, scheduleLayout, setLayoutRegistry } from './lib/layout.svelte';
  import { startSleepScheduler } from './lib/sleep';
  import { appInfo, incognito } from './lib/app.svelte';
  import { updates } from './lib/updates.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import TopBar from './components/TopBar.svelte';
  import Content from './components/Content.svelte';
  import Palette from './components/Palette.svelte';
  import BookmarksBar from './components/BookmarksBar.svelte';
  import BookmarkPopup from './components/BookmarkPopup.svelte';
  import BookmarkDialog from './components/BookmarkDialog.svelte';
  import BookmarkGhost from './components/BookmarkGhost.svelte';
  import BookmarkImport from './components/BookmarkImport.svelte';
  import Glance from './components/Glance.svelte';
  import Menu from './components/Menu.svelte';
  import Toasts from './components/Toasts.svelte';
  import PermissionPrompt from './components/PermissionPrompt.svelte';
  import PasswordSavePrompt from './components/PasswordSavePrompt.svelte';
  import PasswordSuggest from './components/PasswordSuggest.svelte';
  import { vault } from './lib/vault.svelte';
  import ExtensionInstallPrompt from './components/ExtensionInstallPrompt.svelte';
  import ExtensionsPopover from './components/ExtensionsPopover.svelte';
  import { extensions } from './lib/extensions.svelte';
  import DownloadsPopover from './components/DownloadsPopover.svelte';
  import MediaPanel from './components/MediaPanel.svelte';
  import SpaceEditor from './components/SpaceEditor.svelte';
  import Onboarding from './components/onboarding/Onboarding.svelte';
  import { onboarding } from './lib/onboarding.svelte';
  import { shouldOnboard } from './lib/onboarding-flow';
  import { notes } from './lib/notes.svelte';

  setLayoutRegistry({
    isLive: (id) => {
      const t = browser.tabs[id];
      return !!t && t.live && !t.crashed;
    },
  });

  const compact = $derived(settings.value.compact);
  const pageFullscreen = $derived(!!browser.fullscreenTabId);
  const mode = $derived(settings.value.toolbarMode);
  /** Zen's single toolbar: no toolbar above the page; window controls appear at the top edge. */
  const single = $derived(mode === 'single');
  const showSidebar = $derived(!compact && !pageFullscreen);
  const showTopBar = $derived(!single && !pageFullscreen);
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
  const LAYOUT_MS = 300;
  let animating = $state(false);
  let motionReady = false;
  let lastLayoutKey = '';
  let animTimer: ReturnType<typeof setTimeout> | undefined;
  $effect.pre(() => {
    const key = `${compact}|${mode}|${barOn}`;
    if (key === lastLayoutKey) return;
    const first = !lastLayoutKey;
    lastLayoutKey = key;
    if (first || !motionReady || pageFullscreen) return;
    animating = true;
    clearTimeout(animTimer);
    animTimer = setTimeout(() => (animating = false), LAYOUT_MS + 80);
    followLayout(LAYOUT_MS + 80);
  });
  const motionMs = () => (animating ? LAYOUT_MS : 0);

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
        void api.sessionSave(json);
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
  // floating UI raises it above them: the tint then covers the pages too. Raising is a single
  // z-order switch, so the tint is cut out under the live pages and drawn there by a separate
  // scrim that fades in when floating UI shows, and fades out (with the chrome held above the
  // pages until it is done) when it goes away, instead of blinking on and off.
  const raised = $derived(overlay.leases > 0 || overlay.forced || !!browser.glance);
  // One mask layer per page rectangle; the window tint keeps everything but them (the full
  // layer excludes their union), the scrim only them.
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
      if (!incognito) await api.sessionSave(browser.snapshot());
    } finally {
      await api.windowClose();
    }
  }

  // Compact mode: the sidebar slides in from the left edge and stays while hovered, plus
  // Zen's 150 ms keep-hover grace (zen.view.compact.sidebar-keep-hover.duration).
  let revealTimer: ReturnType<typeof setTimeout> | undefined;
  function revealSidebar() {
    clearTimeout(revealTimer);
    clearTimeout(hideTimer);
    revealTimer = setTimeout(() => (ui.flyout = true), 50);
  }
  function cancelReveal() {
    clearTimeout(revealTimer);
  }
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  function scheduleHideFlyout() {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      if (!ui.menu && !ui.downloadsOpen && !extensions.popover && !ui.media && !ui.spaceEditor && !ui.palette.open && !ui.renaming && !ui.dragging)
        ui.flyout = false;
    }, 150);
  }
  function keepFlyout() {
    clearTimeout(hideTimer);
  }

  // Single toolbar: hovering the strip above the page reveals the window controls (Zen's
  // zen.view.hide-window-controls), after a short delay, and hides them once the pointer leaves.
  let captionTimer: ReturnType<typeof setTimeout> | undefined;
  function revealCaptions() {
    clearTimeout(captionTimer);
    captionTimer = setTimeout(() => (ui.captionReveal = true), 120);
  }
  function hideCaptions(delay = 250) {
    clearTimeout(captionTimer);
    captionTimer = setTimeout(() => (ui.captionReveal = false), delay);
  }

  onMount(() => {
    const unlisten: Promise<() => void>[] = [
      on('tab-update', (p) => browser.applyPatch(p)),
      on('tab-created', (e) => {
        if (e.popup) browser.adoptPopup(e.id, e.url, e.openerId);
        else browser.adoptTab(e.id, e.url, e.openerId, true);
      }),
      on('tab-open-request', (r) => {
        if (r.kind === 'glance') browser.openGlance(r.url, r.openerId);
        else if (r.kind === 'split' && r.openerId) {
          const t = browser.openTab(r.url, { activate: false, openerId: r.openerId });
          browser.splitWith(r.openerId, t.id);
        } else browser.openTab(r.url, { activate: r.kind === 'foreground', openerId: r.openerId });
      }),
      on('tab-close-request', ({ id }) => {
        if (browser.glance?.tabId === id) browser.closeGlance();
        else browser.removeTab(id, false);
        permissions.dropTab(id);
        vault.dropTab(id);
      }),
      on('tab-focused', ({ id }) => {
        if (browser.activeSplit?.ids.includes(id) && browser.activeTab?.id !== id) browser.activate(id, { focus: false });
        if (ui.flyout) ui.flyout = false;
      }),
      on('popup-blocked', ({ tabId, url }) => {
        ui.toast('Pop-up blocked', {
          icon: 'shield',
          timeout: 6000,
          action: { label: 'Open', run: () => browser.openTab(url, { openerId: tabId }) },
        });
      }),
      // The setup assistant is modal: only Quit gets through.
      on('shortcut', (s) => {
        if (onboarding.open && s.action !== 'quit') return;
        void runAction(s.action, s.source, s.repeat);
      }),
      on('permission-request', (p) => permissions.push(p)),
      on('vault-status', (s) => vault.apply(s)),
      on('password-suggest', (s) => {
        if (browser.isVisible(s.tabId) && !onboarding.open) vault.suggest = s;
      }),
      on('password-suggest-close', ({ tabId }) => {
        if (vault.suggest?.tabId === tabId) vault.closeSuggest(false);
      }),
      on('password-save-offer', (o) => vault.pushOffer(o)),
      on('ext-install', (e) => extensions.handleEvent(e)),
      on('download', (d) => {
        const known = downloads.items.some((x) => x.id === d.id);
        downloads.upsert(d);
        if (!known && d.state === 'progress') {
          ui.toast(`Downloading ${d.fileName ?? 'file'}`, {
            icon: 'download',
            action: { label: 'Show', run: () => (ui.downloadsOpen = true) },
          });
        } else if (known && d.state === 'completed') {
          ui.toast(`Downloaded ${d.fileName ?? 'file'}`, {
            icon: 'check',
            action: { label: 'Open', run: () => void api.openPath(d.path) },
          });
        }
      }),
      on('find-result', (r) => {
        if (r.id === ui.find.tabId) {
          ui.find.matches = r.matches;
          ui.find.active = r.active;
        }
      }),
      on('open-urls', (urls) => {
        for (const url of urls) browser.openTab(url);
      }),
      on('update-status', (s) => updates.apply(s)),
      on('media-changed', ({ tabId, count }) => browser.setMediaCount(tabId, count)),
      on('media-request', ({ tabId, hint }) => {
        // From the button over a video or the page's context menu.
        if (browser.tabs[tabId]) ui.openMedia(tabId, hint);
      }),
      on('app-close-requested', () => void flushAndClose()),
      // Another window changed the bookmarks.
      bookmarks.listen(),
    ];

    void (async () => {
      await settings.load();
      // Appearance must be applied before the window is first shown.
      const pushed = settings.push().catch((e) => console.warn('apply_settings failed', e));
      void settings.pushShortcuts();
      const s = settings.value;
      // First launch (or setup not finished yet): the setup assistant shows before the window
      // does, so returning users never see it flash by.
      if (shouldOnboard(s, incognito)) onboarding.begin();
      const weekly = Date.now() - s.filterListsUpdated > 7 * 86_400_000;
      void api
        .blockerConfigure(settings.enabledLists(), weekly && s.blockerEnabled, s.blockerDisabledSites)
        .then(() => {
          if (weekly) settings.update({ filterListsUpdated: Date.now() });
        })
        .catch(() => {});
      if (s.historyDays > 0) void api.historyPrune(s.historyDays).catch(() => {});

      requestAnimationFrame(() => (motionReady = true));
      if (incognito) browser.startPrivate();
      else browser.restore(await api.sessionLoad().catch(() => null), s.restoreSession);
      await pushed;
      await tick();
      requestAnimationFrame(async () => {
        const info = await api.uiReady();
        appInfo.info = info;
        for (const url of info.startupUrls) browser.openTab(url);
        // A window with nothing to show opens the home page.
        if (!incognito && !info.startupUrls.length && s.homepage && !browser.activeTab) browser.openTab(s.homepage);
        scheduleLayout();
      });
      startSleepScheduler();
      void bookmarks.load();
      // Today's automatic backup of the bookmarks (the first start of the day writes it).
      if (!incognito) void api.bookmarksAutoBackup().catch(() => {});
      void vault.refresh();
      void downloads.load();
      void updates.load();
      // Pinned extension icons need the list; store updates are checked once a day at most.
      setTimeout(() => void extensions.refresh(), 1500);
      if (!incognito && s.extensionUpdates && Date.now() - (s.extensionUpdatesChecked ?? 0) > 86_400_000) {
        setTimeout(() => {
          settings.update({ extensionUpdatesChecked: Date.now() });
          void extensions.checkUpdates(true);
        }, 20_000);
      }
    })();

    return () => {
      for (const u of unlisten) void u.then((f) => f());
    };
  });

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
      else if (ui.flyout) ui.flyout = false;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} bind:innerWidth={winW} />

<div
  class="app"
  role="application"
  class:compact
  class:fullscreen={pageFullscreen}
  class:single
  class:has-bar={barOn}
  class:collapsed-mode={mode === 'collapsed'}
  class:incognito
  class:animating
  inert={onboarding.open}
  style:--sidebar-w="{sideW}px"
  oncontextmenu={(e) => {
    if (!(e.target as HTMLElement).closest('input, textarea, [contenteditable]')) e.preventDefault();
  }}
>
  <!-- The space's colour tints the window (Zen's workspace gradient), cross-fading on a switch. -->
  {#snippet tint()}
    {#key space?.id}
      <div
        class="space-bg"
        style:--c={space?.color ?? '#8b7cf6'}
        in:fade={{ duration: 250, easing: springEase }}
        out:fade={{ duration: 250, easing: springEase }}
      ></div>
    {/key}
  {/snippet}
  <div
    class="space-bg-layer"
    class:masked={hasPanes}
    aria-hidden="true"
    style:mask-image={hasPanes ? `linear-gradient(#000 0 0), ${paneMask.image}` : null}
    style:mask-position={hasPanes ? `0 0, ${paneMask.position}` : null}
    style:mask-size={hasPanes ? `100% 100%, ${paneMask.size}` : null}
    style:mask-composite={hasPanes ? `exclude, ${overlay.panes.map(() => 'add').join(', ')}` : null}
  >
    {@render tint()}
  </div>

  {#if showSidebar}
    <div
      class="side-slot"
      in:slideX={{ from: -1, duration: motionMs(), easing: springEase }}
      out:slideX={{ from: -1, duration: motionMs(), easing: springEase }}
    >
      <Sidebar />
    </div>
  {/if}
  {#if showTopBar}
    <div
      class="toolbar-slot"
      in:slidePx={{ y: -16, duration: motionMs(), easing: springEase }}
      out:slidePx={{ y: -16, duration: motionMs() * 0.6, easing: cubicOut }}
    >
      <TopBar />
      {#if barOn}
        <div class="bar-row" transition:fade={{ duration: 150 }}>
          <BookmarksBar />
        </div>
      {/if}
    </div>
  {/if}
  <main class="stage">
    <Content />
  </main>

  {#if single && !pageFullscreen}
    <!-- The strip above the page belongs to the chrome; hovering it shows the window controls. -->
    <div
      class="hot-caption"
      role="presentation"
      onmouseenter={revealCaptions}
      onmouseleave={() => hideCaptions(400)}
      data-tauri-drag-region
    ></div>
    {#if barOn}
      <!-- The bookmarks toolbar in the strip above the page; hovering it shows the window controls too. -->
      <div
        class="single-bar"
        role="presentation"
        onmouseenter={revealCaptions}
        onmouseleave={() => hideCaptions(400)}
        transition:fade={{ duration: 150 }}
      >
        <BookmarksBar />
      </div>
    {/if}
    {#if ui.captionReveal}
      {#if revealBar}
        <div
          class="reveal-bar"
          role="presentation"
          use:overlayLease
          onmouseenter={() => clearTimeout(captionTimer)}
          onmouseleave={() => hideCaptions()}
          transition:slidePx={{ y: -12, duration: 150, easing: cubicOut }}
        >
          <BookmarksBar floating />
        </div>
      {/if}
      <div
        class="captions"
        role="presentation"
        use:overlayLease
        onmouseenter={() => clearTimeout(captionTimer)}
        onmouseleave={() => hideCaptions()}
        transition:slidePx={{ y: -12, duration: 150, easing: cubicOut }}
      >
        <WindowControls />
      </div>
    {/if}
  {/if}

  {#if compact && !pageFullscreen}
    <div
      class="hot-left"
      role="presentation"
      onmouseenter={revealSidebar}
      onmouseleave={cancelReveal}
      data-tauri-drag-region
    ></div>
    {#if ui.flyout}
      <div
        class="flyout"
        role="presentation"
        use:overlayLease
        onmouseenter={keepFlyout}
        onmouseleave={scheduleHideFlyout}
        in:slideX={{ from: -1.08, duration: 250, easing: compactEase }}
        out:slideX={{ from: -1.08, duration: 150, easing: cubicOut }}
      >
        <Sidebar floating />
      </div>
    {/if}
  {/if}

  <!-- The window tint over the live pages while the chrome is raised (see `raised`). -->
  {#if raised && hasPanes && !pageFullscreen}
    <div
      class="space-bg-layer page-scrim masked"
      aria-hidden="true"
      use:overlayHold
      style:mask-image={paneMask.image}
      style:mask-position={paneMask.position}
      style:mask-size={paneMask.size}
      transition:fade={{ duration: 200, easing: cubicOut }}
    >
      {@render tint()}
    </div>
  {/if}

  <Glance />
  <PermissionPrompt />
  <PasswordSavePrompt />
  <PasswordSuggest />
  <ExtensionInstallPrompt />
  <DownloadsPopover />
  <ExtensionsPopover />
  <MediaPanel />
  <SpaceEditor />
  <BookmarkPopup />
  <BookmarkDialog />
  <BookmarkImport />
  <Palette />
  <Menu />
  <BookmarkGhost />
  <Toasts />
</div>

<Onboarding />

<style>
  /* Layout (Zen): the sidebar on the left, the page inset by the element separation (8px) on
   * the other sides. With multiple toolbars the nav bar sits above the page; in the collapsed
   * layout it spans the whole window above the icon-only sidebar. */
  /* The pieces are placed with the layout variables registered in app.css (--lay-*), so a
   * layout change can transition them; the page webviews follow the stage frame by frame.
   *   --lay-left      left edge of the page      --lay-top       top edge of the page
   *   --lay-bar-left  left edge of the toolbar   --lay-side-top  top edge of the sidebar */
  .app {
    position: fixed;
    inset: 0;
    overflow: hidden;
    --lay-left: var(--sidebar-w);
    --lay-top: var(--zen-toolbar-h);
    --lay-bar-left: var(--sidebar-w);
    --lay-side-top: 0px;
  }
  .app.single {
    --lay-top: var(--zen-sep);
  }
  .app.collapsed-mode {
    --lay-left: calc(var(--collapsed-tab-w) + 12px);
    --lay-bar-left: 0px;
    --lay-side-top: var(--zen-toolbar-h);
  }
  .app.compact {
    --lay-left: var(--zen-sep);
    --lay-bar-left: 0px;
  }
  /* The bookmarks toolbar takes its own row above the page. */
  .app.has-bar {
    --lay-top: calc(var(--zen-toolbar-h) + var(--bm-bar-h));
  }
  .app.single.has-bar {
    --lay-top: var(--bm-bar-h);
  }
  .app.animating {
    transition:
      --lay-left 300ms var(--zen-spring),
      --lay-top 300ms var(--zen-spring),
      --lay-bar-left 300ms var(--zen-spring),
      --lay-side-top 300ms var(--zen-spring);
  }
  .side-slot {
    position: absolute;
    left: 0;
    top: var(--lay-side-top);
    bottom: 0;
    display: flex;
    z-index: 1;
  }
  .side-slot > :global(aside.sidebar) {
    height: 100%;
  }
  .toolbar-slot {
    position: absolute;
    top: 0;
    left: var(--lay-bar-left);
    right: 0;
    display: flex;
    flex-direction: column;
    min-width: 0;
    z-index: 1;
  }
  .stage {
    position: absolute;
    left: var(--lay-left);
    top: var(--lay-top);
    right: var(--zen-sep);
    bottom: var(--zen-sep);
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    z-index: 1;
  }
  .app.fullscreen .stage {
    inset: 0;
  }

  /* Window tint by the active space. Over Mica the tint stays translucent, so the system
   * material still shows through, as in Zen on Windows 11. */
  .space-bg-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 0;
    background: var(--tint);
  }
  /* Above the page area, below every floating panel (the flyout sits at z-index 40). */
  .space-bg-layer.masked {
    mask-repeat: no-repeat;
  }
  .space-bg-layer.page-scrim {
    z-index: 2;
  }
  .space-bg {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(120% 90% at 0% 0%, color-mix(in srgb, var(--c) 24%, transparent), transparent 70%),
      linear-gradient(160deg, color-mix(in srgb, var(--c) 16%, transparent), color-mix(in srgb, var(--c) 7%, transparent));
  }
  :global(:root[data-theme='light']) .space-bg {
    background:
      radial-gradient(120% 90% at 0% 0%, color-mix(in srgb, var(--c) 30%, transparent), transparent 70%),
      linear-gradient(160deg, color-mix(in srgb, var(--c) 20%, transparent), color-mix(in srgb, var(--c) 9%, transparent));
  }

  /* Under the toolbar the row spans the page area, not the collapsed layout's icon sidebar. */
  .bar-row {
    margin-left: calc(var(--lay-left) - var(--lay-bar-left));
  }
  .single-bar {
    position: absolute;
    top: 0;
    left: var(--lay-left);
    right: 0;
    height: var(--bm-bar-h);
    z-index: 6;
  }
  /* The hidden toolbar of the single layout, revealed with the window controls. */
  .reveal-bar {
    position: absolute;
    top: 0;
    left: var(--lay-left);
    right: 138px;
    height: var(--bm-bar-h);
    z-index: 44;
    overflow: hidden;
    border-bottom-left-radius: var(--r-panel);
    border-bottom-right-radius: var(--r-panel);
    corner-shape: var(--sq);
    background: var(--compact-bg);
    box-shadow: var(--zen-big-shadow);
  }
  .hot-caption {
    position: absolute;
    top: 0;
    right: 0;
    left: var(--sidebar-w);
    height: var(--zen-sep);
    z-index: 5;
  }
  .app.compact .hot-caption {
    left: var(--zen-sep);
  }
  .captions {
    position: absolute;
    top: 0;
    right: 0;
    height: 34px;
    z-index: 45;
    display: flex;
    overflow: hidden;
    border-bottom-left-radius: var(--r-panel);
    corner-shape: var(--sq);
    background: var(--compact-bg);
    box-shadow: var(--zen-big-shadow);
  }
  .hot-left {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: var(--margin);
    z-index: 5;
  }
  /* Compact mode's floating sidebar (Zen: .zen-toolbar-background over #navigator-toolbox). */
  .flyout {
    position: absolute;
    left: calc(var(--zen-sep) / 2);
    top: calc(var(--zen-sep) / 2);
    bottom: calc(var(--zen-sep) / 2);
    width: var(--sidebar-w);
    z-index: 40;
    padding-top: 2px;
    border-radius: var(--r-panel);
    corner-shape: var(--sq);
    background: color-mix(in srgb, var(--accent) 7%, var(--compact-bg));
    outline: 1px solid rgba(255, 255, 255, 0.15);
    outline-offset: -1px;
    box-shadow: var(--zen-big-shadow);
    overflow: hidden;
  }
  .app:not(.single) .flyout {
    top: var(--zen-toolbar-h);
  }
</style>
