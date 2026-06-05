<script lang="ts">
  import BookmarksPanel from '../features/bookmarks/components/BookmarksPanel.svelte';
  import DownloadManager from '../features/downloads/components/DownloadManager.svelte';
  import FindBar from '../features/find-in-page/components/FindBar.svelte';
  import HistoryPanel from '../features/history/components/HistoryPanel.svelte';
  import HomePage from '../features/tabs/components/HomePage.svelte';
  import SettingsPage from '../features/settings/components/SettingsPage.svelte';
  import WebviewHost, {
    type WebViewController
  } from '../features/tabs/components/WebviewHost.svelte';
  import { bookmarksState } from '../features/bookmarks/state/bookmarks.svelte';
  import { downloadsState } from '../features/downloads/state/downloads.svelte';
  import { historyState } from '../features/history/state/history.svelte';
  import { resolveHomepageUrl, settingsState } from '../features/settings/state/settings.svelte';
  import { tabsState } from '../features/tabs/state/tabs.svelte';
  import NavigationBar from '../shell/components/NavigationBar.svelte';
  import TitleBar from '../shell/components/TitleBar.svelte';
  import Toast from '../shell/components/Toast.svelte';
  import { toastsState } from '../shell/state/toasts.svelte';
  import { focusAddressInput, focusAppShell } from '../shell/utils/focus';
  import {
    BROWSER_HOME_URL,
    BROWSER_SETTINGS_URL,
    titleForInternalUrl
  } from '../shared/utils/internal-urls';
  import {
    closeAllShellWindows,
    closeWindow as closeAppWindow,
    createShellWindow,
    hideCurrentWindow as hideAppWindow,
    isCurrentWindowFullscreen,
    maximizeWindow as maximizeAppWindow,
    minimizeWindow as minimizeAppWindow,
    openExternalUrl,
    restoreWindow as restoreAppWindow,
    setCurrentWindowFullscreen
  } from '../bridge/commands/windows';
  import { listenNativeShortcutEvents } from '../bridge/events/shortcuts';
  import { isTauriRuntime } from '../bridge/runtime';
  import type { FindResult } from '../shared/types/backend';
  import type { FindDirection, NativeShortcutCommand } from '../shared/types/events';

  type ShellPanel = 'bookmarks' | 'history' | 'downloads' | 'settings' | 'find';
  type ShortcutCommand = () => void | Promise<void>;
  type FindStepDirection = 'next' | 'previous';
  type ShortcutSequence = 'window-menu' | 'browser-menu';

  const nativeRuntime = isTauriRuntime();
  const isApplePlatform = /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);
  const ZOOM_STEP = 0.1;
  const MIN_ZOOM = 0.25;
  const MAX_ZOOM = 5;
  const SHORTCUT_SEQUENCE_TIMEOUT_MS = 1400;
  const BROWSER_WINDOW_LABEL_PREFIX = 'browser_window_';
  const isIncognitoWindow = new URLSearchParams(window.location.search).get('incognito') === '1';

  let activePanel = $state<ShellPanel | null>(null);
  let webViewController = $state<WebViewController | null>(null);
  let mainContentElement: HTMLElement | undefined = $state();
  let addressFocusRequest = $state(0);
  let toolbarStartFocusRequest = $state(0);
  let toolbarEndFocusRequest = $state(0);
  let menuOpenRequest = $state(0);
  let isChromeOverlayOpen = $state(false);
  let findStepRequest = $state(0);
  let findStepDirection = $state<FindStepDirection>('next');
  let pageZoom = $state(1);
  let isFullscreen = $state(false);
  let didStartApp = false;
  let pendingShortcutSequence: ShortcutSequence | null = null;
  let pendingShortcutSequenceTimeout = 0;
  let focusTabsOnNextF6 = false;
  const reportedUnsupportedShortcuts: string[] = [];
  let theme = $derived(settingsState.settings.theme === 'light' ? 'light' : 'dark');
  let isSidePanelOpen = $derived(
    activePanel === 'bookmarks' || activePanel === 'history' || activePanel === 'downloads'
  );
  let isHomeViewVisible = $derived(
    !tabsState.activeTab || tabsState.activeTab.url === BROWSER_HOME_URL
  );
  let isInternalTabActive = $derived(isHomeViewVisible || Boolean(tabsState.activeTab?.isInternal));
  let isSettingsTabActive = $derived(tabsState.activeTab?.url === BROWSER_SETTINGS_URL);

  const EMPTY_FIND_RESULT: FindResult = {
    active_match: 0,
    total_matches: 0
  };

  $effect(() => {
    if (didStartApp) {
      return;
    }

    didStartApp = true;

    void (async () => {
      if (nativeRuntime) {
        await Promise.all([
          settingsState.load(),
          bookmarksState.load(),
          historyState.load(),
          downloadsState.load()
        ]);
      }
    })();
  });

  $effect(() => {
    const handleKeydown = (event: KeyboardEvent) => {
      handleGlobalKeydown(event);
    };
    const handleWheel = (event: WheelEvent) => {
      handleGlobalWheel(event);
    };
    let cancelled = false;
    let unlistenNativeShortcuts: (() => void) | null = null;

    window.addEventListener('keydown', handleKeydown);
    window.addEventListener('wheel', handleWheel, { passive: false });

    if (nativeRuntime) {
      void listenNativeShortcutEvents(async (command) => {
        try {
          handleNativeShortcut(command);
        } catch (error) {
          toastsState.error(`Shortcut failed: ${errorMessage(error)}`);
        }
      })
        .then((unlisten) => {
          if (cancelled) {
            unlisten();
            return;
          }

          unlistenNativeShortcuts = unlisten;
        })
        .catch((error) => {
          toastsState.error(`Failed to listen for keyboard shortcuts: ${errorMessage(error)}`);
        });
    }

    return () => {
      cancelled = true;
      unlistenNativeShortcuts?.();
      window.removeEventListener('keydown', handleKeydown);
      window.removeEventListener('wheel', handleWheel);
      clearPendingShortcutSequence();
    };
  });

  async function navigateActive(url: string): Promise<void> {
    if (url === BROWSER_SETTINGS_URL) {
      openSettingsTab();
      return;
    }

    if (url === BROWSER_HOME_URL) {
      openHomepageInCurrentTab();
      return;
    }

    if (tabsState.activeTab?.url === BROWSER_HOME_URL && isBrowserWebviewUrl(url)) {
      replaceActiveHomeTab(url);
      activePanel = null;
      return;
    }

    if (!webViewController) {
      tabsState.openTab(url, url);
      activePanel = null;
      return;
    }

    await webViewController.navigateActive(url);
    activePanel = null;
  }

  async function reloadActive(): Promise<void> {
    await webViewController?.reloadActive();
  }

  async function hardReloadActive(): Promise<void> {
    await reloadActive();
  }

  async function stopActive(): Promise<void> {
    await webViewController?.stopActive();
  }

  async function backActive(): Promise<void> {
    if (moveInternalActiveHistory(-1)) {
      return;
    }

    await webViewController?.backActive();
  }

  async function forwardActive(): Promise<void> {
    if (moveInternalActiveHistory(1)) {
      return;
    }

    await webViewController?.forwardActive();
  }

  function openPanel(panel: ShellPanel): void {
    if (panel === 'settings') {
      openSettingsTab();
      return;
    }

    activePanel = activePanel === panel ? null : panel;
  }

  function openSettingsTab(): void {
    const existingSettingsTab = tabsState.tabs.find((tab) => tab.url === BROWSER_SETTINGS_URL);

    if (existingSettingsTab) {
      tabsState.setActiveTab(existingSettingsTab.id);
    } else {
      tabsState.openInternalTab(BROWSER_SETTINGS_URL);
    }

    activePanel = null;
  }

  function closeFindBar(): void {
    activePanel = null;
  }

  function closePanel(): void {
    activePanel = null;
  }

  function openDefaultTab(): void {
    const activeTab = tabsState.activeTab;

    activePanel = null;

    if (!activeTab || activeTab.url === BROWSER_HOME_URL) {
      focusAddressBar();
      return;
    }

    tabsState.openInternalTab(BROWSER_HOME_URL);
    focusAddressBar();
  }

  function openVisibleHomepageTab(): void {
    const url = homepageUrl();

    if (url === BROWSER_HOME_URL) {
      activePanel = null;
      focusAddressBar();
      return;
    }

    tabsState.openTab(url);
    activePanel = null;
    focusAddressBar();
  }

  async function openBrowserWindow(incognito = false): Promise<void> {
    if (!nativeRuntime) {
      showUnsupportedShortcut(
        incognito ? 'new-incognito-window-native' : 'new-window-native',
        'New browser windows are only available in the Tauri app.'
      );
      return;
    }

    const label = createBrowserWindowLabel();
    await createShellWindow({
      label,
      url: incognito ? '/?incognito=1' : '/',
      title: incognito ? 'ampui - Private' : 'ampui',
      incognito
    });
  }

  function openHomepageInCurrentTab(): void {
    const activeTab = tabsState.activeTab;

    if (!activeTab) {
      openVisibleHomepageTab();
      return;
    }

    const history = activeTab.history.slice(0, activeTab.historyIndex + 1);

    if (history.at(-1) !== BROWSER_HOME_URL) {
      history.push(BROWSER_HOME_URL);
    }

    const historyIndex = history.length - 1;

    tabsState.updateTab(activeTab.id, {
      url: BROWSER_HOME_URL,
      title: titleForInternalUrl(BROWSER_HOME_URL),
      favicon_url: null,
      is_loading: false,
      can_go_back: historyIndex > 0,
      can_go_forward: false,
      isInternal: true,
      history,
      historyIndex
    });
    activePanel = null;
  }

  function closeActiveTab(): void {
    const activeTab = tabsState.activeTab;

    if (activeTab) {
      tabsState.closeTab(activeTab.id);
    }
  }

  function restoreClosedTab(): void {
    const restored = tabsState.restoreClosedTab();

    if (!restored) {
      showUnsupportedShortcut('restore-tab', 'There is no closed tab to restore.');
    }

    activePanel = null;
  }

  function focusAddressBar(): void {
    addressFocusRequest += 1;
    focusAddressInput();
  }

  function focusFirstToolbarItem(): void {
    toolbarStartFocusRequest += 1;
  }

  function focusRightmostToolbarItem(): void {
    toolbarEndFocusRequest += 1;
  }

  function openBrowserMenu(): void {
    menuOpenRequest += 1;
  }

  async function focusWebContent(): Promise<void> {
    if (!tabsState.activeTab) {
      return;
    }

    if (tabsState.activeTab.isInternal) {
      mainContentElement?.focus();
      return;
    }

    await webViewController?.focusActive();
  }

  function focusNextShellArea(): void {
    if (focusTabsOnNextF6) {
      focusTabsOnNextF6 = false;
      focusActiveTabElement();
      return;
    }

    focusTabsOnNextF6 = true;
    focusAddressBar();
  }

  function focusActiveTabElement(): void {
    window.requestAnimationFrame(() => {
      const activeTab = document.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
      activeTab?.focus();
    });
  }

  function openFindBar(direction: FindStepDirection = 'next', step = false): void {
    activePanel = 'find';

    if (step) {
      findStepDirection = direction;
      findStepRequest += 1;
    }
  }

  function openHistoryPanel(): void {
    activePanel = 'history';
  }

  function openDownloadsPanel(): void {
    activePanel = 'downloads';
  }

  function openBookmarksPanel(): void {
    activePanel = 'bookmarks';
  }

  function toggleBookmarksPanel(): void {
    activePanel = activePanel === 'bookmarks' ? null : 'bookmarks';
  }

  function showKeyboardShortcutHelpUnavailable(): void {
    showUnsupportedShortcut('keyboard-help', 'Keyboard shortcut help is not implemented.');
  }

  async function printActivePage(): Promise<void> {
    if (!tabsState.activeTab) {
      showUnsupportedShortcut('print-no-tab', 'There is no active page to print.');
      return;
    }

    if (tabsState.activeTab.isInternal) {
      window.print();
      return;
    }

    await webViewController?.printActive();
  }

  async function emailActivePageLink(): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || !isBookmarkableUrl(activeTab.url)) {
      showUnsupportedShortcut('email-page-empty', 'There is no web page link to email.');
      return;
    }

    if (!nativeRuntime) {
      showUnsupportedShortcut(
        'email-page-native',
        'Emailing the current page is only available in the Tauri app.'
      );
      return;
    }

    const subject = encodeURIComponent(activeTab.title.trim() || activeTab.url);
    const body = encodeURIComponent(activeTab.url);
    await openExternalUrl(`mailto:?subject=${subject}&body=${body}`);
  }

  function replaceActiveHomeTab(url: string): void {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.url !== BROWSER_HOME_URL) {
      return;
    }

    tabsState.updateTab(activeTab.id, {
      url,
      title: url,
      favicon_url: null,
      is_loading: true,
      can_go_back: false,
      can_go_forward: false,
      isInternal: false,
      history: [url],
      historyIndex: 0
    });
  }

  function homepageUrl(): string {
    return resolveHomepageUrl(settingsState.settings.homepage);
  }

  function isBrowserWebviewUrl(url: string): boolean {
    try {
      const parsed = new URL(url);
      return parsed.protocol === 'http:' || parsed.protocol === 'https:';
    } catch {
      return false;
    }
  }

  function moveInternalActiveHistory(direction: -1 | 1): boolean {
    const activeTab = tabsState.activeTab;

    if (!activeTab?.isInternal) {
      return false;
    }

    const historyIndex = activeTab.historyIndex + direction;
    const url = activeTab.history[historyIndex];

    if (!url) {
      return false;
    }

    tabsState.updateTab(activeTab.id, {
      url,
      title: isBrowserWebviewUrl(url) ? url : internalTitleForUrl(url),
      favicon_url: null,
      is_loading: isBrowserWebviewUrl(url),
      can_go_back: historyIndex > 0,
      can_go_forward: historyIndex < activeTab.history.length - 1,
      isInternal: !isBrowserWebviewUrl(url),
      historyIndex
    });

    return true;
  }

  function internalTitleForUrl(url: string): string {
    return url === BROWSER_SETTINGS_URL
      ? titleForInternalUrl(BROWSER_SETTINGS_URL)
      : titleForInternalUrl(BROWSER_HOME_URL);
  }

  function handleGlobalKeydown(event: KeyboardEvent): void {
    if (event.defaultPrevented) {
      return;
    }

    const command = shortcutCommand(event);

    if (!command) {
      return;
    }

    event.preventDefault();
    runShortcut(command);
  }

  function handleGlobalWheel(event: WheelEvent): void {
    if (!event.ctrlKey || event.defaultPrevented) {
      return;
    }

    event.preventDefault();
    const direction = event.deltaY < 0 ? 1 : -1;
    runShortcut(() => setPageZoom(pageZoom + direction * ZOOM_STEP));
  }

  function handleNativeShortcut(command: NativeShortcutCommand): void {
    const shortcut = nativeShortcutCommand(command);

    if (shortcut) {
      runShortcut(shortcut);
    }
  }

  function nativeShortcutCommand(command: NativeShortcutCommand): ShortcutCommand | null {
    if (command.startsWith('tab-')) {
      const tabIndex = Number(command.slice('tab-'.length)) - 1;

      if (Number.isInteger(tabIndex) && tabsState.tabs[tabIndex]) {
        return () => {
          tabsState.tabByIndex(tabIndex);
        };
      }

      return null;
    }

    switch (command) {
      case 'new-tab':
        return openDefaultTab;
      case 'new-window':
        return () => openBrowserWindow(false);
      case 'new-incognito-window':
        return () => openBrowserWindow(true);
      case 'restore-tab':
        return restoreClosedTab;
      case 'close-tab':
        return tabsState.activeTab ? closeActiveTab : null;
      case 'close-window':
        return closeCurrentWindow;
      case 'quit-browser':
        return quitBrowser;
      case 'next-tab':
        return tabsState.tabs.length > 0 ? tabsState.nextTab : null;
      case 'previous-tab':
        return tabsState.tabs.length > 0 ? tabsState.previousTab : null;
      case 'move-tab-left':
        return () => tabsState.moveActiveTab(-1);
      case 'move-tab-right':
        return () => tabsState.moveActiveTab(1);
      case 'last-tab':
        return tabsState.tabs.length > 0 ? tabsState.lastTab : null;
      case 'focus-address':
        return focusAddressBar;
      case 'reload':
        return tabsState.activeTab ? reloadActive : null;
      case 'hard-reload':
        return tabsState.activeTab ? hardReloadActive : null;
      case 'back':
        return tabsState.activeTab?.can_go_back ? backActive : null;
      case 'forward':
        return tabsState.activeTab?.can_go_forward ? forwardActive : null;
      case 'home':
        return openHomepageInCurrentTab;
      case 'find':
        return tabsState.activeTab ? () => openFindBar() : null;
      case 'find-next':
        return tabsState.activeTab ? () => openFindBar('next', activePanel === 'find') : null;
      case 'find-previous':
        return tabsState.activeTab ? () => openFindBar('previous', activePanel === 'find') : null;
      case 'history':
        return openHistoryPanel;
      case 'hide-window':
        return hideCurrentWindow;
      case 'minimize-window':
        return minimizeCurrentWindow;
      case 'downloads':
        return openDownloadsPanel;
      case 'bookmarks':
        return openBookmarksPanel;
      case 'toggle-bookmarks':
        return toggleBookmarksPanel;
      case 'settings':
        return openSettingsTab;
      case 'bookmark-page':
        return canBookmarkActiveTab() ? toggleActiveBookmark : null;
      case 'bookmark-all':
        return bookmarkAllOpenTabs;
      case 'print':
        return printActivePage;
      case 'zoom-in':
        return () => setPageZoom(pageZoom + ZOOM_STEP);
      case 'zoom-out':
        return () => setPageZoom(pageZoom - ZOOM_STEP);
      case 'zoom-reset':
        return () => setPageZoom(1);
      case 'toggle-fullscreen':
        return toggleFullscreen;
      case 'focus-web-content':
        return focusWebContent;
      case 'focus-next-shell-area':
        return focusNextShellArea;
      case 'focus-toolbar-start':
        return focusFirstToolbarItem;
      case 'focus-toolbar-end':
        return focusRightmostToolbarItem;
      case 'open-browser-menu':
        return openBrowserMenu;
      case 'keyboard-help':
        return showKeyboardShortcutHelpUnavailable;
      case 'caret-browsing':
        return () =>
          showUnsupportedShortcut('caret-browsing', 'Caret browsing is not implemented.');
      case 'devtools':
        return () => showUnsupportedShortcut('devtools', 'Developer Tools are not implemented.');
      case 'save-page':
        return () =>
          showUnsupportedShortcut('save-page', 'Saving the current page is not implemented.');
      case 'open-file':
        return () =>
          showUnsupportedShortcut('open-file', 'Opening local files is not implemented.');
      case 'view-source':
        return () => showUnsupportedShortcut('view-source', 'Page source view is not implemented.');
      case 'profile-switcher':
        return () =>
          showUnsupportedShortcut(
            'profile-switcher',
            'Profiles and guest browsing are not implemented.'
          );
    }

    return null;
  }

  function shortcutCommand(event: KeyboardEvent): ShortcutCommand | null {
    const key = shortcutKey(event);
    const primary = isPrimaryShortcut(event);
    const onlyPrimary = primary && !event.altKey;
    const ctrlOnly = event.ctrlKey && !event.metaKey && !event.altKey;
    const noCommandModifier = !event.ctrlKey && !event.metaKey && !event.altKey;

    if (pendingShortcutSequence) {
      return commandForPendingShortcutSequence(key);
    }

    if (key === 'escape' && event.shiftKey && noCommandModifier) {
      return () => showUnsupportedShortcut('task-manager', 'Task Manager is not implemented.');
    }

    if (key === 'escape') {
      if (activePanel) {
        return () => {
          activePanel = null;
        };
      }

      if (isFullscreen) {
        return exitFullscreen;
      }

      if (tabsState.activeTab?.is_loading) {
        return stopActive;
      }

      return null;
    }

    if (event.altKey && !event.ctrlKey && !event.metaKey && key === ' ') {
      startPendingShortcutSequence('window-menu');
      return noop;
    }

    if (event.altKey && !event.ctrlKey && !event.metaKey && key === 'f4') {
      return closeCurrentWindow;
    }

    if (key === 'f11' && noCommandModifier && !event.shiftKey) {
      return toggleFullscreen;
    }

    if (isApplePlatform && event.ctrlKey && !event.metaKey && !event.altKey && key === 'f2') {
      return focusRightmostToolbarItem;
    }

    if (key === 'f6' && event.ctrlKey && !event.altKey && !event.metaKey) {
      return focusWebContent;
    }

    if (key === 'f5' && event.ctrlKey && !event.altKey && !event.metaKey && tabsState.activeTab) {
      return hardReloadActive;
    }

    if (key === 'f5' && tabsState.activeTab && !event.altKey && !event.ctrlKey && !event.metaKey) {
      return event.shiftKey ? hardReloadActive : reloadActive;
    }

    if (key === 'f3' && !event.ctrlKey && !event.metaKey && !event.altKey && tabsState.activeTab) {
      return () => {
        openFindBar(event.shiftKey ? 'previous' : 'next', activePanel === 'find');
      };
    }

    if (key === 'f1' && noCommandModifier && !event.shiftKey) {
      return showKeyboardShortcutHelpUnavailable;
    }

    if (key === 'f6' && !event.altKey && !event.metaKey) {
      return focusNextShellArea;
    }

    if (key === 'f7' && noCommandModifier && !event.shiftKey) {
      return () => showUnsupportedShortcut('caret-browsing', 'Caret browsing is not implemented.');
    }

    if (key === 'f10' && noCommandModifier && !event.shiftKey) {
      return focusRightmostToolbarItem;
    }

    if (key === 'f12' && noCommandModifier && !event.shiftKey) {
      return () => showUnsupportedShortcut('devtools', 'Developer Tools are not implemented.');
    }

    if (event.altKey && !event.ctrlKey && !event.shiftKey) {
      if (key === 'd') {
        return focusAddressBar;
      }

      if (key === 'home') {
        return openHomepageInCurrentTab;
      }

      if (event.key === 'ArrowLeft' && tabsState.activeTab?.can_go_back) {
        return backActive;
      }

      if (event.key === 'ArrowRight' && tabsState.activeTab?.can_go_forward) {
        return forwardActive;
      }

      if (key === 'f' || key === 'e') {
        startPendingShortcutSequence('browser-menu');
        openBrowserMenu();
        return noop;
      }

      return null;
    }

    if (event.altKey && event.shiftKey && !event.ctrlKey && !event.metaKey) {
      if (key === 't') {
        return focusFirstToolbarItem;
      }

      if (key === 'i') {
        return () => showUnsupportedShortcut('feedback', 'The feedback form is not implemented.');
      }

      if (key === 'a') {
        return () =>
          showUnsupportedShortcut(
            'inactive-dialogs',
            'Inactive dialog focusing is not implemented.'
          );
      }

      if (key === 'n') {
        return () => showUnsupportedShortcut('split-view', 'Split view is not implemented.');
      }

      return null;
    }

    if (isApplePlatform && event.metaKey && event.altKey && !event.ctrlKey) {
      if (event.shiftKey && key === 'i') {
        return () => showUnsupportedShortcut('feedback', 'The feedback form is not implemented.');
      }

      if (event.shiftKey && key === 'a') {
        return () =>
          showUnsupportedShortcut(
            'inactive-dialogs',
            'Inactive dialog focusing is not implemented.'
          );
      }

      if (key === 'arrowright') {
        return tabsState.nextTab;
      }

      if (key === 'arrowleft') {
        return tabsState.previousTab;
      }

      if (key === 'arrowup' || key === 'arrowdown') {
        return () =>
          showUnsupportedShortcut(
            'dialog-toolbar-focus',
            'Dialog and toolbar focus cycling is not implemented.'
          );
      }

      if (key === 'b' && !event.shiftKey) {
        return () => {
          activePanel = 'bookmarks';
        };
      }

      if (key === 'f' && !event.shiftKey) {
        return focusAddressBar;
      }

      if (key === 'n' && !event.shiftKey) {
        return () => showUnsupportedShortcut('split-view', 'Split view is not implemented.');
      }

      if (key === 'p' && !event.shiftKey) {
        return () => showUnsupportedShortcut('page-setup', 'Page setup is not implemented.');
      }

      if (key === 'u' && !event.shiftKey) {
        return () => showUnsupportedShortcut('view-source', 'Page source view is not implemented.');
      }

      if (key === 'i' || key === 'j') {
        return () => showUnsupportedShortcut('devtools', 'Developer Tools are not implemented.');
      }
    }

    if (event.metaKey && !event.ctrlKey && !event.altKey) {
      if (key === '[' || key === 'arrowleft') {
        return tabsState.activeTab?.can_go_back ? backActive : null;
      }

      if (key === ']' || key === 'arrowright') {
        return tabsState.activeTab?.can_go_forward ? forwardActive : null;
      }
    }

    if (!onlyPrimary && !(ctrlOnly && (key === 'pageup' || key === 'pagedown'))) {
      return null;
    }

    if (ctrlOnly && event.shiftKey && key === 'pageup') {
      return () => tabsState.moveActiveTab(-1);
    }

    if (ctrlOnly && event.shiftKey && key === 'pagedown') {
      return () => tabsState.moveActiveTab(1);
    }

    if ((key === 'tab' || (ctrlOnly && key === 'pagedown')) && tabsState.tabs.length > 0) {
      if (key === 'tab' && event.shiftKey) {
        return tabsState.previousTab;
      }

      return tabsState.nextTab;
    }

    if (ctrlOnly && key === 'pageup' && tabsState.tabs.length > 0) {
      return tabsState.previousTab;
    }

    if (event.shiftKey && key === 'n') {
      return () => openBrowserWindow(true);
    }

    if (event.shiftKey && key === 't') {
      return restoreClosedTab;
    }

    if (key === 'n' && !event.shiftKey) {
      return () => openBrowserWindow(false);
    }

    if (key === 't' && !event.shiftKey) {
      return openDefaultTab;
    }

    if (event.shiftKey && key === 'w') {
      return closeCurrentWindow;
    }

    if (key === 'q' && !event.shiftKey && isApplePlatform) {
      return quitBrowser;
    }

    if ((key === 'w' && !event.shiftKey) || (ctrlOnly && key === 'f4')) {
      return tabsState.activeTab ? closeActiveTab : null;
    }

    if (key === 'm' && !event.shiftKey && isApplePlatform) {
      return minimizeCurrentWindow;
    }

    if (key === 'h' && event.shiftKey && isApplePlatform) {
      return openHomepageInCurrentTab;
    }

    if ((key === 'l' || key === 'k' || key === 'e') && !event.shiftKey) {
      return focusAddressBar;
    }

    if (key === 'r' && tabsState.activeTab) {
      return event.shiftKey ? hardReloadActive : reloadActive;
    }

    if ((key === '+' || key === '=') && !event.altKey) {
      return () => setPageZoom(pageZoom + ZOOM_STEP);
    }

    if (key === '-' && !event.altKey) {
      return () => setPageZoom(pageZoom - ZOOM_STEP);
    }

    if (key === '0' && !event.altKey) {
      return () => setPageZoom(1);
    }

    if (key === 'f' && !event.shiftKey && tabsState.activeTab) {
      return () => {
        openFindBar();
      };
    }

    if (key === 'g' && tabsState.activeTab) {
      return () => {
        openFindBar(event.shiftKey ? 'previous' : 'next', true);
      };
    }

    if (key === 'h' && !event.shiftKey) {
      if (isApplePlatform) {
        return hideCurrentWindow;
      }

      return openHistoryPanel;
    }

    if (key === 'y' && !event.shiftKey && isApplePlatform) {
      return openHistoryPanel;
    }

    if (key === 'j' && !event.shiftKey) {
      return openDownloadsPanel;
    }

    if (key === 'j' && event.shiftKey) {
      return openDownloadsPanel;
    }

    if (key === 'o' && event.shiftKey) {
      return openBookmarksPanel;
    }

    if (key === 'b' && event.shiftKey) {
      return toggleBookmarksPanel;
    }

    if (key === 'd' && event.shiftKey) {
      return bookmarkAllOpenTabs;
    }

    if (key === 'd' && !event.shiftKey && canBookmarkActiveTab()) {
      return toggleActiveBookmark;
    }

    if ((event.key === ',' && !event.shiftKey) || (key === 'delete' && event.shiftKey)) {
      return openSettingsTab;
    }

    if (key === 'p' && !event.shiftKey) {
      return printActivePage;
    }

    if (key === 's' && !event.shiftKey) {
      return () =>
        showUnsupportedShortcut('save-page', 'Saving the current page is not implemented.');
    }

    if (key === 'o' && !event.shiftKey) {
      return () => showUnsupportedShortcut('open-file', 'Opening local files is not implemented.');
    }

    if (key === 'u' && !event.shiftKey) {
      return () => showUnsupportedShortcut('view-source', 'Page source view is not implemented.');
    }

    if (key === 'i' && event.shiftKey && isApplePlatform) {
      return emailActivePageLink;
    }

    if ((key === 'i' || key === 'c') && event.shiftKey) {
      return () => showUnsupportedShortcut('devtools', 'Developer Tools are not implemented.');
    }

    if (key === 'm' && event.shiftKey) {
      return () =>
        showUnsupportedShortcut(
          'profile-switcher',
          'Profiles and guest browsing are not implemented.'
        );
    }

    if (key === 'tab' && tabsState.tabs.length > 0) {
      return event.shiftKey ? tabsState.previousTab : tabsState.nextTab;
    }

    if (!event.shiftKey && key >= '1' && key <= '8') {
      const tabIndex = Number(key) - 1;

      if (tabsState.tabs[tabIndex]) {
        return () => {
          tabsState.tabByIndex(tabIndex);
        };
      }

      return null;
    }

    if (!event.shiftKey && key === '9' && tabsState.tabs.length > 0) {
      return tabsState.lastTab;
    }

    return null;
  }

  function runShortcut(command: ShortcutCommand): void {
    void (async () => {
      await focusAppShell();
      await command();
    })().catch((error) => {
      toastsState.error(`Shortcut failed: ${errorMessage(error)}`);
    });
  }

  async function bookmarkAllOpenTabs(): Promise<void> {
    const bookmarkableTabs = tabsState.tabs.filter((tab) => isBookmarkableUrl(tab.url));

    if (bookmarkableTabs.length === 0) {
      showUnsupportedShortcut('bookmark-all-tabs-empty', 'No open tabs can be bookmarked.');
      return;
    }

    await Promise.all(
      bookmarkableTabs.map((tab) =>
        bookmarksState.addBookmark(tab.title.trim() || tab.url, tab.url)
      )
    );
    toastsState.success(`Bookmarked ${bookmarkableTabs.length} open tab(s)`);
  }

  async function setPageZoom(nextZoom: number): Promise<void> {
    pageZoom = clampZoom(nextZoom);
    await webViewController?.setActiveZoom(pageZoom);
  }

  async function toggleFullscreen(): Promise<void> {
    if (!nativeRuntime) {
      showUnsupportedShortcut('fullscreen', 'Fullscreen is only available in the Tauri app.');
      return;
    }

    const nextFullscreen = !(await isCurrentWindowFullscreen());
    await setCurrentWindowFullscreen(nextFullscreen);
    isFullscreen = nextFullscreen;
  }

  async function exitFullscreen(): Promise<void> {
    if (!nativeRuntime) {
      return;
    }

    await setCurrentWindowFullscreen(false);
    isFullscreen = false;
  }

  async function closeCurrentWindow(): Promise<void> {
    if (!nativeRuntime) {
      showUnsupportedShortcut(
        'close-window',
        'Window controls are only available in the Tauri app.'
      );
      return;
    }

    await closeAppWindow();
  }

  async function minimizeCurrentWindow(): Promise<void> {
    if (!nativeRuntime) {
      showUnsupportedShortcut(
        'minimize-window',
        'Window controls are only available in the Tauri app.'
      );
      return;
    }

    await minimizeAppWindow();
  }

  async function hideCurrentWindow(): Promise<void> {
    if (!nativeRuntime) {
      showUnsupportedShortcut(
        'hide-window',
        'Window controls are only available in the Tauri app.'
      );
      return;
    }

    await hideAppWindow();
  }

  async function maximizeCurrentWindow(): Promise<void> {
    if (!nativeRuntime) {
      showUnsupportedShortcut(
        'maximize-window',
        'Window controls are only available in the Tauri app.'
      );
      return;
    }

    await maximizeAppWindow();
  }

  async function restoreCurrentWindow(): Promise<void> {
    if (!nativeRuntime) {
      showUnsupportedShortcut(
        'restore-window',
        'Window controls are only available in the Tauri app.'
      );
      return;
    }

    await restoreAppWindow();
  }

  async function quitBrowser(): Promise<void> {
    if (!nativeRuntime) {
      showUnsupportedShortcut('quit-browser', 'Quitting is only available in the Tauri app.');
      return;
    }

    await closeAllShellWindows();
  }

  async function toggleActiveBookmark(): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || !isBookmarkableUrl(activeTab.url)) {
      return;
    }

    const existingBookmark = bookmarksState.findByUrl(activeTab.url);

    if (existingBookmark) {
      await bookmarksState.removeBookmark(existingBookmark.id);
      toastsState.success('Bookmark removed');
      return;
    }

    if (await bookmarksState.checkIsBookmarked(activeTab.url)) {
      const removed = await bookmarksState.removeBookmarkByUrl(activeTab.url);
      if (removed) {
        toastsState.success('Bookmark removed');
      }
      return;
    }

    const bookmark = await bookmarksState.addBookmark(
      activeTab.title.trim() || activeTab.url,
      activeTab.url
    );

    if (bookmark) {
      toastsState.success('Bookmark saved');
    }
  }

  function canBookmarkActiveTab(): boolean {
    const url = tabsState.activeTab?.url ?? '';
    return isBookmarkableUrl(url);
  }

  function isBookmarkableUrl(url: string): boolean {
    return url.startsWith('http://') || url.startsWith('https://');
  }

  function noop(): void {
    // Used for shortcut prefixes such as Alt+Space that should be consumed.
  }

  function shortcutKey(event: KeyboardEvent): string {
    if (event.key === ' ') {
      return ' ';
    }

    return event.key.toLocaleLowerCase();
  }

  function isPrimaryShortcut(event: KeyboardEvent): boolean {
    return isApplePlatform ? event.metaKey : event.ctrlKey;
  }

  function startPendingShortcutSequence(sequence: ShortcutSequence): void {
    clearPendingShortcutSequence();
    pendingShortcutSequence = sequence;
    pendingShortcutSequenceTimeout = window.setTimeout(() => {
      pendingShortcutSequence = null;
      pendingShortcutSequenceTimeout = 0;
    }, SHORTCUT_SEQUENCE_TIMEOUT_MS);
  }

  function clearPendingShortcutSequence(): void {
    if (pendingShortcutSequenceTimeout) {
      window.clearTimeout(pendingShortcutSequenceTimeout);
    }

    pendingShortcutSequence = null;
    pendingShortcutSequenceTimeout = 0;
  }

  function commandForPendingShortcutSequence(key: string): ShortcutCommand | null {
    const sequence = pendingShortcutSequence;
    clearPendingShortcutSequence();

    if (sequence === 'window-menu') {
      if (key === 'r') {
        return restoreCurrentWindow;
      }

      if (key === 'n') {
        return minimizeCurrentWindow;
      }

      if (key === 'x') {
        return maximizeCurrentWindow;
      }

      if (key === 'c') {
        return closeCurrentWindow;
      }
    }

    if (sequence === 'browser-menu' && key === 'x') {
      return quitBrowser;
    }

    return null;
  }

  function clampZoom(value: number): number {
    return Math.min(Math.max(Math.round(value * 100) / 100, MIN_ZOOM), MAX_ZOOM);
  }

  function createBrowserWindowLabel(): string {
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    const id = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
    return `${BROWSER_WINDOW_LABEL_PREFIX}${id}`;
  }

  function showUnsupportedShortcut(key: string, message: string): void {
    if (reportedUnsupportedShortcuts.includes(key)) {
      return;
    }

    reportedUnsupportedShortcuts.push(key);
    toastsState.info(message);
  }

  async function findActive(
    query: string,
    direction: FindDirection,
    caseSensitive: boolean
  ): Promise<FindResult> {
    if (!webViewController) {
      return EMPTY_FIND_RESULT;
    }

    return webViewController.findActive({
      query,
      direction,
      caseSensitive
    });
  }

  async function clearFindActive(): Promise<void> {
    await webViewController?.clearFindActive();
  }

  function errorMessage(error: unknown): string {
    if (error instanceof Error) {
      return error.message;
    }

    if (typeof error === 'string') {
      return error;
    }

    return 'Unknown error';
  }
