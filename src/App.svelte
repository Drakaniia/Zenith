<script lang="ts">
  import { getAllWindows, getCurrentWindow } from '@tauri-apps/api/window';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import BookmarksPanel from './lib/components/BookmarksPanel.svelte';
  import DownloadManager from './lib/components/DownloadManager.svelte';
  import HistoryPanel from './lib/components/HistoryPanel.svelte';
  import NavigationBar from './lib/components/NavigationBar.svelte';
  import FindBar from './lib/components/FindBar.svelte';
  import HomePage from './lib/components/HomePage.svelte';
  import SettingsPage from './lib/components/SettingsPage.svelte';
  import TitleBar from './lib/components/TitleBar.svelte';
  import Toast from './lib/components/Toast.svelte';
  import WebViewContainer, {
    type WebViewController
  } from './lib/components/WebViewContainer.svelte';
  import { bookmarksState } from './lib/state/bookmarks.svelte';
  import { downloadsState } from './lib/state/downloads.svelte';
  import { historyState } from './lib/state/history.svelte';
  import { resolveHomepageUrl, settingsState } from './lib/state/settings.svelte';
  import { tabsState } from './lib/state/tabs.svelte';
  import { toastsState } from './lib/state/toasts.svelte';
  import {
    BROWSER_HOME_URL,
    BROWSER_SETTINGS_URL,
    titleForInternalUrl
  } from './lib/utils/internalUrls';
  import {
    closeWindow as closeAppWindow,
    isTauriRuntime,
    maximizeWindow as maximizeAppWindow,
    minimizeWindow as minimizeAppWindow,
    restoreWindow as restoreAppWindow,
    type FindDirection,
    type FindResult
  } from './lib/utils/tauri';

  type ChromePanel = 'bookmarks' | 'history' | 'downloads' | 'settings' | 'find';
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

  let activePanel = $state<ChromePanel | null>(null);
  let webViewController = $state<WebViewController | null>(null);
  let addressFocusRequest = $state(0);
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
    let cancelled = false;

    void (async () => {
      if (nativeRuntime) {
        await Promise.all([
          settingsState.load(),
          bookmarksState.load(),
          historyState.load(),
          downloadsState.load()
        ]);
      }

      if (!cancelled && tabsState.tabs.length === 0) {
        tabsState.openTab(homepageUrl());
      }
    })();

    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const handleKeydown = (event: KeyboardEvent) => {
      handleGlobalKeydown(event);
    };

    window.addEventListener('keydown', handleKeydown);

    return () => {
      window.removeEventListener('keydown', handleKeydown);
      clearPendingShortcutSequence();
    };
  });

  async function navigateActive(url: string): Promise<void> {
    if (url === BROWSER_SETTINGS_URL) {
      openSettingsTab();
      return;
    }

    if (url === BROWSER_HOME_URL) {
      tabsState.openHomeTab();
      activePanel = null;
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

  function openPanel(panel: ChromePanel): void {
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

  function openHomepageTab(): void {
    tabsState.openTab(homepageUrl());
    activePanel = null;
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
    const browserWindow = new WebviewWindow(label, {
      url: incognito ? '/?incognito=1' : '/',
      title: incognito ? 'Rust Browser - Incognito' : 'Rust Browser',
      width: 1200,
      height: 800,
      minWidth: 800,
      minHeight: 600,
      decorations: false,
      resizable: true,
      center: true,
      focus: true,
      incognito
    });

    await new Promise<void>((resolve, reject) => {
      void browserWindow.once('tauri://created', () => {
        resolve();
      });
      void browserWindow.once('tauri://error', (event) => {
        reject(new Error(String(event.payload || 'Failed to create browser window.')));
      });
    });
  }

  function openHomepageInCurrentTab(): void {
    const activeTab = tabsState.activeTab;

    if (!activeTab) {
      openHomepageTab();
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
  }

  function focusNextChromeArea(): void {
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
      return () =>
        showUnsupportedShortcut('task-manager', 'Chrome Task Manager is not implemented.');
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

    if (key === 'f5' && primary && !event.shiftKey) {
      return focusAddressBar;
    }

    if (key === 'f5' && tabsState.activeTab && !event.altKey && !event.ctrlKey && !event.metaKey) {
      return reloadActive;
    }

    if (key === 'f3' && !event.ctrlKey && !event.metaKey && !event.altKey && tabsState.activeTab) {
      return () => {
        openFindBar(event.shiftKey ? 'previous' : 'next', activePanel === 'find');
      };
    }

    if (key === 'f1' && noCommandModifier && !event.shiftKey) {
      return () =>
        showUnsupportedShortcut(
          'help-center',
          'Opening a Chrome Help Center tab is not implemented.'
        );
    }

    if (key === 'f6' && !event.altKey && !event.metaKey) {
      return focusNextChromeArea;
    }

    if (key === 'f7' && noCommandModifier && !event.shiftKey) {
      return () => showUnsupportedShortcut('caret-browsing', 'Caret browsing is not implemented.');
    }

    if (key === 'f10' && noCommandModifier && !event.shiftKey) {
      return focusAddressBar;
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
        return noop;
      }

      return null;
    }

    if (event.altKey && event.shiftKey && !event.ctrlKey && !event.metaKey) {
      if (key === 't') {
        return focusAddressBar;
      }

      if (key === 'i') {
        return () =>
          showUnsupportedShortcut('feedback', 'The Chrome feedback form is not implemented.');
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
      if (key === 'arrowright') {
        return tabsState.nextTab;
      }

      if (key === 'arrowleft') {
        return tabsState.previousTab;
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
      return openHomepageTab;
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

    if ((key === 'l' || key === 'k' || key === 'e') && !event.shiftKey) {
      return focusAddressBar;
    }

    if (key === 'r' && tabsState.activeTab) {
      return reloadActive;
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

      return () => {
        activePanel = 'history';
      };
    }

    if (key === 'y' && !event.shiftKey && isApplePlatform) {
      return () => {
        activePanel = 'history';
      };
    }

    if (key === 'j' && !event.shiftKey) {
      return () => {
        activePanel = 'downloads';
      };
    }

    if (key === 'j' && event.shiftKey) {
      return isApplePlatform
        ? () => {
            activePanel = 'downloads';
          }
        : () => showUnsupportedShortcut('devtools', 'Developer Tools are not implemented.');
    }

    if (key === 'o' && event.shiftKey) {
      return () => {
        activePanel = 'bookmarks';
      };
    }

    if (key === 'b' && event.shiftKey) {
      return () => {
        activePanel = activePanel === 'bookmarks' ? null : 'bookmarks';
      };
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
      return () => showUnsupportedShortcut('print', 'Print is not implemented.');
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

    if ((key === 'i' || key === 'c') && event.shiftKey) {
      return () => showUnsupportedShortcut('devtools', 'Developer Tools are not implemented.');
    }

    if (key === 'm' && event.shiftKey) {
      return () =>
        showUnsupportedShortcut(
          'profile-switcher',
          'Chrome profiles and guest browsing are not implemented.'
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
    void Promise.resolve(command()).catch((error) => {
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
    const appWindow = getAppWindow();

    if (!appWindow) {
      showUnsupportedShortcut('fullscreen', 'Fullscreen is only available in the Tauri app.');
      return;
    }

    const nextFullscreen = !(await appWindow.isFullscreen());
    await appWindow.setFullscreen(nextFullscreen);
    isFullscreen = nextFullscreen;
  }

  async function exitFullscreen(): Promise<void> {
    const appWindow = getAppWindow();

    if (!appWindow) {
      return;
    }

    await appWindow.setFullscreen(false);
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
    const appWindow = getAppWindow();

    if (!appWindow) {
      showUnsupportedShortcut(
        'hide-window',
        'Window controls are only available in the Tauri app.'
      );
      return;
    }

    await appWindow.hide();
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

    const windows = await getAllWindows();

    if (windows.length === 0) {
      await closeCurrentWindow();
      return;
    }

    await Promise.all(windows.map((appWindow) => appWindow.close()));
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

  function getAppWindow(): ReturnType<typeof getCurrentWindow> | null {
    try {
      return getCurrentWindow();
    } catch {
      return null;
    }
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
  <TitleBar />
  <NavigationBar
    onNavigate={navigateActive}
    onReload={reloadActive}
    onStop={stopActive}
    onBack={backActive}
    onForward={forwardActive}
    onOpenPanel={openPanel}
    focusAddressRequest={addressFocusRequest}
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
    class={`app-content ${isSidePanelOpen ? 'app-content--with-library' : ''}`}
    aria-label="Browser content"
    data-active-panel={activePanel ?? 'none'}
  >
    <div class={`webview-slot ${isInternalTabActive ? 'webview-slot--internal' : ''}`}>
      <div class="webview-host" aria-hidden={isInternalTabActive}>
        <WebViewContainer
          bind:controller={webViewController}
          zoomScale={pageZoom}
          incognito={isIncognitoWindow}
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
    --chrome-tabbar-height: 36px;
    --chrome-nav-height: 48px;
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
    grid-template-rows: var(--chrome-tabbar-height) var(--chrome-nav-height) auto minmax(0, 1fr);
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
