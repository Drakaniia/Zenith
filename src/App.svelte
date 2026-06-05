<script lang="ts">
  import TabBar from './lib/components/TabBar.svelte';
  import BookmarksPanel from './lib/components/BookmarksPanel.svelte';
  import HistoryPanel from './lib/components/HistoryPanel.svelte';
  import NavigationBar from './lib/components/NavigationBar.svelte';
  import FindBar from './lib/components/FindBar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import TitleBar from './lib/components/TitleBar.svelte';
  import Toast from './lib/components/Toast.svelte';
  import WebViewContainer, {
    type WebViewController
  } from './lib/components/WebViewContainer.svelte';
  import { settingsState } from './lib/state/settings.svelte';
  import { tabsState } from './lib/state/tabs.svelte';
  import type { FindDirection, FindResult } from './lib/utils/tauri';

  type ChromePanel = 'bookmarks' | 'history' | 'downloads' | 'settings' | 'find';

  let activePanel = $state<ChromePanel | null>(null);
  let webViewController = $state<WebViewController | null>(null);
  let theme = $derived(settingsState.settings.theme === 'light' ? 'light' : 'dark');
  let title = $derived(tabsState.activeTab?.title?.trim() || 'Rust Browser');
  let showStatusBar = $derived(settingsState.settings.show_status_bar);
  let isLibraryPanelOpen = $derived(activePanel === 'bookmarks' || activePanel === 'history');

  const EMPTY_FIND_RESULT: FindResult = {
    active_match: 0,
    total_matches: 0
  };

  async function navigateActive(url: string): Promise<void> {
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
    await webViewController?.backActive();
  }

  async function forwardActive(): Promise<void> {
    await webViewController?.forwardActive();
  }

  function openPanel(panel: ChromePanel): void {
    activePanel = activePanel === panel ? null : panel;
  }

  function closeFindBar(): void {
    activePanel = null;
  }

  function closePanel(): void {
    activePanel = null;
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
</script>

<svelte:head>
  <meta name="color-scheme" content="dark light" />
</svelte:head>

<div class="app-shell" data-theme={theme}>
  <TitleBar {title} />
  <TabBar />
  <NavigationBar
    onNavigate={navigateActive}
    onReload={reloadActive}
    onStop={stopActive}
    onBack={backActive}
    onForward={forwardActive}
    onOpenPanel={openPanel}
  />
  <FindBar
    open={activePanel === 'find'}
    onClose={closeFindBar}
    onFind={findActive}
    onClear={clearFindActive}
  />
  <main
    class={`app-content ${isLibraryPanelOpen ? 'app-content--with-library' : ''}`}
    aria-label="Browser content"
    data-active-panel={activePanel ?? 'none'}
  >
    <div class="webview-slot">
      <WebViewContainer bind:controller={webViewController} />
    </div>
    <BookmarksPanel
      open={activePanel === 'bookmarks'}
      onClose={closePanel}
      onNavigate={navigateActive}
    />
    <HistoryPanel open={activePanel === 'history'} onClose={closePanel} onNavigate={navigateActive} />
  </main>
  {#if showStatusBar}
    <StatusBar />
  {/if}
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
    --chrome-statusbar-height: 24px;
    --chrome-content-height: calc(
      100vh - var(--chrome-tabbar-height) - var(--chrome-nav-height) -
        var(--chrome-statusbar-height)
    );
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
    grid-template-rows:
      auto var(--chrome-tabbar-height) var(--chrome-nav-height) auto minmax(0, 1fr)
      auto;
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