</script>

<svelte:head>
  <meta name="color-scheme" content="dark light" />
</svelte:head>

<div class="app-shell" data-theme={theme}>
  <TitleBar
    onOpenPanel={openPanel}
    onOpenTab={openDefaultTab}
    focusMenuRequest={toolbarEndFocusRequest}
    openMenuRequest={menuOpenRequest}
    onMenuOpenChange={(open) => {
      isChromeOverlayOpen = open;
    }}
  />
  <NavigationBar
    onNavigate={navigateActive}
    onReload={reloadActive}
    onStop={stopActive}
    onBack={backActive}
    onForward={forwardActive}
    focusAddressRequest={addressFocusRequest}
    focusToolbarStartRequest={toolbarStartFocusRequest}
  />
  <div class="findbar-slot">
    <FindBar
      open={activePanel === 'find'}
      onClose={closeFindBar}
      onFind={findActive}
      onClear={clearFindActive}
      stepRequest={findStepRequest}
      stepDirection={findStepDirection}
    />
  </div>
  <main
    bind:this={mainContentElement}
    class={`app-content ${isSidePanelOpen ? 'app-content--with-library' : ''}`}
    aria-label="Browser content"
    data-active-panel={activePanel ?? 'none'}
    tabindex="-1"
  >
    <div class={`webview-slot ${isInternalTabActive ? 'webview-slot--internal' : ''}`}>
      <div class="webview-host" aria-hidden={isInternalTabActive}>
        <WebviewHost
          bind:controller={webViewController}
          zoomScale={pageZoom}
          incognito={isIncognitoWindow}
          chromeOverlayOpen={isChromeOverlayOpen}
        />
      </div>
      {#if isHomeViewVisible}
        <HomePage onNavigate={navigateActive} />
      {:else if isSettingsTabActive}
        <SettingsPage />
      {/if}
    </div>
    <BookmarksPanel
      open={activePanel === 'bookmarks'}
      onClose={closePanel}
      onNavigate={navigateActive}
    />
    <HistoryPanel
      open={activePanel === 'history'}
      onClose={closePanel}
      onNavigate={navigateActive}
    />
    <DownloadManager open={activePanel === 'downloads'} onClose={closePanel} />
  </main>
  <Toast />
</div>

<style>
  :global(:root) {
    color-scheme: dark;
    --color-background: #0f0f10;
    --color-surface: #1a1a1c;
    --color-surface-elevated: #222226;
    --color-border: #2a2a2e;
    --color-accent: #3b82f6;
    --color-focus: #60a5fa;
    --color-text-primary: #f5f5f7;
    --color-text-secondary: #a1a1aa;
    --color-danger: #ef4444;
    --color-success: #22c55e;
    --color-warning: #f59e0b;
    --shadow-elevated: 0 16px 48px rgb(0 0 0 / 0.32);
    --shell-tabbar-height: 36px;
    --shell-nav-height: 48px;
    --shell-statusbar-height: 24px;
    --radius-control: 8px;
  }

  :global(html),
  :global(body),
  :global(#app) {
    width: 100%;
    height: 100%;
  }

  :global(html) {
    background: var(--color-background);
    text-rendering: geometricprecision;
  }

  :global(body) {
    margin: 0;
    overflow: hidden;
    background: var(--color-background);
    color: var(--color-text-primary);
    font-family: 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif;
  }

  :global(*) {
    box-sizing: border-box;
    scrollbar-color: var(--color-border) transparent;
    scrollbar-width: thin;
  }

  :global(*::selection) {
    background: rgb(59 130 246 / 0.35);
    color: var(--color-text-primary);
  }

  :global(*:focus-visible) {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }

  :global(*::-webkit-scrollbar) {
    width: 10px;
    height: 10px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: transparent;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: var(--color-border);
    border: 2px solid transparent;
    border-radius: 999px;
    background-clip: padding-box;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: var(--color-text-secondary);
    background-clip: padding-box;
  }

  :global(button),
  :global(input),
  :global(select),
  :global(textarea) {
    font: inherit;
  }

  .app-shell {
    color-scheme: dark;
    position: relative;
    display: grid;
    grid-template-rows: var(--shell-tabbar-height) var(--shell-nav-height) auto minmax(0, 1fr);
    width: 100%;
    height: 100%;
    min-width: 320px;
    min-height: 320px;
    overflow: hidden;
    background: var(--color-background);
    color: var(--color-text-primary);
  }

  .app-shell[data-theme='light'] {
    color-scheme: light;
    --color-background: #f5f5f7;
    --color-surface: #ffffff;
    --color-surface-elevated: #ffffff;
    --color-border: #d1d1d6;
    --color-accent: #2563eb;
    --color-focus: #2563eb;
    --color-text-primary: #1d1d1f;
    --color-text-secondary: #6e6e73;
    --shadow-elevated: 0 18px 48px rgb(27 31 38 / 0.16);
  }

  .app-content {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    width: 100%;
    min-height: 0;
    overflow: hidden;
    background: var(--color-background);
  }

  .app-content--with-library {
    grid-template-columns: minmax(0, 1fr) clamp(320px, 34vw, 430px);
  }

  .webview-slot {
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .webview-host {
    position: absolute;
    inset: 0;
    min-width: 0;
    min-height: 0;
  }

  .webview-slot--internal {
    background: var(--color-background);
  }

  .findbar-slot {
    min-width: 0;
    min-height: 0;
  }

  .webview-slot--internal .webview-host {
    visibility: hidden;
    pointer-events: none;
  }

  @media (max-width: 720px) {
    .app-content--with-library {
      grid-template-columns: minmax(0, 80px) minmax(280px, 1fr);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(*),
    :global(*::before),
    :global(*::after) {
      animation-duration: 0.001ms !important;
      animation-iteration-count: 1 !important;
      scroll-behavior: auto !important;
      transition-duration: 0.001ms !important;
    }
  }
</style>
