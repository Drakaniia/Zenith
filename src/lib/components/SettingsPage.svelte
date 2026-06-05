<script lang="ts">
  import {
    BadgeInfo,
    Brush,
    Check,
    Database,
    Home,
    Loader2,
    PanelBottom,
    Search,
    ShieldCheck,
    Trash2
  } from 'lucide-svelte';
  import {
    settingsState,
    type SearchEngine,
    type ThemePreference
  } from '../state/settings.svelte';
  import { historyState } from '../state/history.svelte';
  import { toastsState } from '../state/toasts.svelte';
  import { clearAllBrowsingData, getAppVersion, isTauriRuntime } from '../utils/tauri';

  const AUTO_SAVE_DELAY_MS = 450;
  const SEARCH_ENGINES: Array<{ value: SearchEngine; label: string }> = [
    { value: 'yandex', label: 'Yandex' },
    { value: 'google', label: 'Google' }
  ];
  const THEMES: Array<{ value: ThemePreference; label: string }> = [
    { value: 'dark', label: 'Dark' },
    { value: 'light', label: 'Light' }
  ];

  const nativeRuntime = isTauriRuntime();

  let homepageDraft = $state(settingsState.settings.homepage);
  let appVersion = $state<string | null>(null);
  let isLoadingVersion = $state(false);
  let isReadyForAutoSave = $state(false);
  let isClearingHistory = $state(false);
  let isClearingBrowsingData = $state(false);
  let didInitialize = false;
  let persistedSignature = '';

  let searchEngine = $derived(searchEngineFromValue(settingsState.settings.default_search_engine));
  let theme = $derived(themeFromValue(settingsState.settings.theme));
  let homepageIssue = $derived(homepageValidationMessage(homepageDraft));
  let saveStatus = $derived(settingsSaveStatus());
  let settingsSignatureValue = $derived(settingsSignature(settingsState.settings));

  $effect(() => {
    if (didInitialize) {
      return;
    }

    didInitialize = true;

    void (async () => {
      if (nativeRuntime) {
        await Promise.all([settingsState.load(), loadAppVersion()]);
      } else {
        await loadAppVersion();
      }

      homepageDraft = settingsState.settings.homepage;
      persistedSignature = settingsSignature(settingsState.settings);
      isReadyForAutoSave = nativeRuntime;
    })();
  });

  $effect(() => {
    const signature = settingsSignatureValue;

    if (!isReadyForAutoSave || signature === persistedSignature) {
      return;
    }

    const timeout = window.setTimeout(() => {
      persistedSignature = signature;
      void settingsState.save();
    }, AUTO_SAVE_DELAY_MS);

    return () => {
      window.clearTimeout(timeout);
    };
  });

  async function loadAppVersion(): Promise<void> {
    isLoadingVersion = true;

    try {
      appVersion = await getAppVersion();
    } catch (error) {
      appVersion = null;
      toastsState.error(`Failed to load app version: ${errorMessage(error)}`);
    } finally {
      isLoadingVersion = false;
    }
  }

  function handleSearchEngineChange(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (value === 'yandex' || value === 'google') {
      settingsState.update({ default_search_engine: value });
    }
  }

  function handleHomepageInput(event: Event): void {
    homepageDraft = (event.currentTarget as HTMLInputElement).value;
    settingsState.update({ homepage: homepageDraft.trim() });
  }

  function handleHomepageBlur(): void {
    homepageDraft = homepageDraft.trim();
    settingsState.update({ homepage: homepageDraft });
  }

  function setTheme(value: ThemePreference): void {
    settingsState.update({ theme: value });
  }

  function handleJavascriptToggle(event: Event): void {
    settingsState.update({ javascript_enabled: (event.currentTarget as HTMLInputElement).checked });
  }

  function handleStatusBarToggle(event: Event): void {
    settingsState.update({ show_status_bar: (event.currentTarget as HTMLInputElement).checked });
  }

  async function clearHistoryFromSettings(): Promise<void> {
    isClearingHistory = true;

    try {
      const cleared = await historyState.clear();
      if (cleared) {
        toastsState.success('History cleared');
      }
    } finally {
      isClearingHistory = false;
    }
  }

  async function clearBrowsingDataFromSettings(): Promise<void> {
    isClearingBrowsingData = true;

    try {
      await clearAllBrowsingData();
      toastsState.success('Browsing data cleared');
    } catch (error) {
      toastsState.error(`Failed to clear browsing data: ${errorMessage(error)}`);
    } finally {
      isClearingBrowsingData = false;
    }
  }

  function settingsSaveStatus(): string {
    if (!nativeRuntime) {
      return 'Preview';
    }

    if (settingsState.isLoading) {
      return 'Loading';
    }

    if (settingsState.isSaving) {
      return 'Saving';
    }

    if (settingsState.lastSavedAt) {
      return 'Saved';
    }

    return 'Ready';
  }

  function settingsSignature(settings: typeof settingsState.settings): string {
    return JSON.stringify({
      default_search_engine: settings.default_search_engine,
      homepage: settings.homepage,
      theme: settings.theme,
      javascript_enabled: settings.javascript_enabled,
      show_status_bar: settings.show_status_bar
    });
  }

  function homepageValidationMessage(value: string): string {
    const trimmed = value.trim();
    if (!trimmed) {
      return 'Homepage cannot be empty';
    }

    try {
      const url = new URL(trimmed);
      if (url.protocol === 'http:' || url.protocol === 'https:') {
        return '';
      }
    } catch {
      return 'Homepage must be a valid URL';
    }

    return 'Homepage must use http or https';
  }

  function searchEngineFromValue(value: string): SearchEngine {
    return value === 'google' ? 'google' : 'yandex';
  }

  function themeFromValue(value: string): ThemePreference {
    return value === 'light' ? 'light' : 'dark';
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

<section class="settings-page" aria-label="Settings">
  <aside class="settings-rail" aria-label="Settings sections">
    <label class="settings-search">
      <Search size={15} strokeWidth={2.25} />
      <input type="search" aria-label="Search settings" placeholder="Search" />
    </label>

    <nav class="settings-nav" aria-label="Settings categories">
      <a href="#general" class="settings-nav__item settings-nav__item--active">General</a>
      <a href="#appearance" class="settings-nav__item">Appearance</a>
      <a href="#privacy" class="settings-nav__item">Privacy</a>
      <a href="#about" class="settings-nav__item">About</a>
    </nav>
  </aside>

  <div class="settings-scroll">
    <header class="settings-header">
      <div>
        <p class="settings-kicker">Browser</p>
        <h1>Settings</h1>
      </div>

      <span class={`save-pill ${settingsState.isSaving ? 'save-pill--busy' : ''}`}>
        {#if settingsState.isSaving || settingsState.isLoading}
          <Loader2 size={13} strokeWidth={2.4} />
        {:else}
          <Check size={13} strokeWidth={2.5} />
        {/if}
        <span>{saveStatus}</span>
      </span>
    </header>

    <section class="settings-section" id="general" aria-labelledby="general-title">
      <div class="section-heading">
        <Search size={18} strokeWidth={2.25} />
        <div>
          <h2 id="general-title">General</h2>
          <p>Search provider and startup page</p>
        </div>
      </div>

      <div class="settings-rows">
        <label class="setting-row" for="search-engine">
          <span class="setting-copy">
            <span class="setting-title">Default search engine</span>
            <span class="setting-detail">Used for address bar searches</span>
          </span>
          <select id="search-engine" value={searchEngine} onchange={handleSearchEngineChange}>
            {#each SEARCH_ENGINES as option}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </label>

        <label class="setting-row setting-row--stack" for="homepage-url">
          <span class="setting-copy">
            <span class="setting-title">Homepage</span>
            <span class="setting-detail">Opened for new tabs from the browser menu</span>
          </span>
          <span class="homepage-control">
            <Home size={16} strokeWidth={2.25} />
            <input
              id="homepage-url"
              type="url"
              inputmode="url"
              value={homepageDraft}
              autocomplete="off"
              spellcheck="false"
              aria-invalid={homepageIssue ? 'true' : 'false'}
              aria-describedby={homepageIssue ? 'homepage-issue' : undefined}
              oninput={handleHomepageInput}
              onblur={handleHomepageBlur}
            />
          </span>
          {#if homepageIssue}
            <span class="field-issue" id="homepage-issue">{homepageIssue}</span>
          {/if}
        </label>
      </div>
    </section>

    <section class="settings-section" id="appearance" aria-labelledby="appearance-title">
      <div class="section-heading">
        <Brush size={18} strokeWidth={2.25} />
        <div>
          <h2 id="appearance-title">Appearance</h2>
          <p>Chrome theme and visible browser controls</p>
        </div>
      </div>

      <div class="settings-rows">
        <div class="setting-row">
          <span class="setting-copy">
            <span class="setting-title">Theme</span>
            <span class="setting-detail">Applies to trusted browser chrome</span>
          </span>
          <div class="segmented-control" role="group" aria-label="Theme">
            {#each THEMES as option}
              <button
                class:segmented-control__item--active={theme === option.value}
                class="segmented-control__item"
                type="button"
                aria-pressed={theme === option.value}
                onclick={() => setTheme(option.value)}
              >
                {option.label}
              </button>
            {/each}
          </div>
        </div>

        <label class="setting-row">
          <span class="setting-copy">
            <span class="setting-title">Show status bar</span>
            <span class="setting-detail">Displays page security and loading state</span>
          </span>
          <span class="switch-control">
            <input
              type="checkbox"
              checked={settingsState.settings.show_status_bar}
              onchange={handleStatusBarToggle}
              aria-label="Show status bar"
            />
            <span class="switch-track" aria-hidden="true">
              <span class="switch-thumb"></span>
            </span>
          </span>
        </label>
      </div>
    </section>

    <section class="settings-section" id="privacy" aria-labelledby="privacy-title">
      <div class="section-heading">
        <ShieldCheck size={18} strokeWidth={2.25} />
        <div>
          <h2 id="privacy-title">Privacy and Content</h2>
          <p>Page runtime and local browsing records</p>
        </div>
      </div>

      <div class="settings-rows">
        <label class="setting-row">
          <span class="setting-copy">
            <span class="setting-title">JavaScript</span>
            <span class="setting-detail">Used when creating browser webviews</span>
          </span>
          <span class="switch-control">
            <input
              type="checkbox"
              checked={settingsState.settings.javascript_enabled}
              onchange={handleJavascriptToggle}
              aria-label="JavaScript enabled"
            />
            <span class="switch-track" aria-hidden="true">
              <span class="switch-thumb"></span>
            </span>
          </span>
        </label>

        <div class="action-row">
          <div class="action-copy">
            <Trash2 size={17} strokeWidth={2.3} />
            <span>
              <strong>History</strong>
              <span>Remove saved visits from this device</span>
            </span>
          </div>
          <button
            class="danger-button"
            type="button"
            disabled={!nativeRuntime || isClearingHistory}
            title={nativeRuntime ? 'Clear history' : 'Available in the Tauri app'}
            onclick={clearHistoryFromSettings}
          >
            {#if isClearingHistory}
              <Loader2 size={15} strokeWidth={2.4} />
            {:else}
              <Trash2 size={15} strokeWidth={2.3} />
            {/if}
            <span>Clear History</span>
          </button>
        </div>

        <div class="action-row">
          <div class="action-copy">
            <Database size={17} strokeWidth={2.25} />
            <span>
              <strong>Browsing data</strong>
              <span>Clear WebView cookies, cache, and storage where supported</span>
            </span>
          </div>
          <button
            class="danger-button"
            type="button"
            disabled={!nativeRuntime || isClearingBrowsingData}
            title={nativeRuntime ? 'Clear browsing data' : 'Available in the Tauri app'}
            onclick={clearBrowsingDataFromSettings}
          >
            {#if isClearingBrowsingData}
              <Loader2 size={15} strokeWidth={2.4} />
            {:else}
              <Database size={15} strokeWidth={2.25} />
            {/if}
            <span>Clear Data</span>
          </button>
        </div>
      </div>
    </section>

    <section class="settings-section settings-section--last" id="about" aria-labelledby="about-title">
      <div class="section-heading">
        <BadgeInfo size={18} strokeWidth={2.25} />
        <div>
          <h2 id="about-title">About</h2>
          <p>Application and runtime details</p>
        </div>
      </div>

      <div class="about-grid">
        <div class="about-item">
          <PanelBottom size={18} strokeWidth={2.2} />
          <span>
            <strong>Rust Browser</strong>
            <span>Desktop browser shell</span>
          </span>
        </div>
        <div class="about-item">
          <BadgeInfo size={18} strokeWidth={2.2} />
          <span>
            <strong>Version</strong>
            <span>
              {#if isLoadingVersion}
                Loading
              {:else}
                {appVersion ?? 'Unavailable'}
              {/if}
            </span>
          </span>
        </div>
      </div>
    </section>
  </div>
</section>

<style>
  .settings-page {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: grid;
    grid-template-columns: clamp(196px, 18vw, 248px) minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    color: var(--color-text-primary);
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.035), transparent 260px),
      var(--color-background);
  }

  .settings-rail {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 14px;
    min-width: 0;
    padding: 16px 12px;
    background: color-mix(in srgb, var(--color-surface) 86%, var(--color-background));
    border-right: 1px solid var(--color-border);
  }

  .settings-search {
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr);
    align-items: center;
    height: 36px;
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    background: color-mix(in srgb, var(--color-background) 74%, var(--color-surface));
    border: 1px solid color-mix(in srgb, var(--color-border) 84%, transparent);
    border-radius: 8px;
  }

  .settings-search :global(svg) {
    justify-self: center;
  }

  .settings-search input {
    min-width: 0;
    height: 100%;
    padding: 0 10px 0 0;
    color: var(--color-text-primary);
    background: transparent;
    border: 0;
    outline: 0;
  }

  .settings-search input::-webkit-search-cancel-button {
    display: none;
  }

  .settings-nav {
    display: grid;
    align-content: start;
    gap: 4px;
    min-width: 0;
  }

  .settings-nav__item {
    display: flex;
    align-items: center;
    height: 34px;
    min-width: 0;
    padding: 0 12px;
    overflow: hidden;
    color: var(--color-text-primary);
    font-size: 13px;
    font-weight: 670;
    letter-spacing: 0;
    text-decoration: none;
    text-overflow: ellipsis;
    white-space: nowrap;
    border-radius: 8px;
  }

  .settings-nav__item:hover,
  .settings-nav__item--active {
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
  }

  .settings-scroll {
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    scroll-behavior: smooth;
  }

  .settings-header,
  .settings-section {
    width: min(980px, calc(100% - 40px));
    margin-inline: auto;
  }

  .settings-header {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 18px;
    padding: 28px 0 16px;
    border-bottom: 1px solid var(--color-border);
  }

  .settings-kicker {
    margin: 0 0 7px;
    color: var(--color-accent);
    font-size: 11px;
    font-weight: 760;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  .settings-header h1 {
    margin: 0;
    font-size: 28px;
    font-weight: 760;
    letter-spacing: 0;
    line-height: 1;
  }

  .save-pill {
    display: grid;
    grid-template-columns: 16px minmax(0, auto);
    gap: 6px;
    align-items: center;
    height: 28px;
    min-width: 76px;
    padding: 0 9px;
    color: var(--color-text-secondary);
    background: color-mix(in srgb, var(--color-surface) 82%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-border) 88%, transparent);
    border-radius: 999px;
  }

  .save-pill span {
    font-size: 11px;
    font-weight: 690;
    letter-spacing: 0;
    line-height: 1;
  }

  .save-pill--busy :global(svg),
  .danger-button :global(svg:first-child),
  .save-pill :global(svg) {
    justify-self: center;
  }

  .save-pill--busy :global(svg),
  .danger-button:disabled :global(svg) {
    animation: spin 820ms linear infinite;
  }

  .settings-section {
    display: grid;
    grid-template-columns: minmax(210px, 0.34fr) minmax(0, 0.66fr);
    gap: 32px;
    padding: 26px 0 28px;
    border-bottom: 1px solid color-mix(in srgb, var(--color-border) 78%, transparent);
  }

  .settings-section--last {
    border-bottom: 0;
  }

  .section-heading {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr);
    gap: 10px;
    align-content: start;
    min-width: 0;
  }

  .section-heading :global(svg) {
    color: var(--color-accent);
  }

  .section-heading h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 740;
    letter-spacing: 0;
    line-height: 1.2;
  }

  .section-heading p {
    margin: 6px 0 0;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 520;
    letter-spacing: 0;
    line-height: 1.35;
  }

  .settings-rows {
    display: grid;
    gap: 10px;
    min-width: 0;
  }

  .setting-row,
  .action-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(148px, auto);
    gap: 18px;
    align-items: center;
    min-width: 0;
    min-height: 56px;
    padding: 11px 0;
  }

  .setting-row + .setting-row,
  .action-row + .action-row,
  .setting-row + .action-row {
    border-top: 1px solid color-mix(in srgb, var(--color-border) 58%, transparent);
  }

  .setting-row--stack {
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
  }

  .setting-copy,
  .action-copy span,
  .about-item span {
    display: grid;
    gap: 4px;
    min-width: 0;
  }

  .setting-title,
  .action-copy strong,
  .about-item strong {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-primary);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .setting-detail,
  .action-copy span span,
  .about-item span span {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 520;
    letter-spacing: 0;
    line-height: 1.3;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  select,
  .homepage-control {
    height: 36px;
    min-width: 0;
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-background) 76%, var(--color-surface));
    border: 1px solid color-mix(in srgb, var(--color-border) 86%, transparent);
    border-radius: 8px;
  }

  select {
    width: 176px;
    padding: 0 32px 0 10px;
  }

  .homepage-control {
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr);
    align-items: center;
    width: min(560px, 100%);
    overflow: hidden;
    color: var(--color-text-secondary);
  }

  .homepage-control :global(svg) {
    justify-self: center;
  }

  .homepage-control input {
    min-width: 0;
    height: 100%;
    padding: 0 11px 0 0;
    color: var(--color-text-primary);
    background: transparent;
    border: 0;
    outline: 0;
  }

  .field-issue {
    color: var(--color-warning);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0;
    line-height: 1.25;
  }

  .segmented-control {
    display: grid;
    grid-template-columns: repeat(2, minmax(74px, 1fr));
    width: 176px;
    height: 36px;
    padding: 3px;
    background: color-mix(in srgb, var(--color-background) 76%, var(--color-surface));
    border: 1px solid color-mix(in srgb, var(--color-border) 86%, transparent);
    border-radius: 8px;
  }

  .segmented-control__item {
    min-width: 0;
    height: 28px;
    padding: 0 10px;
    overflow: hidden;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
    background: transparent;
    border: 0;
    border-radius: 6px;
    cursor: default;
  }

  .segmented-control__item--active {
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-accent) 24%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-accent) 28%, transparent);
  }

  .switch-control {
    position: relative;
    display: inline-grid;
    width: 46px;
    height: 26px;
    place-items: center;
  }

  .switch-control input {
    position: absolute;
    inset: 0;
    z-index: 1;
    margin: 0;
    opacity: 0;
    cursor: default;
  }

  .switch-track {
    position: relative;
    width: 46px;
    height: 26px;
    background: color-mix(in srgb, var(--color-border) 92%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-border) 88%, transparent);
    border-radius: 999px;
    transition:
      background 140ms ease,
      border-color 140ms ease;
  }

  .switch-thumb {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    background: var(--color-text-primary);
    border-radius: 999px;
    box-shadow: 0 2px 6px rgb(0 0 0 / 0.24);
    transition: transform 140ms ease;
  }

  .switch-control input:checked + .switch-track {
    background: color-mix(in srgb, var(--color-accent) 74%, #111827);
    border-color: color-mix(in srgb, var(--color-accent) 86%, var(--color-border));
  }

  .switch-control input:checked + .switch-track .switch-thumb {
    transform: translateX(20px);
  }

  .action-row {
    min-height: 66px;
  }

  .action-copy,
  .about-item {
    display: grid;
    grid-template-columns: 30px minmax(0, 1fr);
    gap: 10px;
    align-items: center;
    min-width: 0;
  }

  .action-copy :global(svg),
  .about-item :global(svg) {
    color: var(--color-text-secondary);
  }

  .danger-button {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr);
    gap: 7px;
    align-items: center;
    justify-self: end;
    width: 148px;
    height: 34px;
    min-width: 0;
    padding: 0 10px;
    color: var(--color-text-primary);
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--color-border) 88%, transparent);
    border-radius: 8px;
    cursor: default;
  }

  .danger-button span {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .danger-button:hover:not(:disabled) {
    color: var(--color-danger);
    background: color-mix(in srgb, var(--color-danger) 11%, transparent);
    border-color: color-mix(in srgb, var(--color-danger) 38%, var(--color-border));
  }

  .danger-button:disabled {
    color: color-mix(in srgb, var(--color-text-secondary) 46%, transparent);
  }

  .about-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    min-width: 0;
  }

  .about-item {
    min-height: 58px;
    padding: 12px;
    background: color-mix(in srgb, var(--color-surface) 78%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-border) 72%, transparent);
    border-radius: 8px;
  }

  :global(.app-shell[data-theme='light']) .settings-page {
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.82), transparent 260px),
      var(--color-background);
  }

  :global(.app-shell[data-theme='light']) .settings-rail {
    background: color-mix(in srgb, var(--color-surface) 94%, var(--color-background));
  }

  :global(.app-shell[data-theme='light']) .settings-nav__item:hover,
  :global(.app-shell[data-theme='light']) .settings-nav__item--active {
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  }

  :global(.app-shell[data-theme='light']) .about-item {
    background: color-mix(in srgb, var(--color-surface) 96%, transparent);
  }

  @media (max-width: 760px) {
    .settings-page {
      grid-template-columns: minmax(0, 1fr);
    }

    .settings-rail {
      display: none;
    }

    .settings-section {
      grid-template-columns: minmax(0, 1fr);
      gap: 16px;
    }

    .settings-header,
    .settings-section {
      width: min(100% - 28px, 980px);
    }
  }

  @media (max-width: 560px) {
    .settings-header {
      align-items: start;
      flex-direction: column;
    }

    .setting-row,
    .action-row {
      grid-template-columns: minmax(0, 1fr);
      gap: 10px;
    }

    select,
    .segmented-control,
    .danger-button {
      justify-self: stretch;
      width: 100%;
    }

    .switch-control {
      justify-self: start;
    }

    .about-grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
