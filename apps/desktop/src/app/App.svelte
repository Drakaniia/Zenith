<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { compactEase, slidePx, slideX, springEase } from '$shared/ui/motion';
  import WindowControls from '$shared/ui/WindowControls.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { settings, SIDEBAR_MIN } from '$features/settings/settings.svelte';
  import { bookmarks } from '$features/bookmarks/bookmarks.svelte';
  import { downloads } from '$features/downloads/downloads.svelte';
  import { permissions } from '$features/page/permissions.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { api, on } from '$platform/ipc';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { runAction } from './actions';
  import { barPinned } from '$features/bookmarks/bookmark-actions';
  import { bmDnd } from '$features/bookmarks/bookmark-dnd.svelte';
  import { followLayout, overlay, overlayHold, overlayLease, scheduleLayout, setLayoutRegistry } from '$platform/layout.svelte';
  import { startSleepScheduler } from '$features/browser/sleep';
  import { appInfo, incognito } from '$platform/app.svelte';
  import { updates } from '$features/updates/updates.svelte';
  import { promptDefaultBrowser } from '$features/default-browser/default-browser';
  import { isStrip } from '$features/sidebar/sidebar-position';
  import Sidebar from '$features/sidebar/Sidebar.svelte';
  import TabStrip from '$features/sidebar/TabStrip.svelte';
  import TopBar from '$features/toolbar/TopBar.svelte';
  import Content from '$features/browser/Content.svelte';
  import Palette from '$features/palette/Palette.svelte';
  import BookmarksBar from '$features/bookmarks/BookmarksBar.svelte';
  import BookmarkPopup from '$features/bookmarks/BookmarkPopup.svelte';
  import BookmarkDialog from '$features/bookmarks/BookmarkDialog.svelte';
  import BookmarkGhost from '$features/bookmarks/BookmarkGhost.svelte';
  import BookmarkImport from '$features/bookmarks/BookmarkImport.svelte';
  import ProfileDialog from '$features/profiles/ProfileDialog.svelte';
  import Glance from '$features/browser/Glance.svelte';
  import Menu from './Menu.svelte';
  import Toasts from './Toasts.svelte';
  import PermissionPrompt from '$features/page/PermissionPrompt.svelte';
  import PasswordSavePrompt from '$features/passwords/PasswordSavePrompt.svelte';
  import CardSavePrompt from '$features/passwords/cards/CardSavePrompt.svelte';
  import PasswordSuggest from '$features/passwords/PasswordSuggest.svelte';
  import { vault } from '$features/passwords/vault.svelte';
  import ExtensionInstallPrompt from '$features/extensions/ExtensionInstallPrompt.svelte';
  import ExtensionsPopover from '$features/extensions/ExtensionsPopover.svelte';
  import { extensions } from '$features/extensions/extensions.svelte';
  import DownloadsPopover from '$features/downloads/DownloadsPopover.svelte';
  import MediaPanel from '$features/downloads/MediaPanel.svelte';
  import SpaceEditor from '$features/sidebar/SpaceEditor.svelte';
  import Onboarding from '$features/onboarding/OnboardingDialog.svelte';
  import { onboarding } from '$features/onboarding/onboarding.svelte';
  import { shouldOnboard } from '$features/onboarding/onboarding-flow';
  import { notes } from '$features/notes/notes.svelte';

  setLayoutRegistry({
    isLive: (id) => {
      const t = browser.tabs[id];
      return !!t && t.live && !t.crashed;
    },
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
  const LAYOUT_MS = 300;
  let animating = $state(false);
  let motionReady = false;
  let lastLayoutKey = '';
  let animTimer: ReturnType<typeof setTimeout> | undefined;
  $effect.pre(() => {
    const key = `${compact}|${mode}|${barOn}|${hideToolbar}|${side}`;
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

  // Compact mode: the sidebar slides in from its window edge and stays while hovered, plus
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
      if (!ui.flyoutPinned && !ui.menu && !ui.downloadsOpen && !extensions.popover && !ui.media && !ui.spaceEditor && !ui.palette.open && !ui.renaming && !ui.dragging)
        ui.flyout = false;
    }, 150);
  }
  function keepFlyout() {
    clearTimeout(hideTimer);
  }
  // The pinned floating sidebar goes with compact mode.
  $effect(() => {
    if (!compact) ui.flyoutPinned = false;
  });

  // Compact mode's floating toolbar: the top edge reveals it, like the sidebar's left edge.
  let topTimer: ReturnType<typeof setTimeout> | undefined;
  function revealToolbar() {
    clearTimeout(topTimer);
    topTimer = setTimeout(() => (ui.topReveal = true), 50);
  }
  function scheduleHideToolbar(delay = 150) {
    clearTimeout(topTimer);
    topTimer = setTimeout(() => {
      if (!ui.menu && !ui.downloadsOpen && !extensions.popover && !ui.palette.open) ui.topReveal = false;
      else scheduleHideToolbar(400);
    }, delay);
  }
  $effect(() => {
    if (!hideToolbar) ui.topReveal = false;
  });

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
        if (ui.flyout && !ui.flyoutPinned) ui.flyout = false;
        if (ui.topReveal) ui.topReveal = false;
      }),
      // Another normal window changed the settings (incognito windows keep their own copy).
      on('settings-changed', ({ origin }) => {
        if (!incognito && origin !== getCurrentWebviewWindow().label) void settings.reload();
      }),
      on('translucency-changed', () => void settings.push()),
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
        if (browser.isVisible(s.tabId) && !onboarding.open) vault.offerSuggest(s);
      }),
      on('password-suggest-close', ({ tabId }) => {
        if (vault.suggest?.tabId === tabId) vault.closeSuggest(false);
      }),
      on('password-save-offer', (o) => vault.pushOffer(o)),
      on('card-save-offer', (o) => vault.pushCardOffer(o)),
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
      else {
        const saved = await api.sessionLoad().catch(() => null);
        browser.restore(saved?.data ?? null, saved?.keep ?? s.restoreSession, saved?.fresh ?? false);
        // The main window brings back the other windows that were open when Zenith quit.
        void api.windowsRestore(s.restoreSession).catch(() => {});
      }
      await pushed;
      await tick();
      requestAnimationFrame(async () => {
        const info = await api.uiReady();
        appInfo.info = info;
        for (const url of info.startupUrls) browser.openTab(url);
        // A window with nothing to show opens the home page.
        if (!incognito && !info.startupUrls.length && s.homepage && !browser.activeTab) browser.openTab(s.homepage);
        scheduleLayout();
        // Every launch (the main window only) asks to become the default browser while Zenith
        // isn't; the setup assistant asks on its own last step.
        if (!incognito && !onboarding.open && getCurrentWebviewWindow().label === 'main') {
          setTimeout(() => void promptDefaultBrowser(), 1200);
        }
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
      else if (ui.topReveal) ui.topReveal = false;
      else if (ui.flyout) {
        ui.flyout = false;
        ui.flyoutPinned = false;
      }
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
  class:no-toolbar={hideToolbar}
  class:side-right={side === 'right'}
  class:side-top={side === 'top'}
  class:side-bottom={side === 'bottom'}
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
        style:--space-c={space?.color ?? '#8b7cf6'}
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
    {#if strip}
      <div
        class="strip-slot"
        in:slidePx={{ y: side === 'top' ? -16 : 16, duration: motionMs(), easing: springEase }}
        out:slidePx={{ y: side === 'top' ? -16 : 16, duration: motionMs() * 0.6, easing: cubicOut }}
      >
        <TabStrip edge={side === 'top' ? 'top' : 'bottom'} collapsed={mode === 'collapsed'} controls={stripHasControls} merged={stripIsToolbar} />
      </div>
    {:else}
      <div
        class="side-slot"
        in:slideX={{ from: side === 'right' ? 1 : -1, duration: motionMs(), easing: springEase }}
        out:slideX={{ from: side === 'right' ? 1 : -1, duration: motionMs(), easing: springEase }}
      >
        <Sidebar />
      </div>
    {/if}
  {/if}
  {#if showTopBar || (stripIsToolbar && barOn)}
    <div
      class="toolbar-slot"
      in:slidePx={{ y: -16, duration: motionMs(), easing: springEase }}
      out:slidePx={{ y: -16, duration: motionMs() * 0.6, easing: cubicOut }}
    >
      {#if showTopBar}
        <TopBar controls={!stripHasControls} />
      {/if}
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

  <!-- With the tab strip at the top, the top edge reveals the strip and the toolbar together (below). -->
  {#if hideToolbar && side !== 'top'}
    <div
      class="hot-top"
      role="presentation"
      onmouseenter={revealToolbar}
      onmouseleave={() => !ui.topReveal && clearTimeout(topTimer)}
      data-tauri-drag-region
    ></div>
    {#if ui.topReveal}
      <div
        class="top-flyout"
        role="presentation"
        use:overlayLease
        onmouseenter={() => clearTimeout(topTimer)}
        onmouseleave={() => scheduleHideToolbar()}
        in:slidePx={{ y: -16, duration: 200, easing: compactEase }}
        out:slidePx={{ y: -16, duration: 150, easing: cubicOut }}
      >
        <TopBar floating />
      </div>
    {/if}
  {/if}

  {#if compact && !pageFullscreen}
    <!-- The hidden sidebar's window edge reveals it. -->
    <div
      class="hot-edge {side}"
      role="presentation"
      onmouseenter={revealSidebar}
      onmouseleave={cancelReveal}
      data-tauri-drag-region
    ></div>
    {#if ui.flyout}
      {#if strip}
        <div
          class="flyout strip {side}"
          role="presentation"
          use:overlayLease
          onmouseenter={keepFlyout}
          onmouseleave={scheduleHideFlyout}
          in:slidePx={{ y: side === 'top' ? -16 : 16, duration: 200, easing: compactEase }}
          out:slidePx={{ y: side === 'top' ? -16 : 16, duration: 150, easing: cubicOut }}
        >
          <TabStrip floating edge={side === 'top' ? 'top' : 'bottom'} collapsed={mode === 'collapsed'} />
          {#if side === 'top' && hideToolbar}
            <TopBar floating />
          {/if}
        </div>
      {:else}
        <div
          class="flyout {side}"
          role="presentation"
          use:overlayLease
          onmouseenter={keepFlyout}
          onmouseleave={scheduleHideFlyout}
          in:slideX={{ from: side === 'right' ? 1.08 : -1.08, duration: 250, easing: compactEase }}
          out:slideX={{ from: side === 'right' ? 1.08 : -1.08, duration: 150, easing: cubicOut }}
        >
          <Sidebar floating />
        </div>
      {/if}
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
  <CardSavePrompt />
  <PasswordSuggest />
  <ExtensionInstallPrompt />
  <DownloadsPopover />
  <ExtensionsPopover />
  <MediaPanel />
  <SpaceEditor />
  <BookmarkPopup />
  <BookmarkDialog />
  <BookmarkImport />
  <ProfileDialog />
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
   *   --lay-right     inset of its right edge    --lay-bottom    inset of its bottom edge
   *   --lay-bar-left  left edge of the toolbar   --lay-side-top  top edge of the sidebar
   *   --lay-bar-top   top edge of the toolbar (below a tab strip at the top)
   *   --lay-glass     width of the translucent sidebar area (0 when the sidebar floats)
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
  /* The floating sidebar is drawn over the page, where the material cannot show. */
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
      --lay-left 300ms var(--zen-spring),
      --lay-right 300ms var(--zen-spring),
      --lay-top 300ms var(--zen-spring),
      --lay-bottom 300ms var(--zen-spring),
      --lay-bar-top 300ms var(--zen-spring),
      --lay-bar-left 300ms var(--zen-spring),
      --lay-side-top 300ms var(--zen-spring),
      --lay-glass 300ms var(--zen-spring);
  }
  .side-slot {
    position: absolute;
    left: 0;
    top: var(--lay-side-top);
    bottom: 0;
    display: flex;
    z-index: 1;
  }
  .app.side-right .side-slot {
    left: auto;
    right: 0;
  }
  .side-slot > :global(aside.sidebar) {
    height: 100%;
  }
  .strip-slot {
    position: absolute;
    left: 0;
    right: 0;
    height: var(--strip-h);
    z-index: 1;
  }
  .app.side-top .strip-slot {
    top: 0;
  }
  .app.side-bottom .strip-slot {
    bottom: 0;
  }
  .toolbar-slot {
    position: absolute;
    top: var(--lay-bar-top);
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

  /* Window tint by the active space. Only the sidebar shows the system material, through a
   * translucent tint (as in Zen on Windows 11); the toolbar and the frame around the page get
   * the solid window colour. Three non-overlapping rectangles: the sidebar, the strip above it
   * (collapsed layout) and everything to its right. */
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
  .app.side-right .space-bg-layer:not(.page-scrim) {
    background:
      linear-gradient(var(--glass), var(--glass)) right 0 top var(--lay-side-top) / var(--lay-glass)
        calc(100% - var(--lay-side-top)) no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) right 0 top 0 / var(--lay-glass)
        var(--lay-side-top) no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) 0 0 / calc(100% - var(--lay-glass)) 100%
        no-repeat;
  }
  /* A tab strip shows the material across the window's top or bottom row. */
  .app.side-top .space-bg-layer:not(.page-scrim) {
    background:
      linear-gradient(var(--glass), var(--glass)) 0 0 / 100% var(--lay-bar-top) no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) 0 var(--lay-bar-top) / 100%
        calc(100% - var(--lay-bar-top)) no-repeat;
  }
  .app.side-bottom .space-bg-layer:not(.page-scrim) {
    background:
      linear-gradient(var(--glass), var(--glass)) 0 100% / 100% var(--strip-bottom) no-repeat,
      linear-gradient(var(--tint-solid), var(--tint-solid)) 0 0 / 100% calc(100% - var(--strip-bottom))
        no-repeat;
  }
  /* Above the page area, below every floating panel (the flyout sits at z-index 40). */
  .space-bg-layer.masked {
    mask-repeat: no-repeat;
  }
  .space-bg-layer.page-scrim {
    z-index: 2;
    background: var(--page-scrim);
  }
  /* --space-c is a registered colour (app.css), so changing the space's accent fades the tint
   * instead of switching it. */
  .space-bg {
    position: absolute;
    inset: 0;
    transition: --space-c 300ms ease;
    background:
      radial-gradient(120% 90% at 0% 0%, color-mix(in srgb, var(--space-c) 24%, transparent), transparent 70%),
      linear-gradient(160deg, color-mix(in srgb, var(--space-c) 16%, transparent), color-mix(in srgb, var(--space-c) 7%, transparent));
  }
  :global(:root[data-theme='light']) .space-bg {
    background:
      radial-gradient(120% 90% at 0% 0%, color-mix(in srgb, var(--space-c) 30%, transparent), transparent 70%),
      linear-gradient(160deg, color-mix(in srgb, var(--space-c) 20%, transparent), color-mix(in srgb, var(--space-c) 9%, transparent));
  }

  /* Under the toolbar the row spans the page area, not the collapsed layout's icon sidebar. */
  .bar-row {
    margin-left: calc(var(--lay-left) - var(--lay-bar-left));
  }
  .app.side-right .bar-row {
    margin-right: var(--lay-right);
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
  /* Sidebar on the right: the strip above the page ends at the sidebar, and the window controls
   * appear at the page's top right corner rather than over the sidebar's buttons. */
  .app.side-right .hot-caption {
    left: 0;
    right: var(--lay-right);
  }
  .app.side-right.compact .hot-caption {
    left: var(--zen-sep);
    right: 0;
  }
  .app.side-right .single-bar {
    right: var(--lay-right);
  }
  .app.side-right .reveal-bar {
    right: calc(var(--lay-right) + 138px);
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
  .app.side-right:not(.compact) .captions {
    right: var(--lay-right);
    border-bottom-right-radius: var(--r-panel);
  }
  /* Compact mode: the window edge that brings the hidden sidebar back. */
  .hot-edge {
    position: absolute;
    z-index: 5;
  }
  .hot-edge.left,
  .hot-edge.right {
    top: 0;
    bottom: 0;
    width: var(--margin);
  }
  .hot-edge.left {
    left: 0;
  }
  .hot-edge.right {
    right: 0;
  }
  .hot-edge.bottom {
    left: 0;
    right: 0;
    bottom: 0;
    height: var(--margin);
  }
  /* At the top it is a thin band over the toolbar, clear of the window controls; with the toolbar
   * hidden too it is the strip above the page, like the floating toolbar's. */
  .hot-edge.top {
    top: 0;
    left: 0;
    right: 140px;
    height: 4px;
  }
  .app.no-toolbar .hot-edge.top {
    left: var(--margin);
    right: 0;
    height: var(--zen-sep);
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
  .app.no-toolbar .flyout {
    top: calc(var(--zen-sep) / 2);
  }
  .flyout.right {
    left: auto;
    right: calc(var(--zen-sep) / 2);
  }
  /* The floating tab strip, over the toolbar at the top or above the page's bottom edge. */
  .app .flyout.strip {
    left: calc(var(--zen-sep) / 2);
    right: calc(var(--zen-sep) / 2);
    top: auto;
    bottom: auto;
    width: auto;
    padding-top: 0;
  }
  .app .flyout.strip.top {
    top: calc(var(--zen-sep) / 2);
  }
  .app .flyout.strip.bottom {
    bottom: calc(var(--zen-sep) / 2);
  }
  .flyout.strip > :global(.strip) {
    height: var(--strip-h);
  }
  .hot-top {
    position: absolute;
    top: 0;
    left: var(--margin);
    right: 0;
    height: var(--zen-sep);
    z-index: 5;
  }
  /* Compact mode's floating toolbar (Zen: #zen-appcontent-navbar-container in compact mode). */
  .top-flyout {
    position: absolute;
    top: calc(var(--zen-sep) / 2);
    left: calc(var(--zen-sep) / 2);
    right: calc(var(--zen-sep) / 2);
    z-index: 41;
    border-radius: var(--r-panel);
    corner-shape: var(--sq);
    background: color-mix(in srgb, var(--accent) 7%, var(--compact-bg));
    outline: 1px solid rgba(255, 255, 255, 0.15);
    outline-offset: -1px;
    box-shadow: var(--zen-big-shadow);
    overflow: hidden;
  }
</style>
