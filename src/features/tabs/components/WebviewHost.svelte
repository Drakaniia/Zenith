<script module lang="ts">
  import type { FindResult as TauriFindResult } from '../../../shared/types/backend';
  import type { FindDirection } from '../../../shared/types/events';

  export interface WebViewFindRequest {
    query: string;
    direction: FindDirection;
    caseSensitive: boolean;
  }

  export interface WebViewController {
    navigateActive: (url: string) => Promise<void>;
    reloadActive: () => Promise<void>;
    stopActive: () => Promise<void>;
    backActive: () => Promise<void>;
    forwardActive: () => Promise<void>;
    findActive: (request: WebViewFindRequest) => Promise<TauriFindResult>;
    clearFindActive: () => Promise<void>;
    setActiveZoom: (scaleFactor: number) => Promise<void>;
    focusActive: () => Promise<void>;
    printActive: () => Promise<void>;
    openDevtoolsActive: () => Promise<boolean>;
  }
</script>

<script lang="ts">
  import { AlertTriangle, PanelTop } from 'lucide-svelte';
  import {
    clearBrowserWebviewFind,
    closeAllBrowserWebviews,
    closeBrowserWebview,
    createBrowserWebview,
    evalBrowserWebview,
    findInBrowserWebview,
    focusBrowserWebview,
    hideBrowserWebview,
    navigateBrowserWebview,
    reloadBrowserWebview,
    setBrowserWebviewBounds,
    setBrowserWebviewZoom,
    showBrowserWebview
  } from '../../../bridge/commands/tabs';
  import { toggleNativeDevtools } from '../../../bridge/commands/devtools';
  import { fetchPageMetadata } from '../../../bridge/commands/navigation';
  import { listenBrowserWebviewEvents } from '../../../bridge/events/browser-webviews';
  import { isDesktopRuntime, isElectronRuntime, isTauriRuntime } from '../../../bridge/runtime';
  import { historyState } from '../../history/state/history.svelte';
  import { settingsState } from '../../settings/state/settings.svelte';
  import { focusAppShell } from '../../../shell/utils/focus';
  import type { FindResult, WebviewBounds } from '../../../shared/types/backend';
  import type {
    BrowserWebviewLoadEvent,
    BrowserWebviewNavigationEvent,
    BrowserWebviewTitleEvent
  } from '../../../shared/types/events';
  import { isInternalBrowserUrl, titleForInternalUrl } from '../../../shared/utils/internal-urls';
  import { toastsState } from '../../../shell/state/toasts.svelte';
  import { type BrowserTab, tabsState } from '../state/tabs.svelte';

  interface Props {
    controller?: WebViewController | null;
    zoomScale?: number;
    incognito?: boolean;
    chromeOverlayOpen?: boolean;
  }

  const EMPTY_FIND_RESULT: FindResult = {
    active_match: 0,
    total_matches: 0
  };

  let {
    controller = $bindable<WebViewController | null>(null),
    zoomScale = 1,
    incognito = false,
    chromeOverlayOpen = false
  }: Props = $props();

  type ElectronWebviewElement = HTMLElement & {
    src: string;
    partition: string;
    reload: () => void;
    stop: () => void;
    focus: () => void;
    print?: () => void;
    setZoomFactor?: (factor: number) => void;
    executeJavaScript?: <T = unknown>(script: string) => Promise<T>;
    findInPage?: (text: string, options?: { forward?: boolean; matchCase?: boolean }) => number;
    stopFindInPage?: (action?: 'clearSelection' | 'keepSelection' | 'activateSelection') => void;
    openDevTools?: () => void;
    getURL?: () => string;
    getTitle?: () => string;
  };
  type ElectronWebviewEvent = Event & {
    url?: string;
    title?: string;
    result?: {
      activeMatchOrdinal?: number;
      matches?: number;
      finalUpdate?: boolean;
    };
  };

  const electronRuntime = isElectronRuntime();
  const tauriRuntime = isTauriRuntime();
  const nativeRuntime = isDesktopRuntime();
  const createdLabels = new Set<string>();
  const creatingLabels = new Set<string>();
  const electronWebviews = new Map<string, ElectronWebviewElement>();
  const pendingNavigationUrls = new Map<string, string>();
  const metadataTokens = new Map<string, number>();
  const lastReloadRequests = new Map<string, number>();

  let containerElement: HTMLElement | undefined = $state();
  let lastBounds: WebviewBounds | null = $state(null);
  let syncVersion = 0;
  let hasReportedNativeBridgeIssue = false;

  $effect(() => {
    controller = {
      navigateActive,
      reloadActive,
      stopActive,
      backActive,
      forwardActive,
      findActive,
      clearFindActive,
      setActiveZoom,
      focusActive,
      printActive,
      openDevtoolsActive
    };

    return () => {
      controller = null;
    };
  });

  $effect(() => {
    if (!tauriRuntime) {
      return;
    }

    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void (async () => {
      try {
        const registered = await listenBrowserWebviewEvents({
          onNavigation: handleWebviewNavigation,
          onTitle: handleWebviewTitle,
          onLoad: handleWebviewLoad
        });

        if (cancelled) {
          registered();
          return;
        }

        unlisteners.push(registered);
      } catch (error) {
        reportNativeBridgeIssue(`Failed to listen for webview events: ${errorMessage(error)}`);
      }
    })();

    return () => {
      cancelled = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  });

  $effect(() => {
    const element = containerElement;

    if (!element) {
      return;
    }

    let animationFrame = 0;
    const updateBounds = () => {
      if (animationFrame) {
        window.cancelAnimationFrame(animationFrame);
      }

      animationFrame = window.requestAnimationFrame(() => {
        animationFrame = 0;
        const bounds = boundsFromElement(element);

        if (!bounds || boundsEqual(bounds, lastBounds)) {
          return;
        }

        lastBounds = bounds;
        void syncCreatedBounds(bounds);
      });
    };

    const observer = new ResizeObserver(updateBounds);
    observer.observe(element);
    window.addEventListener('resize', updateBounds);
    updateBounds();

    return () => {
      if (animationFrame) {
        window.cancelAnimationFrame(animationFrame);
      }

      observer.disconnect();
      window.removeEventListener('resize', updateBounds);
    };
  });

  $effect(() => {
    const tabs = tabsState.tabs;
    const activeTabId = tabsState.activeTabId;
    const bounds = lastBounds;
    const javascriptEnabled = settingsState.settings.javascript_enabled;
    const incognitoMode = incognito;
    const shellOverlayOpen = chromeOverlayOpen;

    void syncWebviews(
      tabs,
      activeTabId,
      bounds,
      javascriptEnabled,
      incognitoMode,
      shellOverlayOpen
    );
  });

  $effect(() => {
    const requests = tabsState.reloadRequests;

    for (const [tabId, requestCount] of Object.entries(requests)) {
      if (lastReloadRequests.get(tabId) === requestCount) {
        continue;
      }

      lastReloadRequests.set(tabId, requestCount);
      const tab = tabsState.tabs.find((item) => item.id === tabId);

      if (tab) {
        void reloadTab(tab);
      }
    }

    for (const tabId of Array.from(lastReloadRequests.keys())) {
      if (!(tabId in requests)) {
        lastReloadRequests.delete(tabId);
      }
    }
  });

  $effect(() => {
    const activeTabId = tabsState.activeTabId;
    const scaleFactor = zoomScale;

    if (!activeTabId) {
      return;
    }

    void setActiveZoom(scaleFactor).catch((error) => {
      toastsState.error(`Failed to zoom page: ${errorMessage(error)}`);
    });
  });

  async function navigateActive(url: string): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal) {
      tabsState.openTab(url, url);
      return;
    }

    await navigateTab(activeTab, url, true);
  }

  async function reloadActive(): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (activeTab) {
      await reloadTab(activeTab);
    }
  }

  async function stopActive(): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal) {
      return;
    }

    try {
      if (electronRuntime) {
        electronWebviews.get(activeTab.webviewLabel)?.stop();
      } else if (tauriRuntime && createdLabels.has(activeTab.webviewLabel)) {
        await evalBrowserWebview(activeTab.webviewLabel, 'window.stop();');
      }
    } catch (error) {
      toastsState.error(`Failed to stop page load: ${errorMessage(error)}`);
    } finally {
      tabsState.updateTab(activeTab.id, { is_loading: false });
    }
  }

  async function backActive(): Promise<void> {
    await moveActiveHistory(-1);
  }

  async function forwardActive(): Promise<void> {
    await moveActiveHistory(1);
  }

  async function findActive(request: WebViewFindRequest): Promise<FindResult> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal || !nativeRuntime) {
      return EMPTY_FIND_RESULT;
    }

    if (electronRuntime) {
      return findInElectronWebview(activeTab.webviewLabel, request);
    }

    return findInBrowserWebview({
      label: activeTab.webviewLabel,
      query: request.query,
      direction: request.direction,
      caseSensitive: request.caseSensitive
    });
  }

  async function clearFindActive(): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal || !nativeRuntime) {
      return;
    }

    if (electronRuntime) {
      electronWebviews.get(activeTab.webviewLabel)?.stopFindInPage?.('clearSelection');
      return;
    }

    await clearBrowserWebviewFind(activeTab.webviewLabel);
  }

  async function setActiveZoom(scaleFactor: number): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal || !nativeRuntime) {
      return;
    }

    if (electronRuntime) {
      electronWebviews.get(activeTab.webviewLabel)?.setZoomFactor?.(scaleFactor);
      return;
    }

    if (!createdLabels.has(activeTab.webviewLabel)) {
      return;
    }

    await setBrowserWebviewZoom(activeTab.webviewLabel, scaleFactor);
  }

  async function focusActive(): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal || !nativeRuntime) {
      return;
    }

    if (electronRuntime) {
      electronWebviews.get(activeTab.webviewLabel)?.focus();
      return;
    }

    if (!createdLabels.has(activeTab.webviewLabel)) {
      return;
    }

    await focusBrowserWebview(activeTab.webviewLabel);
  }

  async function printActive(): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal || !nativeRuntime) {
      return;
    }

    if (electronRuntime) {
      electronWebviews.get(activeTab.webviewLabel)?.print?.();
      return;
    }

    if (!createdLabels.has(activeTab.webviewLabel)) {
      return;
    }

    await evalBrowserWebview(activeTab.webviewLabel, 'window.print();');
  }

  async function openDevtoolsActive(): Promise<boolean> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal || !nativeRuntime) {
      return false;
    }

    if (electronRuntime) {
      const webview = electronWebviews.get(activeTab.webviewLabel);

      if (!webview?.openDevTools) {
        return false;
      }

      webview.openDevTools();
      return true;
    }

    if (!tauriRuntime || !createdLabels.has(activeTab.webviewLabel)) {
      return false;
    }

    await toggleNativeDevtools(activeTab.webviewLabel);
    return true;
  }

  async function syncWebviews(
    tabs: BrowserTab[],
    activeTabId: string | null,
    bounds: WebviewBounds | null,
    javascriptEnabled: boolean,
    incognitoMode: boolean,
    shellOverlayOpen: boolean
  ): Promise<void> {
    const version = ++syncVersion;
    const remoteTabs = tabs.filter((tab) => !tab.isInternal && isBrowserWebviewUrl(tab.url));
    const remoteLabels = new Set<string>(remoteTabs.map((tab) => tab.webviewLabel));
    const visibleRemoteLabel =
      remoteTabs.find((tab) => tab.id === activeTabId && !shellOverlayOpen)?.webviewLabel ?? null;

    await Promise.all(
      Array.from(createdLabels)
        .filter((label) => !remoteLabels.has(label))
        .map((label) => closeManagedWebview(label))
    );

    if (electronRuntime) {
      if (!containerElement) {
        return;
      }

      await syncElectronWebviews(
        remoteTabs,
        activeTabId,
        javascriptEnabled,
        incognitoMode,
        shellOverlayOpen
      );

      if (!visibleRemoteLabel) {
        await focusAppShell();
      }

      return;
    }

    if (tauriRuntime && remoteTabs.length === 0) {
      await closeEveryBrowserWebview();
      await focusAppShell();
      return;
    }

    if (!tauriRuntime || !bounds) {
      return;
    }

    for (const tab of remoteTabs) {
      if (version !== syncVersion) {
        return;
      }

      await ensureWebview(tab, bounds, javascriptEnabled, incognitoMode);
    }

    if (version !== syncVersion) {
      return;
    }

    await syncCreatedBounds(bounds);

    await Promise.all(
      remoteTabs.map((tab) =>
        setWebviewVisibility(
          tab.webviewLabel,
          tab.id === activeTabId && !tab.isInternal && !shellOverlayOpen
        )
      )
    );

    if (!visibleRemoteLabel) {
      await focusAppShell();
    }
  }

  async function syncElectronWebviews(
    remoteTabs: BrowserTab[],
    activeTabId: string | null,
    javascriptEnabled: boolean,
    incognitoMode: boolean,
    shellOverlayOpen: boolean
  ): Promise<void> {
    for (const tab of remoteTabs) {
      await ensureElectronWebview(tab, javascriptEnabled, incognitoMode);
    }

    await Promise.all(
      remoteTabs.map((tab) =>
        setWebviewVisibility(
          tab.webviewLabel,
          tab.id === activeTabId && !tab.isInternal && !shellOverlayOpen
        )
      )
    );
  }

  async function ensureElectronWebview(
    tab: BrowserTab,
    javascriptEnabled: boolean,
    incognitoMode: boolean
  ): Promise<void> {
    if (!containerElement || createdLabels.has(tab.webviewLabel)) {
      return;
    }

    const webview = document.createElement('webview') as ElectronWebviewElement;
    webview.className = 'electron-webview';
    webview.setAttribute('data-label', tab.webviewLabel);
    webview.partition = incognitoMode ? `ampui-private-${tab.webviewLabel}` : 'persist:ampui';
    webview.setAttribute('partition', webview.partition);

    if (!javascriptEnabled) {
      webview.setAttribute('webpreferences', 'javascript=no');
    }

    attachElectronWebviewEvents(tab.webviewLabel, webview);
    containerElement.append(webview);
    electronWebviews.set(tab.webviewLabel, webview);
    createdLabels.add(tab.webviewLabel);

    webview.src = tab.url;
    tabsState.updateTab(tab.id, { is_loading: true });
    await setActiveZoom(zoomScale);
    await completeNavigation(tab.id, tab.url, true);
  }

  function attachElectronWebviewEvents(label: string, webview: ElectronWebviewElement): void {
    webview.addEventListener('did-start-loading', () => {
      handleWebviewLoad({ label, url: electronWebviewUrl(webview), state: 'started' });
    });
    webview.addEventListener('did-stop-loading', () => {
      handleWebviewLoad({ label, url: electronWebviewUrl(webview), state: 'finished' });
    });
    webview.addEventListener('did-navigate', (event) => {
      const url = (event as ElectronWebviewEvent).url;
      if (url) {
        handleWebviewNavigation({ label, url });
      }
    });
    webview.addEventListener('did-navigate-in-page', (event) => {
      const url = (event as ElectronWebviewEvent).url;
      if (url) {
        handleWebviewNavigation({ label, url });
      }
    });
    webview.addEventListener('page-title-updated', (event) => {
      const title = (event as ElectronWebviewEvent).title;
      if (title) {
        handleWebviewTitle({ label, title });
      }
    });
    webview.addEventListener('new-window', (event) => {
      event.preventDefault();

      const url = (event as ElectronWebviewEvent).url;
      if (url && isBrowserWebviewUrl(url)) {
        tabsState.openBackgroundTab(url, url);
      }
    });
  }

  async function ensureWebview(
    tab: BrowserTab,
    bounds: WebviewBounds,
    javascriptEnabled: boolean,
    incognitoMode: boolean
  ): Promise<void> {
    if (createdLabels.has(tab.webviewLabel) || creatingLabels.has(tab.webviewLabel)) {
      return;
    }

    creatingLabels.add(tab.webviewLabel);
    tabsState.updateTab(tab.id, { is_loading: true });

    try {
      // Remote pages live in child webviews. Only the trusted shell imports and invokes desktop
      // command wrappers; remote labels are not granted capabilities.
      await createBrowserWebview({
        label: tab.webviewLabel,
        url: tab.url,
        bounds,
        javascriptEnabled,
        incognito: incognitoMode
      });

      if (!isTabOpen(tab.id)) {
        await closeManagedWebview(tab.webviewLabel);
        return;
      }

      createdLabels.add(tab.webviewLabel);
      await setBrowserWebviewZoom(tab.webviewLabel, zoomScale);

      const pendingUrl = pendingNavigationUrls.get(tab.webviewLabel);
      if (pendingUrl && pendingUrl !== tab.url) {
        pendingNavigationUrls.delete(tab.webviewLabel);
        await navigateBrowserWebview(tab.webviewLabel, pendingUrl);
      }

      await setWebviewVisibility(tab.webviewLabel, tab.id === tabsState.activeTabId);
      await completeNavigation(tab.id, pendingUrl ?? tab.url, true);
    } catch (error) {
      const message = errorMessage(error);

      if (message.includes('webview label already exists')) {
        createdLabels.add(tab.webviewLabel);
        await setWebviewVisibility(tab.webviewLabel, tab.id === tabsState.activeTabId);
      } else {
        markTabLoadFailed(tab.id, message);
      }
    } finally {
      creatingLabels.delete(tab.webviewLabel);
    }
  }

  async function navigateTab(
    tab: BrowserTab,
    url: string,
    addHistoryEntry: boolean
  ): Promise<void> {
    if (!isBrowserWebviewUrl(url)) {
      const message = 'Browser webviews only support http and https URLs';
      tabsState.updateTab(tab.id, { title: 'Load failed', is_loading: false });
      toastsState.error(message);
      return;
    }

    const historyPatch = historyPatchFor(tab, url);
    tabsState.updateTab(tab.id, {
      ...historyPatch,
      url,
      title: url,
      favicon_url: null,
      is_loading: true
    });

    try {
      if (electronRuntime) {
        const webview = electronWebviews.get(tab.webviewLabel);

        if (webview) {
          webview.src = url;
        } else if (creatingLabels.has(tab.webviewLabel)) {
          pendingNavigationUrls.set(tab.webviewLabel, url);
        }
      } else if (tauriRuntime) {
        if (createdLabels.has(tab.webviewLabel)) {
          await navigateBrowserWebview(tab.webviewLabel, url);
        } else if (creatingLabels.has(tab.webviewLabel)) {
          pendingNavigationUrls.set(tab.webviewLabel, url);
        }
      }

      await completeNavigation(tab.id, url, addHistoryEntry);
    } catch (error) {
      markTabLoadFailed(tab.id, errorMessage(error));
    }
  }

  async function reloadTab(tab: BrowserTab): Promise<void> {
    if (tab.isInternal || !isBrowserWebviewUrl(tab.url)) {
      return;
    }

    tabsState.updateTab(tab.id, { is_loading: true });

    try {
      if (electronRuntime) {
        electronWebviews.get(tab.webviewLabel)?.reload();
      } else if (tauriRuntime && createdLabels.has(tab.webviewLabel)) {
        await reloadBrowserWebview(tab.webviewLabel);
      }

      await completeNavigation(tab.id, tab.url, false);
    } catch (error) {
      markTabLoadFailed(tab.id, errorMessage(error));
    }
  }

  async function moveActiveHistory(direction: -1 | 1): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal) {
      return;
    }

    const historyIndex = activeTab.historyIndex + direction;
    const url = activeTab.history[historyIndex];

    if (!url) {
      return;
    }

    if (isInternalBrowserUrl(url)) {
      tabsState.updateTab(activeTab.id, {
        url,
        title: titleForInternalUrl(url),
        favicon_url: null,
        is_loading: false,
        can_go_back: historyIndex > 0,
        can_go_forward: historyIndex < activeTab.history.length - 1,
        isInternal: true,
        historyIndex
      });
      return;
    }

    if (!isBrowserWebviewUrl(url)) {
      toastsState.error(`Unsupported history entry: ${url}`);
      return;
    }

    // Back/forward tracks app-visible navigations and webview URL events. It is not the engine's
    // complete internal session history, so redirects and same-document navigations can differ.
    tabsState.updateTab(activeTab.id, {
      url,
      title: url,
      favicon_url: null,
      is_loading: true,
      isInternal: false,
      historyIndex,
      can_go_back: historyIndex > 0,
      can_go_forward: historyIndex < activeTab.history.length - 1
    });

    try {
      if (electronRuntime) {
        const webview = electronWebviews.get(activeTab.webviewLabel);

        if (webview) {
          webview.src = url;
        }
      } else if (tauriRuntime && createdLabels.has(activeTab.webviewLabel)) {
        await navigateBrowserWebview(activeTab.webviewLabel, url);
      }

      await completeNavigation(activeTab.id, url, true);
    } catch (error) {
      markTabLoadFailed(activeTab.id, errorMessage(error));
    }
  }

  async function completeNavigation(
    tabId: string,
    url: string,
    addHistoryEntry: boolean
  ): Promise<void> {
    if (!nativeRuntime) {
      tabsState.updateTab(tabId, { title: url, is_loading: false });
      return;
    }

    const token = (metadataTokens.get(tabId) ?? 0) + 1;
    metadataTokens.set(tabId, token);
    const metadata = await fetchPageMetadata(url);

    if (metadataTokens.get(tabId) !== token) {
      return;
    }

    const title = metadata.title.trim() || url;
    tabsState.updateTab(tabId, {
      title,
      favicon_url: metadata.favicon_url,
      is_loading: false
    });

    if (addHistoryEntry && !incognito) {
      await historyState.addEntry(title, url);
    }
  }

  function handleWebviewNavigation(payload: BrowserWebviewNavigationEvent): void {
    const tab = tabByLabel(payload.label);

    if (!tab || tab.url === payload.url) {
      return;
    }

    const historyPatch = historyPatchFor(tab, payload.url, {
      replaceCurrentLoadingEntry: true
    });
    tabsState.updateTab(tab.id, {
      ...historyPatch,
      url: payload.url,
      title: payload.url,
      favicon_url: null,
      is_loading: true
    });
    void completeNavigation(tab.id, payload.url, true).catch((error) => {
      markTabLoadFailed(tab.id, errorMessage(error));
    });
  }

  function handleWebviewTitle(payload: BrowserWebviewTitleEvent): void {
    const tab = tabByLabel(payload.label);
    const title = payload.title.trim();

    if (tab && title) {
      tabsState.updateTab(tab.id, { title });
    }
  }

  function handleWebviewLoad(payload: BrowserWebviewLoadEvent): void {
    const tab = tabByLabel(payload.label);

    if (!tab) {
      return;
    }

    if (payload.url && payload.url !== tab.url && isBrowserWebviewUrl(payload.url)) {
      handleWebviewNavigation({ label: payload.label, url: payload.url });
      return;
    }

    tabsState.updateTab(tab.id, {
      is_loading: payload.state === 'started'
    });
  }

  async function syncCreatedBounds(bounds: WebviewBounds): Promise<void> {
    if (!tauriRuntime || createdLabels.size === 0) {
      return;
    }

    await Promise.all(
      Array.from(createdLabels).map(async (label) => {
        try {
          await setBrowserWebviewBounds(label, bounds);
        } catch {
          createdLabels.delete(label);
        }
      })
    );
  }

  async function setWebviewVisibility(label: string, visible: boolean): Promise<void> {
    if (electronRuntime) {
      const webview = electronWebviews.get(label);

      if (webview) {
        webview.hidden = !visible;
      }

      return;
    }

    if (!tauriRuntime || !createdLabels.has(label)) {
      return;
    }

    try {
      if (visible) {
        await showBrowserWebview(label);
      } else {
        await hideBrowserWebview(label);
      }
    } catch {
      createdLabels.delete(label);
    }
  }

  async function closeManagedWebview(label: string): Promise<void> {
    createdLabels.delete(label);
    creatingLabels.delete(label);
    pendingNavigationUrls.delete(label);

    if (electronRuntime) {
      const webview = electronWebviews.get(label);
      webview?.remove();
      electronWebviews.delete(label);
      return;
    }

    if (!tauriRuntime) {
      return;
    }

    try {
      await hideBrowserWebview(label);
    } catch {
      // The native webview may already be gone after window teardown or hot reload.
    }

    try {
      await closeBrowserWebview(label);
    } catch {
      // The native webview may already be gone after window teardown or hot reload.
    }
  }

  async function closeEveryBrowserWebview(): Promise<void> {
    createdLabels.clear();
    creatingLabels.clear();
    pendingNavigationUrls.clear();

    if (electronRuntime) {
      electronWebviews.forEach((webview) => webview.remove());
      electronWebviews.clear();
      return;
    }

    if (!tauriRuntime) {
      return;
    }

    try {
      await closeAllBrowserWebviews();
    } catch (error) {
      reportNativeBridgeIssue(`Failed to close browser webviews: ${errorMessage(error)}`);
    }
  }

  async function findInElectronWebview(
    label: string,
    request: WebViewFindRequest
  ): Promise<FindResult> {
    const webview = electronWebviews.get(label);

    if (!webview?.findInPage) {
      return EMPTY_FIND_RESULT;
    }

    const findInPage = webview.findInPage.bind(webview);

    return new Promise((resolve) => {
      let settled = false;
      const timeout = window.setTimeout(() => {
        finish(EMPTY_FIND_RESULT);
      }, 1200);
      const handleFoundInPage = (event: Event) => {
        const result = (event as ElectronWebviewEvent).result;

        if (!result?.finalUpdate) {
          return;
        }

        finish({
          active_match: result.activeMatchOrdinal ?? 0,
          total_matches: result.matches ?? 0
        });
      };
      const finish = (result: FindResult) => {
        if (settled) {
          return;
        }

        settled = true;
        window.clearTimeout(timeout);
        webview.removeEventListener('found-in-page', handleFoundInPage);
        resolve(result);
      };

      webview.addEventListener('found-in-page', handleFoundInPage);
      findInPage(request.query, {
        forward: request.direction === 'next',
        matchCase: request.caseSensitive
      });
    });
  }

  function electronWebviewUrl(webview: ElectronWebviewElement): string {
    return webview.getURL?.() || webview.src;
  }

  function historyPatchFor(
    tab: BrowserTab,
    url: string,
    options: { replaceCurrentLoadingEntry?: boolean } = {}
  ): Partial<BrowserTab> {
    const history = tab.history.slice(0, tab.historyIndex + 1);

    if (shouldReplaceCurrentHistoryEntry(tab, url, options)) {
      history[tab.historyIndex] = url;
    } else if (history.at(-1) !== url) {
      history.push(url);
    }

    const historyIndex = history.length - 1;
    return {
      history,
      historyIndex,
      can_go_back: historyIndex > 0,
      can_go_forward: false
    };
  }

  function shouldReplaceCurrentHistoryEntry(
    tab: BrowserTab,
    url: string,
    options: { replaceCurrentLoadingEntry?: boolean }
  ): boolean {
    if (!options.replaceCurrentLoadingEntry || !tab.is_loading) {
      return false;
    }

    return tab.history[tab.historyIndex] === tab.url && tab.url !== url;
  }

  function boundsFromElement(element: HTMLElement): WebviewBounds | null {
    const rect = element.getBoundingClientRect();
    const appContentRect = element.closest('.app-content')?.getBoundingClientRect();
    const safeTop = appContentRect ? Math.max(rect.top, appContentRect.top) : rect.top;
    const safeLeft = appContentRect ? Math.max(rect.left, appContentRect.left) : rect.left;
    const safeRight = appContentRect ? Math.min(rect.right, appContentRect.right) : rect.right;
    const safeBottom = appContentRect ? Math.min(rect.bottom, appContentRect.bottom) : rect.bottom;
    const safeWidth = safeRight - safeLeft;
    const safeHeight = safeBottom - safeTop;

    if (
      !Number.isFinite(safeLeft) ||
      !Number.isFinite(safeTop) ||
      !Number.isFinite(safeWidth) ||
      !Number.isFinite(safeHeight) ||
      safeWidth <= 0 ||
      safeHeight <= 0
    ) {
      return null;
    }

    return {
      x: roundCoordinate(safeLeft),
      y: roundCoordinate(safeTop),
      width: roundCoordinate(safeWidth),
      height: roundCoordinate(safeHeight)
    };
  }

  function boundsEqual(left: WebviewBounds, right: WebviewBounds | null): boolean {
    if (!right) {
      return false;
    }

    return (
      left.x === right.x &&
      left.y === right.y &&
      left.width === right.width &&
      left.height === right.height
    );
  }

  function roundCoordinate(value: number): number {
    return Math.round(value * 100) / 100;
  }

  function tabByLabel(label: string): BrowserTab | null {
    return tabsState.tabs.find((tab) => tab.webviewLabel === label) ?? null;
  }

  function isTabOpen(tabId: string): boolean {
    return tabsState.tabs.some((tab) => tab.id === tabId);
  }

  function isBrowserWebviewUrl(url: string): boolean {
    try {
      const parsed = new URL(url);
      return parsed.protocol === 'http:' || parsed.protocol === 'https:';
    } catch {
      return false;
    }
  }

  function markTabLoadFailed(tabId: string, message: string): void {
    tabsState.updateTab(tabId, {
      title: 'Load failed',
      is_loading: false
    });
    toastsState.error(`Failed to load page: ${message}`);
  }

  function reportNativeBridgeIssue(message: string): void {
    if (hasReportedNativeBridgeIssue) {
      return;
    }

    hasReportedNativeBridgeIssue = true;
    toastsState.error(message);
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

<section class="webview-container" aria-label="Web page content" bind:this={containerElement}>
  {#if !nativeRuntime}
    <div class="webview-empty" role="status">
      <AlertTriangle size={22} strokeWidth={2.1} />
      <span>Open in the desktop app to show native webviews</span>
    </div>
  {:else if !tabsState.activeTab}
    <div class="webview-empty" role="status">
      <PanelTop size={24} strokeWidth={2} />
      <span>Open a tab to browse</span>
    </div>
  {:else if tabsState.activeTab.isInternal}
    <div class="webview-empty" role="status">
      <PanelTop size={24} strokeWidth={2} />
      <span>{tabsState.activeTab.title}</span>
    </div>
  {/if}
</section>

<style>
  .webview-container {
    position: absolute;
    inset: 0;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--color-background);
  }

  :global(.electron-webview) {
    position: absolute;
    inset: 0;
    display: block;
    width: 100%;
    height: 100%;
    border: 0;
    background: #ffffff;
  }

  :global(.electron-webview[hidden]) {
    display: none;
  }

  .webview-empty {
    display: grid;
    grid-auto-flow: row;
    gap: 10px;
    place-items: center;
    width: 100%;
    height: 100%;
    padding: 24px;
    color: var(--color-text-secondary);
    text-align: center;
  }

  .webview-empty span {
    max-width: min(360px, 100%);
    overflow-wrap: anywhere;
    font-size: 13px;
    font-weight: 650;
    letter-spacing: 0;
    line-height: 1.35;
  }
</style>
