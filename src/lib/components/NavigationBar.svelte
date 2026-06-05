<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    Bookmark,
    BookmarkCheck,
    Download,
    History,
    Lock,
    Menu,
    PanelTop,
    RotateCw,
    Search,
    Settings,
    Star,
    X
  } from 'lucide-svelte';
  import { bookmarksState } from '../state/bookmarks.svelte';
  import { historyState } from '../state/history.svelte';
  import { resolveHomepageUrl, settingsState } from '../state/settings.svelte';
  import { tabsState } from '../state/tabs.svelte';
  import { toastsState } from '../state/toasts.svelte';
  import { BROWSER_HOME_URL } from '../utils/internalUrls';
  import { normalizeUrl, type HistoryEntry } from '../utils/tauri';

  type ChromePanel = 'bookmarks' | 'history' | 'downloads' | 'settings' | 'find';

  interface Props {
    onNavigate: (url: string) => Promise<void>;
    onReload: () => Promise<void>;
    onStop: () => Promise<void>;
    onBack: () => Promise<void>;
    onForward: () => Promise<void>;
    onOpenPanel: (panel: ChromePanel) => void;
    focusAddressRequest?: number;
  }

  let {
    onNavigate,
    onReload,
    onStop,
    onBack,
    onForward,
    onOpenPanel,
    focusAddressRequest = 0
  }: Props = $props();

  let addressInput = $state('');
  let addressElement: HTMLInputElement | undefined = $state();
  let isFocused = $state(false);
  let validationError = $state('');
  let suggestions = $state<HistoryEntry[]>([]);
  let isSuggestionsOpen = $state(false);
  let isSubmitting = $state(false);
  let isBookmarkChecked = $state(false);
  let isBookmarkBusy = $state(false);
  let isMenuOpen = $state(false);
  let lastFocusAddressRequest = 0;
  let lastActiveTabId: string | null = null;

  let activeTab = $derived(tabsState.activeTab);
  let activeUrl = $derived(activeTab?.url ?? '');
  let activeDisplayUrl = $derived(activeUrl === BROWSER_HOME_URL ? '' : activeUrl);
  let activeTabId = $derived(activeTab?.id ?? null);
  let canBookmark = $derived(isBookmarkableUrl(activeUrl));
  let addressIcon = $derived(iconKindForUrl(addressInput || activeDisplayUrl));

  $effect(() => {
    const tabId = activeTabId;
    const url = activeUrl;

    if (tabId !== lastActiveTabId) {
      lastActiveTabId = tabId;
      addressInput = url === BROWSER_HOME_URL ? '' : url;
      validationError = '';
      isSuggestionsOpen = false;
      return;
    }

    if (!isFocused) {
      addressInput = url === BROWSER_HOME_URL ? '' : url;
      validationError = '';
    }
  });

  $effect(() => {
    const request = focusAddressRequest;

    if (request <= 0 || request === lastFocusAddressRequest) {
      return;
    }

    lastFocusAddressRequest = request;
    focusAddressInput();
  });

  $effect(() => {
    const query = addressInput.trim();

    if (!isFocused || query.length < 2) {
      suggestions = [];
      isSuggestionsOpen = false;
      return;
    }

    let cancelled = false;
    const timeout = window.setTimeout(() => {
      void (async () => {
        const results = await historyState.suggest(query, 6);

        if (!cancelled) {
          suggestions = results;
          isSuggestionsOpen = results.length > 0;
        }
      })();
    }, 150);

    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  });

  $effect(() => {
    const url = activeUrl;

    if (!isBookmarkableUrl(url)) {
      isBookmarkChecked = false;
      isBookmarkBusy = false;
      return;
    }

    let cancelled = false;
    isBookmarkBusy = true;

    void (async () => {
      const checked = await bookmarksState.checkIsBookmarked(url);

      if (!cancelled) {
        isBookmarkChecked = checked;
        isBookmarkBusy = false;
      }
    })();

    return () => {
      cancelled = true;
    };
  });

  function handleAddressFocus(): void {
    prepareAddressFocus();

    window.setTimeout(() => {
      addressElement?.select();
    }, 0);
  }

  function focusAddressInput(): void {
    prepareAddressFocus();

    window.setTimeout(() => {
      addressElement?.focus();
      addressElement?.select();
    }, 0);
  }

  function prepareAddressFocus(): void {
    isFocused = true;
    addressInput = activeUrl === BROWSER_HOME_URL ? '' : activeUrl;
    validationError = '';
  }

  function handleAddressBlur(): void {
    isFocused = false;

    window.setTimeout(() => {
      isSuggestionsOpen = false;
    }, 120);
  }

  function handleAddressInput(event: Event): void {
    addressInput = (event.currentTarget as HTMLInputElement).value;
    validationError = '';
  }

  function handleAddressKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter') {
      event.preventDefault();

      if (event.ctrlKey && event.shiftKey) {
        toastsState.error('Opening the address in a new window is not implemented.');
        return;
      }

      if (event.ctrlKey) {
        void submitNavigation(siteNameShortcutUrl(addressInput));
        return;
      }

      if (event.altKey) {
        void submitNavigationInNewTab(addressInput);
        return;
      }

      void submitNavigation(addressInput);
      return;
    }

    if (event.key === 'Escape') {
      event.preventDefault();
      validationError = '';
      isSuggestionsOpen = false;
      addressElement?.blur();
    }
  }

  async function submitNavigationInNewTab(rawInput: string): Promise<void> {
    const rawUrl = rawInput.trim();

    if (!rawUrl) {
      validationError = 'Enter a URL or search query';
      return;
    }

    isSubmitting = true;
    validationError = '';

    try {
      const normalizedUrl = await normalizeUrl(rawUrl);
      tabsState.openTab(normalizedUrl, normalizedUrl);
      addressInput = normalizedUrl;
      isSuggestionsOpen = false;
      addressElement?.blur();
    } catch (error) {
      validationError = errorMessage(error);
    } finally {
      isSubmitting = false;
    }
  }

  async function submitNavigation(rawInput: string): Promise<void> {
    const rawUrl = rawInput.trim();

    if (!rawUrl) {
      validationError = 'Enter a URL or search query';
      return;
    }

    isSubmitting = true;
    validationError = '';

    try {
      const normalizedUrl = await normalizeUrl(rawUrl);
      await onNavigate(normalizedUrl);
      addressInput = normalizedUrl;
      isSuggestionsOpen = false;
      addressElement?.blur();
    } catch (error) {
      validationError = errorMessage(error);
    } finally {
      isSubmitting = false;
    }
  }

  function selectSuggestion(event: PointerEvent, entry: HistoryEntry): void {
    event.preventDefault();
    void submitNavigation(entry.url);
  }

  async function handleReloadOrStop(): Promise<void> {
    if (activeTab?.is_loading) {
      await onStop();
      return;
    }

    await onReload();
  }

  async function toggleBookmark(): Promise<void> {
    const tab = activeTab;

    if (!tab || !isBookmarkableUrl(tab.url)) {
      return;
    }

    isBookmarkBusy = true;

    try {
      if (isBookmarkChecked) {
        const removed = await bookmarksState.removeBookmarkByUrl(tab.url);
        isBookmarkChecked = !removed;
        return;
      }

      const bookmark = await bookmarksState.addBookmark(tab.title.trim() || tab.url, tab.url);
      isBookmarkChecked = Boolean(bookmark);
    } finally {
      isBookmarkBusy = false;
    }
  }

  function openMenuPanel(panel: ChromePanel): void {
    onOpenPanel(panel);
    isMenuOpen = false;
  }

  function openNewTab(): void {
    tabsState.openTab(resolveHomepageUrl(settingsState.settings.homepage));
    isMenuOpen = false;
  }

  function handleWindowClick(): void {
    isMenuOpen = false;
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      isMenuOpen = false;
    }
  }

  async function runCommand(command: () => Promise<void>): Promise<void> {
    try {
      await command();
    } catch (error) {
      toastsState.error(`Navigation action failed: ${errorMessage(error)}`);
    }
  }

  function isBookmarkableUrl(url: string): boolean {
    return url.startsWith('http://') || url.startsWith('https://');
  }

  function iconKindForUrl(url: string): 'secure' | 'search' | 'internal' {
    if (url.startsWith('https://')) {
      return 'secure';
    }

    if (url.startsWith('browser://')) {
      return 'internal';
    }

    return 'search';
  }

  function siteNameShortcutUrl(rawInput: string): string {
    const trimmed = rawInput.trim();

    if (!trimmed || trimmed.includes('://') || /\s/.test(trimmed)) {
      return trimmed;
    }

    const siteName = trimmed.replace(/^www\./i, '').replace(/\.com$/i, '');

    if (!siteName) {
      return trimmed;
    }

    return `https://www.${siteName}.com`;
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

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} />

