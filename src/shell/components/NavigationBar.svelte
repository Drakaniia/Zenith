<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    BookmarkCheck,
    History,
    Lock,
    PanelTop,
    RotateCw,
    Search,
    Star,
    X
  } from 'lucide-svelte';
  import { normalizeUrl } from '../../bridge/commands/navigation';
  import { bookmarksState } from '../../features/bookmarks/state/bookmarks.svelte';
  import { historyState } from '../../features/history/state/history.svelte';
  import type { SearchEngine } from '../../features/settings/state/settings.svelte';
  import { tabsState } from '../../features/tabs/state/tabs.svelte';
  import type { HistoryEntry } from '../../shared/types/backend';
  import { BROWSER_HOME_URL } from '../../shared/utils/internal-urls';
  import { toastsState } from '../state/toasts.svelte';

  interface Props {
    onNavigate: (url: string) => Promise<void>;
    onReload: () => Promise<void>;
    onStop: () => Promise<void>;
    onBack: () => Promise<void>;
    onForward: () => Promise<void>;
    focusAddressRequest?: number;
    focusToolbarStartRequest?: number;
  }

  let {
    onNavigate,
    onReload,
    onStop,
    onBack,
    onForward,
    focusAddressRequest = 0,
    focusToolbarStartRequest = 0
  }: Props = $props();

  const isApplePlatform = /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);

  let addressInput = $state('');
  let addressElement: HTMLInputElement | undefined = $state();
  let backButtonElement: HTMLButtonElement | undefined = $state();
  let isFocused = $state(false);
  let validationError = $state('');
  let suggestions = $state<HistoryEntry[]>([]);
  let selectedSuggestionIndex = $state(-1);
  let isSuggestionsOpen = $state(false);
  let isSubmitting = $state(false);
  let isBookmarkChecked = $state(false);
  let isBookmarkBusy = $state(false);
  let searchEngineMode = $state<SearchEngine | null>(null);
  let lastFocusAddressRequest = 0;
  let lastFocusToolbarStartRequest = 0;
  let lastActiveTabId: string | null = null;

  let activeTab = $derived(tabsState.activeTab);
  let activeUrl = $derived(activeTab?.url ?? '');
  let activeDisplayUrl = $derived(activeUrl === BROWSER_HOME_URL ? '' : activeUrl);
  let activeTabId = $derived(activeTab?.id ?? null);
  let canGoBack = $derived(Boolean(activeTab?.can_go_back));
  let canGoForward = $derived(Boolean(activeTab?.can_go_forward));
  let canBookmark = $derived(isBookmarkableUrl(activeUrl));
  let addressIcon = $derived(iconKindForUrl(addressInput || activeDisplayUrl));
  let addressPlaceholder = $derived(
    searchEngineMode ? `Search ${searchEngineLabel(searchEngineMode)}` : 'Search or enter address'
  );

  $effect(() => {
    const tabId = activeTabId;
    const url = activeUrl;

    if (tabId !== lastActiveTabId) {
      lastActiveTabId = tabId;
      addressInput = url === BROWSER_HOME_URL ? '' : url;
      validationError = '';
      isSuggestionsOpen = false;
      selectedSuggestionIndex = -1;
      searchEngineMode = null;
      return;
    }

    if (!isFocused) {
      addressInput = url === BROWSER_HOME_URL ? '' : url;
      validationError = '';
      selectedSuggestionIndex = -1;
      searchEngineMode = null;
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
    const request = focusToolbarStartRequest;

    if (request <= 0 || request === lastFocusToolbarStartRequest) {
      return;
    }

    lastFocusToolbarStartRequest = request;
    window.setTimeout(() => {
      backButtonElement?.focus();
    }, 0);
  });

  $effect(() => {
    const query = addressInput.trim();

    if (!isFocused || searchEngineMode || query.length < 2) {
      suggestions = [];
      isSuggestionsOpen = false;
      selectedSuggestionIndex = -1;
      return;
    }

    let cancelled = false;
    const timeout = window.setTimeout(() => {
      void (async () => {
        const results = await historyState.suggest(query, 6);

        if (!cancelled) {
          suggestions = results;
          isSuggestionsOpen = results.length > 0;
          selectedSuggestionIndex = -1;
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
    searchEngineMode = null;
  }

  function handleAddressBlur(): void {
    isFocused = false;

    if (!addressInput.trim()) {
      searchEngineMode = null;
    }

    window.setTimeout(() => {
      isSuggestionsOpen = false;
      selectedSuggestionIndex = -1;
    }, 120);
  }

  function handleAddressInput(event: Event): void {
    addressInput = (event.currentTarget as HTMLInputElement).value;
    validationError = '';
  }

  function handleAddressKeydown(event: KeyboardEvent): void {
    if (activateSearchEngineMode(event)) {
      return;
    }

    if (handleSuggestionKeydown(event)) {
      return;
    }

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

      if (isApplePlatform && event.metaKey) {
        void submitNavigationInNewTab(addressInput, true);
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
      selectedSuggestionIndex = -1;
      searchEngineMode = null;
      addressElement?.blur();
    }
  }

  function activateSearchEngineMode(event: KeyboardEvent): boolean {
    if (event.key !== 'Tab' || event.shiftKey || event.ctrlKey || event.metaKey || event.altKey) {
      return false;
    }

    const engine = searchEngineFromKeyword(addressInput);

    if (!engine) {
      return false;
    }

    event.preventDefault();
    searchEngineMode = engine;
    addressInput = '';
    suggestions = [];
    isSuggestionsOpen = false;
    selectedSuggestionIndex = -1;
    validationError = '';
    return true;
  }

  function handleSuggestionKeydown(event: KeyboardEvent): boolean {
    if (!isSuggestionsOpen || suggestions.length === 0) {
      return false;
    }

    if (event.key === 'ArrowDown') {
      event.preventDefault();
      selectedSuggestionIndex =
        (selectedSuggestionIndex + 1 + suggestions.length) % suggestions.length;
      return true;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      selectedSuggestionIndex =
        selectedSuggestionIndex <= 0 ? suggestions.length - 1 : selectedSuggestionIndex - 1;
      return true;
    }

    if (event.key === 'Enter' && selectedSuggestionIndex >= 0) {
      const suggestion = suggestions[selectedSuggestionIndex];

      if (!suggestion) {
        return false;
      }

      event.preventDefault();
      void submitNavigation(suggestion.url);
      return true;
    }

    if (event.shiftKey && (event.key === 'Delete' || event.key === 'Backspace')) {
      const suggestion = suggestions[selectedSuggestionIndex];

      if (!suggestion) {
        return false;
      }

      event.preventDefault();
      void removeSuggestion(suggestion);
      return true;
    }

    return false;
  }

  async function removeSuggestion(suggestion: HistoryEntry): Promise<void> {
    await historyState.removeEntry(suggestion.id);
    suggestions = suggestions.filter((entry) => entry.id !== suggestion.id);

    if (suggestions.length === 0) {
      isSuggestionsOpen = false;
      selectedSuggestionIndex = -1;
      return;
    }

    selectedSuggestionIndex = Math.min(selectedSuggestionIndex, suggestions.length - 1);
  }

  async function submitNavigationInNewTab(rawInput: string, background = false): Promise<void> {
    const rawUrl = rawInput.trim();

    if (!rawUrl) {
      validationError = '';
      return;
    }

    isSubmitting = true;
    validationError = '';

    try {
      const normalizedUrl = await resolveAddressInput(rawUrl);

      if (background) {
        tabsState.openBackgroundTab(normalizedUrl, normalizedUrl);
      } else {
        tabsState.openTab(normalizedUrl, normalizedUrl);
      }

      addressInput = background ? activeDisplayUrl : normalizedUrl;
      isSuggestionsOpen = false;
      selectedSuggestionIndex = -1;
      searchEngineMode = null;
      if (!background) {
        addressElement?.blur();
      }
    } catch (error) {
      validationError = errorMessage(error);
    } finally {
      isSubmitting = false;
    }
  }

  async function submitNavigation(rawInput: string): Promise<void> {
    const rawUrl = rawInput.trim();

    if (!rawUrl) {
      validationError = '';
      return;
    }

    isSubmitting = true;
    validationError = '';

    try {
      const normalizedUrl = await resolveAddressInput(rawUrl);
      await onNavigate(normalizedUrl);
      addressInput = normalizedUrl;
      isSuggestionsOpen = false;
      selectedSuggestionIndex = -1;
      searchEngineMode = null;
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

  async function resolveAddressInput(rawInput: string): Promise<string> {
    if (searchEngineMode) {
      return searchUrlForEngine(rawInput, searchEngineMode);
    }

    return normalizeUrl(rawInput);
  }

  function searchEngineFromKeyword(rawInput: string): SearchEngine | null {
    const keyword = rawInput.trim().toLocaleLowerCase();

    if (keyword === 'google' || keyword === 'google.com') {
      return 'google';
    }

    if (keyword === 'yandex' || keyword === 'yandex.com') {
      return 'yandex';
    }

    return null;
  }

  function searchEngineLabel(engine: SearchEngine): string {
    return engine === 'google' ? 'Google' : 'Yandex';
  }

  function searchUrlForEngine(query: string, engine: SearchEngine): string {
    const encodedQuery = encodeURIComponent(query.trim());

    if (engine === 'google') {
      return `https://www.google.com/search?q=${encodedQuery}`;
    }

    return `https://yandex.com/search/?text=${encodedQuery}`;
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

<nav class="navigation-bar" aria-label="Navigation controls">
  <div class="history-controls" aria-label="Page history">
    <button
      bind:this={backButtonElement}
      class="toolbar-button"
      type="button"
      aria-label="Back"
      title="Back"
      disabled={!canGoBack}
      onclick={() => {
        if (canGoBack) {
          void runCommand(onBack);
        }
      }}
    >
      <ArrowLeft size={18} strokeWidth={2.25} />
    </button>
    {#if canGoForward}
      <button
        class="toolbar-button toolbar-button--forward"
        type="button"
        aria-label="Forward"
        title="Forward"
        onclick={() => {
          if (canGoForward) {
            void runCommand(onForward);
          }
        }}
      >
        <ArrowRight size={18} strokeWidth={2.25} />
      </button>
    {/if}
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
        placeholder={addressPlaceholder}
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
        {#each suggestions as suggestion, index (suggestion.id)}
          <button
            class={`suggestion ${index === selectedSuggestionIndex ? 'suggestion--selected' : ''}`}
            type="button"
            role="option"
            aria-selected={index === selectedSuggestionIndex}
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
  </div>
</nav>

<style>
  .navigation-bar {
    display: grid;
    grid-template-columns: auto minmax(180px, 1fr) auto;
    gap: 8px;
    align-items: center;
    width: 100%;
    height: var(--shell-nav-height);
    min-height: var(--shell-nav-height);
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
    transition:
      color 120ms ease,
      background 120ms ease,
      opacity 120ms ease,
      transform 120ms ease;
  }

  .toolbar-button--forward {
    transform-origin: left center;
    animation: forward-button-appear 150ms cubic-bezier(0.22, 0.61, 0.36, 1);
  }

  .toolbar-button:hover:not(:disabled) {
    color: var(--color-text-primary);
    background: rgb(255 255 255 / 0.085);
  }

  .toolbar-button:disabled {
    color: color-mix(in srgb, var(--color-text-secondary) 42%, transparent);
    opacity: 0.58;
    pointer-events: none;
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
  .suggestions {
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

  .suggestion:hover,
  .suggestion--selected {
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

  @keyframes forward-button-appear {
    from {
      opacity: 0;
      transform: translateX(-6px) scale(0.92);
    }

    to {
      opacity: 1;
      transform: translateX(0) scale(1);
    }
  }
</style>
