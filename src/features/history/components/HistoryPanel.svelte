<script lang="ts">
  import {
    AlertTriangle,
    Clock3,
    Globe,
    History as HistoryIcon,
    Search,
    Trash2,
    X
  } from 'lucide-svelte';
  import type { HistoryEntry } from '../../../shared/types/backend';
  import { displayUrl, domainFromUrl } from '../../../shared/utils/url';
  import { toastsState } from '../../../shell/state/toasts.svelte';
  import { historyState } from '../state/history.svelte';

  interface Props {
    open: boolean;
    onClose: () => void;
    onNavigate: (url: string) => Promise<void>;
  }

  interface HistoryGroup {
    label: 'Today' | 'Yesterday' | 'This Week' | 'Earlier';
    entries: HistoryEntry[];
  }

  interface LocaleWeekInfo {
    firstDay?: number;
  }

  interface LocaleWithWeekInfo {
    weekInfo?: LocaleWeekInfo;
  }

  interface IntlLocaleAccess {
    Locale?: new (locale: string) => LocaleWithWeekInfo;
  }

  const TIME_FORMATTER = new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit'
  });
  const DATE_FORMATTER = new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric'
  });

  let { open, onClose, onNavigate }: Props = $props();

  let searchQuery = $state('');
  let deletingId = $state<string | null>(null);
  let failedFavicons = $state<Record<string, true>>({});
  let showClearConfirm = $state(false);
  let isClearing = $state(false);
  let wasOpen = false;

  let visibleEntries = $derived(
    searchQuery.trim() ? historyState.searchResults : historyState.entries
  );
  let historyGroups = $derived(groupHistoryEntries(visibleEntries));
  let countLabel = $derived(formatCount(visibleEntries.length, 'visit'));
  let emptyLabel = $derived(searchQuery.trim() ? 'No matching history' : 'No history yet');
  let canClear = $derived(historyState.entries.length > 0 && !isClearing);

  $effect(() => {
    if (open && !wasOpen) {
      void historyState.load();
    }

    if (!open && wasOpen) {
      searchQuery = '';
      showClearConfirm = false;
    }

    wasOpen = open;
  });

  $effect(() => {
    if (!open) {
      return;
    }

    const query = searchQuery.trim();
    let cancelled = false;
    const timeout = window.setTimeout(() => {
      void (async () => {
        await historyState.search(query);
        if (cancelled) {
          return;
        }
      })();
    }, 140);

    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  });

  function handleSearchInput(event: Event): void {
    searchQuery = (event.currentTarget as HTMLInputElement).value;
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (!open) {
      return;
    }

    if (event.key === 'Escape') {
      if (showClearConfirm) {
        showClearConfirm = false;
        return;
      }

      onClose();
    }
  }

  async function navigateTo(url: string): Promise<void> {
    try {
      await onNavigate(url);
      onClose();
    } catch (error) {
      toastsState.error(`Failed to open history entry: ${errorMessage(error)}`);
    }
  }

  async function deleteEntry(id: string): Promise<void> {
    deletingId = id;

    try {
      await historyState.removeEntry(id);
    } finally {
      deletingId = null;
    }
  }

  async function clearHistory(): Promise<void> {
    isClearing = true;

    try {
      const cleared = await historyState.clear();
      if (!cleared) {
        return;
      }

      await historyState.load();
      searchQuery = '';
      showClearConfirm = false;
      toastsState.success('History cleared');
    } finally {
      isClearing = false;
    }
  }

  function markFaviconFailed(id: string): void {
    failedFavicons = {
      ...failedFavicons,
      [id]: true
    };
  }

  function groupHistoryEntries(entries: HistoryEntry[]): HistoryGroup[] {
    const groups: HistoryGroup[] = [
      { label: 'Today', entries: [] },
      { label: 'Yesterday', entries: [] },
      { label: 'This Week', entries: [] },
      { label: 'Earlier', entries: [] }
    ];
    const todayStart = startOfLocalDay(new Date());
    const yesterdayStart = addLocalDays(todayStart, -1);
    const weekStart = startOfLocalWeek(todayStart);

    for (const entry of entries) {
      const visitedAt = parseEntryDate(entry.visited_at);

      if (!visitedAt || visitedAt < weekStart) {
        groups[3].entries.push(entry);
      } else if (visitedAt >= todayStart) {
        groups[0].entries.push(entry);
      } else if (visitedAt >= yesterdayStart) {
        groups[1].entries.push(entry);
      } else {
        groups[2].entries.push(entry);
      }
    }

    return groups.filter((group) => group.entries.length > 0);
  }

  function startOfLocalWeek(date: Date): Date {
    const start = startOfLocalDay(date);
    const firstDay = localeFirstDayOfWeek();
    const dayOffset = (start.getDay() - firstDay + 7) % 7;
    start.setDate(start.getDate() - dayOffset);
    return start;
  }

  function localeFirstDayOfWeek(): number {
    const LocaleConstructor = (Intl as unknown as IntlLocaleAccess).Locale;
    if (!LocaleConstructor) {
      return 0;
    }

    try {
      const localeTag = navigator.languages?.[0] ?? navigator.language;
      const firstDay = new LocaleConstructor(localeTag).weekInfo?.firstDay;
      if (typeof firstDay === 'number' && firstDay >= 1 && firstDay <= 7) {
        return firstDay % 7;
      }
    } catch {
      return 0;
    }

    return 0;
  }

  function startOfLocalDay(date: Date): Date {
    const start = new Date(date);
    start.setHours(0, 0, 0, 0);
    return start;
  }

  function addLocalDays(date: Date, days: number): Date {
    const next = new Date(date);
    next.setDate(next.getDate() + days);
    return next;
  }

  function parseEntryDate(value: string): Date | null {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? null : date;
  }

  function faviconUrl(url: string): string | null {
    try {
      const parsed = new URL(url);
      if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
        return null;
      }

      return `${parsed.origin}/favicon.ico`;
    } catch {
      return null;
    }
  }

  function formatVisitedAt(value: string): string {
    const date = parseEntryDate(value);
    if (!date) {
      return 'Unknown time';
    }

    return `${DATE_FORMATTER.format(date)} ${TIME_FORMATTER.format(date)}`;
  }

  function formatCount(count: number, singular: string): string {
    return `${count} ${count === 1 ? singular : `${singular}s`}`;
  }

  function groupHeadingId(label: HistoryGroup['label']): string {
    return `history-${label.toLocaleLowerCase().replace(/\s+/g, '-')}`;
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

<svelte:window onkeydown={handleWindowKeydown} />

{#if open}
  <aside class="library-panel" aria-label="History">
    <header class="panel-header">
      <div class="panel-heading">
        <HistoryIcon size={18} strokeWidth={2.25} />
        <div class="panel-heading__text">
          <h2>History</h2>
          <span>{countLabel}</span>
        </div>
      </div>

      <button
        class="icon-button"
        type="button"
        aria-label="Close history"
        title="Close"
        onclick={onClose}
      >
        <X size={18} strokeWidth={2.35} />
      </button>
    </header>

    <div class="panel-tools">
      <label class="search-box">
        <Search size={15} strokeWidth={2.25} />
        <input
          type="search"
          value={searchQuery}
          autocomplete="off"
          spellcheck="false"
          aria-label="Search history"
          placeholder="Search"
          oninput={handleSearchInput}
        />
      </label>

      <button
        class="clear-button"
        type="button"
        disabled={!canClear}
        onclick={() => {
          showClearConfirm = true;
        }}
      >
        <Trash2 size={16} strokeWidth={2.3} />
        <span>Clear History</span>
      </button>
    </div>

    <div class="panel-body">
      {#if historyState.isLoading && visibleEntries.length === 0}
        <div class="empty-state" role="status">Loading history</div>
      {:else if visibleEntries.length === 0}
        <div class="empty-state" role="status">{emptyLabel}</div>
      {:else}
        <div class="history-list">
          {#each historyGroups as group (group.label)}
            <section class="history-group" aria-labelledby={groupHeadingId(group.label)}>
              <h3 id={groupHeadingId(group.label)}>{group.label}</h3>

              <div class="history-group__items" role="list">
                {#each group.entries as entry (entry.id)}
                  {@const iconUrl = faviconUrl(entry.url)}
                  <article class="history-row" role="listitem">
                    <button
                      class="history-main"
                      type="button"
                      title={entry.url}
                      onclick={() => navigateTo(entry.url)}
                    >
                      <span class="favicon" aria-hidden="true">
                        {#if iconUrl && !failedFavicons[entry.id]}
                          <img
                            src={iconUrl}
                            alt=""
                            draggable="false"
                            onerror={() => markFaviconFailed(entry.id)}
                          />
                        {:else}
                          <Globe size={16} strokeWidth={2.15} />
                        {/if}
                      </span>

                      <span class="history-text">
                        <span class="history-title">{entry.title || domainFromUrl(entry.url)}</span>
                        <span class="history-url">{displayUrl(entry.url)}</span>
                      </span>

                      <span class="history-time">
                        <Clock3 size={12} strokeWidth={2.25} />
                        <span>{formatVisitedAt(entry.visited_at)}</span>
                      </span>
                    </button>

                    <button
                      class="icon-button row-action"
                      type="button"
                      aria-label={`Delete ${entry.title || entry.url}`}
                      title="Delete"
                      disabled={deletingId === entry.id}
                      onclick={() => deleteEntry(entry.id)}
                    >
                      <Trash2 size={16} strokeWidth={2.25} />
                    </button>
                  </article>
                {/each}
              </div>
            </section>
          {/each}
        </div>
      {/if}
    </div>

    {#if showClearConfirm}
      <div class="confirm-layer" role="presentation">
        <div
          class="confirm-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby="clear-history-title"
        >
          <div class="confirm-icon" aria-hidden="true">
            <AlertTriangle size={20} strokeWidth={2.35} />
          </div>
          <div class="confirm-body">
            <h3 id="clear-history-title">Clear History?</h3>
            <p>Remove all saved history entries from this device.</p>
          </div>
          <div class="confirm-actions">
            <button
              class="confirm-cancel"
              type="button"
              disabled={isClearing}
              onclick={() => {
                showClearConfirm = false;
              }}
            >
              Cancel
            </button>
            <button
              class="confirm-clear"
              type="button"
              disabled={isClearing}
              onclick={clearHistory}
            >
              Clear
            </button>
          </div>
        </div>
      </div>
    {/if}
  </aside>
{/if}

<style>
  .library-panel {
    position: relative;
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    min-width: 0;
    height: 100%;
    color: var(--color-text-primary);
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.035), transparent 180px),
      color-mix(in srgb, var(--color-surface) 94%, var(--color-background));
    border-left: 1px solid var(--color-border);
    box-shadow: -16px 0 42px rgb(0 0 0 / 0.18);
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-width: 0;
    min-height: 60px;
    padding: 12px 12px 10px;
    border-bottom: 1px solid var(--color-border);
  }

  .panel-heading {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    min-width: 0;
  }

  .panel-heading :global(svg) {
    color: var(--color-accent);
  }

  .panel-heading__text {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  .panel-heading h2 {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 14px;
    font-weight: 720;
    letter-spacing: 0;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .panel-heading span {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    font-size: 11px;
    font-weight: 620;
    letter-spacing: 0;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .panel-tools {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 8px;
    align-items: center;
    padding: 10px 12px;
    border-bottom: 1px solid color-mix(in srgb, var(--color-border) 72%, transparent);
  }

  .search-box {
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr);
    align-items: center;
    height: 34px;
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    background: color-mix(in srgb, var(--color-background) 74%, var(--color-surface));
    border: 1px solid color-mix(in srgb, var(--color-border) 86%, transparent);
    border-radius: 8px;
  }

  .search-box:focus-within {
    color: var(--color-text-primary);
    border-color: color-mix(in srgb, var(--color-accent) 74%, var(--color-border));
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 18%, transparent);
  }

  .search-box :global(svg) {
    justify-self: center;
  }

  .search-box input {
    min-width: 0;
    height: 100%;
    padding: 0 9px 0 0;
    color: var(--color-text-primary);
    background: transparent;
    border: 0;
    outline: 0;
  }

  .search-box input::-webkit-search-cancel-button {
    display: none;
  }

  .search-box input::placeholder {
    color: color-mix(in srgb, var(--color-text-secondary) 72%, transparent);
  }

  .clear-button {
    display: grid;
    grid-template-columns: 20px minmax(0, auto);
    gap: 7px;
    align-items: center;
    height: 34px;
    max-width: 142px;
    padding: 0 10px;
    color: var(--color-text-primary);
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--color-border) 86%, transparent);
    border-radius: 8px;
    cursor: default;
  }

  .clear-button span {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 670;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .clear-button:hover:not(:disabled) {
    color: var(--color-danger);
    background: color-mix(in srgb, var(--color-danger) 11%, transparent);
    border-color: color-mix(in srgb, var(--color-danger) 38%, var(--color-border));
  }

  .panel-body {
    min-height: 0;
    overflow: hidden;
  }

  .history-list {
    height: 100%;
    padding: 8px;
    overflow-y: auto;
  }

  .history-group {
    display: grid;
    gap: 5px;
    margin-bottom: 14px;
  }

  .history-group h3 {
    margin: 5px 7px 3px;
    color: var(--color-text-secondary);
    font-size: 11px;
    font-weight: 720;
    letter-spacing: 0;
    line-height: 1;
    text-transform: uppercase;
  }

  .history-group__items {
    display: grid;
    gap: 2px;
  }

  .history-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 30px;
    gap: 4px;
    align-items: center;
    min-width: 0;
    min-height: 50px;
    border-radius: 8px;
  }

  .history-row:hover {
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  }

  .history-main {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr);
    grid-template-areas:
      'icon text'
      'icon time';
    column-gap: 8px;
    row-gap: 4px;
    align-items: center;
    min-width: 0;
    min-height: 50px;
    padding: 5px 6px;
    color: inherit;
    text-align: left;
    background: transparent;
    border: 0;
    border-radius: 8px;
    cursor: default;
  }

  .favicon {
    display: grid;
    grid-area: icon;
    width: 28px;
    height: 28px;
    place-items: center;
    overflow: hidden;
    color: var(--color-text-secondary);
    background: color-mix(in srgb, var(--color-background) 58%, var(--color-surface));
    border: 1px solid color-mix(in srgb, var(--color-border) 76%, transparent);
    border-radius: 7px;
  }

  .favicon img {
    width: 16px;
    height: 16px;
    object-fit: contain;
  }

  .history-text {
    display: grid;
    grid-area: text;
    gap: 3px;
    min-width: 0;
  }

  .history-title,
  .history-url,
  .history-time span {
    min-width: 0;
    overflow: hidden;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .history-title {
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 670;
    line-height: 1.1;
  }

  .history-url {
    color: var(--color-text-secondary);
    font-size: 11px;
    font-weight: 520;
    line-height: 1.1;
  }

  .history-time {
    display: flex;
    grid-area: time;
    align-items: center;
    min-width: 0;
    gap: 4px;
    color: color-mix(in srgb, var(--color-text-secondary) 82%, transparent);
    font-size: 10px;
    font-weight: 620;
    line-height: 1;
  }

  .history-time :global(svg) {
    flex: 0 0 auto;
  }

  .icon-button {
    display: grid;
    width: 30px;
    height: 30px;
    min-width: 30px;
    place-items: center;
    padding: 0;
    color: var(--color-text-secondary);
    background: transparent;
    border: 0;
    border-radius: 7px;
    cursor: default;
  }

  .icon-button:hover:not(:disabled) {
    color: var(--color-text-primary);
    background: rgb(255 255 255 / 0.085);
  }

  .icon-button:disabled,
  .clear-button:disabled {
    color: color-mix(in srgb, var(--color-text-secondary) 48%, transparent);
  }

  .row-action:hover:not(:disabled) {
    color: var(--color-danger);
  }

  .empty-state {
    display: grid;
    height: 100%;
    min-height: 160px;
    place-items: center;
    padding: 24px;
    color: var(--color-text-secondary);
    font-size: 13px;
    font-weight: 650;
    letter-spacing: 0;
    text-align: center;
  }

  .confirm-layer {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: grid;
    place-items: center;
    padding: 18px;
    background: rgb(0 0 0 / 0.42);
    backdrop-filter: blur(8px);
  }

  .confirm-dialog {
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr);
    gap: 12px;
    width: min(340px, 100%);
    padding: 14px;
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-surface-elevated) 98%, transparent);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: var(--shadow-elevated);
  }

  .confirm-icon {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    color: var(--color-warning);
    background: color-mix(in srgb, var(--color-warning) 14%, transparent);
    border-radius: 8px;
  }

  .confirm-body {
    min-width: 0;
  }

  .confirm-body h3 {
    margin: 1px 0 5px;
    font-size: 14px;
    font-weight: 720;
    letter-spacing: 0;
    line-height: 1.15;
  }

  .confirm-body p {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 520;
    letter-spacing: 0;
    line-height: 1.35;
  }

  .confirm-actions {
    display: flex;
    grid-column: 1 / -1;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .confirm-cancel,
  .confirm-clear {
    height: 32px;
    min-width: 72px;
    padding: 0 12px;
    font-size: 12px;
    font-weight: 680;
    letter-spacing: 0;
    border-radius: 7px;
    cursor: default;
  }

  .confirm-cancel {
    color: var(--color-text-primary);
    background: transparent;
    border: 1px solid var(--color-border);
  }

  .confirm-clear {
    color: #ffffff;
    background: var(--color-danger);
    border: 1px solid var(--color-danger);
  }

  .confirm-cancel:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.08);
  }

  .confirm-clear:hover:not(:disabled) {
    filter: brightness(1.08);
  }

  :global(.app-shell[data-theme='light']) .library-panel {
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.72), transparent 180px),
      color-mix(in srgb, var(--color-surface) 96%, var(--color-background));
    box-shadow: -16px 0 42px rgb(27 31 38 / 0.1);
  }

  :global(.app-shell[data-theme='light']) .history-row:hover {
    background: color-mix(in srgb, var(--color-accent) 10%, transparent);
  }

  :global(.app-shell[data-theme='light']) .icon-button:hover:not(:disabled),
  :global(.app-shell[data-theme='light']) .confirm-cancel:hover:not(:disabled) {
    background: rgb(0 0 0 / 0.065);
  }

  @media (max-width: 420px) {
    .panel-tools {
      grid-template-columns: minmax(0, 1fr);
    }

    .clear-button {
      max-width: none;
    }
  }
</style>