<nav class="navigation-bar" aria-label="Navigation controls">
  <div class="history-controls" aria-label="Page history">
    <button
      class="toolbar-button"
      type="button"
      aria-label="Back"
      title="Back"
      disabled={!activeTab?.can_go_back}
      onclick={() => runCommand(onBack)}
    >
      <ArrowLeft size={18} strokeWidth={2.25} />
    </button>
    <button
      class="toolbar-button"
      type="button"
      aria-label="Forward"
      title="Forward"
      disabled={!activeTab?.can_go_forward}
      onclick={() => runCommand(onForward)}
    >
      <ArrowRight size={18} strokeWidth={2.25} />
    </button>
    <button
      class="toolbar-button"
      type="button"
      aria-label={activeTab?.is_loading ? 'Stop loading' : 'Reload'}
      title={activeTab?.is_loading ? 'Stop' : 'Reload'}
      disabled={!activeTab}
      onclick={() => runCommand(handleReloadOrStop)}
    >
      {#if activeTab?.is_loading}
        <X size={17} strokeWidth={2.35} />
      {:else}
        <RotateCw size={17} strokeWidth={2.25} />
      {/if}
    </button>
  </div>

  <form
    class="address-form"
    aria-label="Address and search"
    onsubmit={(event) => event.preventDefault()}
  >
    <div class={`address-box ${validationError ? 'address-box--error' : ''}`}>
      <span class="address-box__icon" aria-hidden="true">
        {#if addressIcon === 'secure'}
          <Lock size={15} strokeWidth={2.3} />
        {:else if addressIcon === 'internal'}
          <PanelTop size={15} strokeWidth={2.25} />
        {:else}
          <Search size={15} strokeWidth={2.25} />
        {/if}
      </span>

      <input
        bind:this={addressElement}
        class="address-input"
        type="text"
        value={addressInput}
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        aria-label="Search or enter address"
        aria-invalid={validationError ? 'true' : 'false'}
        aria-describedby={validationError ? 'address-error' : undefined}
        placeholder="Search or enter address"
        onfocus={handleAddressFocus}
        onblur={handleAddressBlur}
        oninput={handleAddressInput}
        onkeydown={handleAddressKeydown}
      />

      {#if isSubmitting}
        <span class="address-box__status" aria-label="Navigating"></span>
      {/if}
    </div>

    {#if validationError}
      <div class="address-error" id="address-error" role="alert">{validationError}</div>
    {/if}

    {#if isSuggestionsOpen && suggestions.length > 0}
      <div class="suggestions" role="listbox" aria-label="History suggestions">
        {#each suggestions as suggestion (suggestion.id)}
          <button
            class="suggestion"
            type="button"
            role="option"
            aria-selected="false"
            onpointerdown={(event) => selectSuggestion(event, suggestion)}
          >
            <History size={15} strokeWidth={2.2} />
            <span class="suggestion__text">
              <span class="suggestion__title">{suggestion.title || suggestion.url}</span>
              <span class="suggestion__url">{suggestion.url}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </form>

  <div class="page-actions" aria-label="Page actions">
    <button
      class={`toolbar-button star-button ${isBookmarkChecked ? 'star-button--saved' : ''}`}
      type="button"
      aria-label={isBookmarkChecked ? 'Remove bookmark' : 'Bookmark page'}
      title={isBookmarkChecked ? 'Remove bookmark' : 'Bookmark page'}
      disabled={!canBookmark || isBookmarkBusy}
      onclick={() => runCommand(toggleBookmark)}
    >
      {#if isBookmarkChecked}
        <BookmarkCheck size={18} strokeWidth={2.25} />
      {:else}
        <Star size={18} strokeWidth={2.25} />
      {/if}
    </button>

    <div class="menu-anchor">
      <button
        class="toolbar-button"
        type="button"
        aria-label="Open browser menu"
        aria-expanded={isMenuOpen}
        title="Menu"
        onclick={(event) => {
          event.stopPropagation();
          isMenuOpen = !isMenuOpen;
        }}
      >
        <Menu size={19} strokeWidth={2.25} />
      </button>

      {#if isMenuOpen}
        <div class="browser-menu" role="menu">
          <button class="browser-menu__item" type="button" role="menuitem" onclick={openNewTab}>
            <PanelTop size={16} strokeWidth={2.25} />
            <span>New Tab</span>
          </button>
          <button
            class="browser-menu__item"
            type="button"
            role="menuitem"
            onclick={() => openMenuPanel('bookmarks')}
          >
            <Bookmark size={16} strokeWidth={2.25} />
            <span>Bookmarks</span>
          </button>
          <button
            class="browser-menu__item"
            type="button"
            role="menuitem"
            onclick={() => openMenuPanel('history')}
          >
            <History size={16} strokeWidth={2.25} />
            <span>History</span>
          </button>
          <button
            class="browser-menu__item"
            type="button"
            role="menuitem"
            onclick={() => openMenuPanel('downloads')}
          >
            <Download size={16} strokeWidth={2.25} />
            <span>Downloads</span>
          </button>
          <button
            class="browser-menu__item"
            type="button"
            role="menuitem"
            onclick={() => openMenuPanel('settings')}
          >
            <Settings size={16} strokeWidth={2.25} />
            <span>Settings</span>
          </button>
          <button
            class="browser-menu__item"
            type="button"
            role="menuitem"
            onclick={() => openMenuPanel('find')}
          >
            <Search size={16} strokeWidth={2.25} />
            <span>Find in Page</span>
          </button>
        </div>
      {/if}
    </div>
  </div>
</nav>

<style>
  .navigation-bar {
    display: grid;
    grid-template-columns: auto minmax(180px, 1fr) auto;
    gap: 8px;
    align-items: center;
    width: 100%;
    height: var(--chrome-nav-height);
    min-height: var(--chrome-nav-height);
    padding: 6px 8px;
    color: var(--color-text-primary);
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.04), transparent),
      color-mix(in srgb, var(--color-surface) 94%, var(--color-background));
    border-bottom: 1px solid var(--color-border);
    user-select: none;
  }

  .history-controls,
  .page-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .toolbar-button {
    display: grid;
    width: 34px;
    height: 34px;
    min-width: 34px;
    min-height: 34px;
    place-items: center;
    padding: 0;
    color: var(--color-text-secondary);
    background: transparent;
    border: 0;
    border-radius: 8px;
    cursor: default;
  }

  .toolbar-button:hover:not(:disabled) {
    color: var(--color-text-primary);
    background: rgb(255 255 255 / 0.085);
  }

  .toolbar-button:disabled {
    color: color-mix(in srgb, var(--color-text-secondary) 42%, transparent);
  }

  .star-button--saved {
    color: var(--color-warning);
  }

  .address-form {
    position: relative;
    min-width: 0;
  }

  .address-box {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) 22px;
    align-items: center;
    height: 34px;
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    background: color-mix(in srgb, var(--color-background) 72%, var(--color-surface));
    border: 1px solid color-mix(in srgb, var(--color-border) 86%, transparent);
    border-radius: 8px;
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.035);
  }

  .address-box:focus-within {
    color: var(--color-text-primary);
    border-color: color-mix(in srgb, var(--color-accent) 76%, var(--color-border));
    box-shadow:
      0 0 0 2px color-mix(in srgb, var(--color-accent) 20%, transparent),
      inset 0 1px 0 rgb(255 255 255 / 0.045);
  }

  .address-box--error {
    border-color: var(--color-danger);
  }

  .address-box__icon {
    display: grid;
    width: 28px;
    place-items: center;
  }

  .address-input {
    min-width: 0;
    height: 100%;
    padding: 0;
    overflow: hidden;
    color: var(--color-text-primary);
    text-overflow: ellipsis;
    background: transparent;
    border: 0;
    outline: none;
  }

  .address-input::placeholder {
    color: color-mix(in srgb, var(--color-text-secondary) 72%, transparent);
  }

  .address-box__status {
    justify-self: center;
    width: 13px;
    height: 13px;
    border: 2px solid color-mix(in srgb, var(--color-accent) 26%, transparent);
    border-top-color: var(--color-accent);
    border-radius: 999px;
    animation: address-spin 720ms linear infinite;
  }

  .address-error,
  .suggestions,
  .browser-menu {
    position: absolute;
    z-index: 920;
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-surface-elevated) 96%, transparent);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    box-shadow: var(--shadow-elevated);
    backdrop-filter: blur(18px);
  }

  .address-error {
    top: calc(100% + 6px);
    left: 0;
    max-width: min(420px, 100%);
    padding: 8px 10px;
    color: var(--color-danger);
    font-size: 12px;
    font-weight: 650;
    line-height: 1.25;
  }

  .suggestions {
    top: calc(100% + 6px);
    right: 0;
    left: 0;
    display: grid;
    gap: 2px;
    max-height: min(344px, calc(100vh - 144px));
    padding: 6px;
    overflow-y: auto;
  }

  .suggestion {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    width: 100%;
    min-height: 42px;
    padding: 6px 8px;
    color: var(--color-text-secondary);
    text-align: left;
    background: transparent;
    border: 0;
    border-radius: 6px;
    cursor: default;
  }

  .suggestion:hover {
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
  }

  .suggestion__text {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .suggestion__title,
  .suggestion__url {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .suggestion__title {
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0;
  }

  .suggestion__url {
    font-size: 11px;
    letter-spacing: 0;
  }

  .menu-anchor {
    position: relative;
  }

  .browser-menu {
    top: calc(100% + 7px);
    right: 0;
    display: grid;
    gap: 2px;
    width: 204px;
    padding: 6px;
  }

  .browser-menu__item {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    width: 100%;
    height: 34px;
    padding: 0 8px;
    color: var(--color-text-primary);
    text-align: left;
    background: transparent;
    border: 0;
    border-radius: 6px;
    cursor: default;
  }

  .browser-menu__item span {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .browser-menu__item:hover {
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
  }

  :global(.app-shell[data-theme='light']) .navigation-bar {
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.84), transparent),
      color-mix(in srgb, var(--color-surface) 96%, var(--color-background));
  }

  :global(.app-shell[data-theme='light']) .toolbar-button:hover:not(:disabled) {
    background: rgb(0 0 0 / 0.065);
  }

  :global(.app-shell[data-theme='light']) .address-box {
    background: color-mix(in srgb, var(--color-surface) 84%, var(--color-background));
  }

  @media (max-width: 560px) {
    .navigation-bar {
      grid-template-columns: auto minmax(112px, 1fr) auto;
      gap: 4px;
      padding-inline: 5px;
    }

    .toolbar-button {
      width: 31px;
      min-width: 31px;
    }

    .history-controls,
    .page-actions {
      gap: 1px;
    }
  }

  @keyframes address-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
