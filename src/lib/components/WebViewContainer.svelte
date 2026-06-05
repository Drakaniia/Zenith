<script module lang="ts">
  import type { FindDirection, FindResult as TauriFindResult } from '../utils/tauri';

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
  }
</script>

<script lang="ts">
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { AlertTriangle, PanelTop } from 'lucide-svelte';
  import { historyState } from '../state/history.svelte';
  import { settingsState } from '../state/settings.svelte';
  import { type BrowserTab, tabsState } from '../state/tabs.svelte';
  import { toastsState } from '../state/toasts.svelte';
  import {
    BROWSER_WEBVIEW_LOAD_EVENT,
    BROWSER_WEBVIEW_NAVIGATION_EVENT,
    BROWSER_WEBVIEW_TITLE_EVENT,
    clearBrowserWebviewFind,
    closeBrowserWebview,
    createBrowserWebview,
    evalBrowserWebview,
    fetchPageMetadata,
    findInBrowserWebview,
    hideBrowserWebview,
    isTauriRuntime,
    navigateBrowserWebview,
    reloadBrowserWebview,
    setBrowserWebviewBounds,
    setBrowserWebviewZoom,
    showBrowserWebview,
    type BrowserWebviewLoadEvent,
    type BrowserWebviewNavigationEvent,
    type BrowserWebviewTitleEvent,
    type FindResult,
    type WebviewBounds
  } from '../utils/tauri';

  interface Props {
    controller?: WebViewController | null;
    zoomScale?: number;
    incognito?: boolean;
  }

  const EMPTY_FIND_RESULT: FindResult = {
    active_match: 0,
    total_matches: 0
  };

  let {
    controller = $bindable<WebViewController | null>(null),
    zoomScale = 1,
    incognito = false
  }: Props = $props();

  const nativeRuntime = isTauriRuntime();
  const createdLabels = new Set<string>();
  const creatingLabels = new Set<string>();
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
      setActiveZoom
    };

    return () => {
      controller = null;
    };
  });

  $effect(() => {
    if (!nativeRuntime) {
      return;
    }

    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    void (async () => {
      try {
        const registered = await Promise.all([
          listen<BrowserWebviewNavigationEvent>(BROWSER_WEBVIEW_NAVIGATION_EVENT, (event) => {
            handleWebviewNavigation(event.payload);
          }),
          listen<BrowserWebviewTitleEvent>(BROWSER_WEBVIEW_TITLE_EVENT, (event) => {
            handleWebviewTitle(event.payload);
          }),
          listen<BrowserWebviewLoadEvent>(BROWSER_WEBVIEW_LOAD_EVENT, (event) => {
            handleWebviewLoad(event.payload);
          })
        ]);

        if (cancelled) {
          registered.forEach((unlisten) => unlisten());
          return;
        }

        unlisteners.push(...registered);
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

    void syncWebviews(tabs, activeTabId, bounds, javascriptEnabled, incognitoMode);
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
      if (nativeRuntime && createdLabels.has(activeTab.webviewLabel)) {
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

    await clearBrowserWebviewFind(activeTab.webviewLabel);
  }

  async function setActiveZoom(scaleFactor: number): Promise<void> {
    const activeTab = tabsState.activeTab;

    if (!activeTab || activeTab.isInternal || !nativeRuntime) {
      return;
    }

    if (!createdLabels.has(activeTab.webviewLabel)) {
      return;
    }

    await setBrowserWebviewZoom(activeTab.webviewLabel, scaleFactor);
  }

  async function syncWebviews(
    tabs: BrowserTab[],
    activeTabId: string | null,
    bounds: WebviewBounds | null,
    javascriptEnabled: boolean,
    incognitoMode: boolean
  ): Promise<void> {
    const version = ++syncVersion;
    const remoteTabs = tabs.filter((tab) => !tab.isInternal && isBrowserWebviewUrl(tab.url));
    const remoteLabels = new Set<string>(remoteTabs.map((tab) => tab.webviewLabel));

    await Promise.all(
      Array.from(createdLabels)
        .filter((label) => !remoteLabels.has(label))
        .map((label) => closeManagedWebview(label))
    );

    if (!nativeRuntime || !bounds) {
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
        setWebviewVisibility(tab.webviewLabel, tab.id === activeTabId && !tab.isInternal)
      )
    );
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
      // Remote pages live in child webviews. Only the trusted "main" chrome webview imports and
      // invokes Tauri command wrappers; remote labels are not granted capabilities.
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
      if (nativeRuntime) {
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
      if (nativeRuntime && createdLabels.has(tab.webviewLabel)) {
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

    // Back/forward tracks app-visible navigations and Tauri URL events. It is not the engine's
    // complete internal session history, so redirects and same-document navigations can differ.
    tabsState.updateTab(activeTab.id, {
      url,
      title: url,
      is_loading: true,
      historyIndex,
      can_go_back: historyIndex > 0,
      can_go_forward: historyIndex < activeTab.history.length - 1
    });

    try {
      if (nativeRuntime && createdLabels.has(activeTab.webviewLabel)) {
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

    const historyPatch = historyPatchFor(tab, payload.url);
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
    if (!nativeRuntime || createdLabels.size === 0) {
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
    if (!nativeRuntime || !createdLabels.has(label)) {
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

    if (!nativeRuntime) {
      return;
    }

    try {
      await closeBrowserWebview(label);
    } catch {
      // The native webview may already be gone after window teardown or hot reload.
    }
  }

  function historyPatchFor(tab: BrowserTab, url: string): Partial<BrowserTab> {
    const history = tab.history.slice(0, tab.historyIndex + 1);
    if (history.at(-1) !== url) {
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

  function boundsFromElement(element: HTMLElement): WebviewBounds | null {
    const rect = element.getBoundingClientRect();

    if (
      !Number.isFinite(rect.left) ||
      !Number.isFinite(rect.top) ||
      !Number.isFinite(rect.width) ||
      !Number.isFinite(rect.height) ||
      rect.width <= 0 ||
      rect.height <= 0
    ) {
      return null;
    }

    return {
      x: roundCoordinate(rect.left),
      y: roundCoordinate(rect.top),
      width: roundCoordinate(rect.width),
      height: roundCoordinate(rect.height)
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
      <span>Open in Tauri to show native webviews</span>
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
