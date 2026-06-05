<script lang="ts">
  import {
    Bookmark as BookmarkIcon,
    BookmarkPlus,
    Globe,
    Search,
    Trash2,
    X
  } from 'lucide-svelte';
  import { bookmarksState } from '../state/bookmarks.svelte';
  import { tabsState } from '../state/tabs.svelte';
  import { toastsState } from '../state/toasts.svelte';
  import type { Bookmark } from '../utils/tauri';
  import { displayUrl, domainFromUrl } from '../utils/url';

  interface Props {
    open: boolean;
    onClose: () => void;
    onNavigate: (url: string) => Promise<void>;
  }

  let { open, onClose, onNavigate }: Props = $props();

  let searchQuery = $state('');
  let deletingId = $state<string | null>(null);
  let failedFavicons = $state<Record<string, true>>({});
  let wasOpen = false;

  let activeTab = $derived(tabsState.activeTab);
  let activeUrl = $derived(activeTab?.url ?? '');
  let activeBookmark = $derived(bookmarksState.findByUrl(activeUrl));
  let canAddCurrent = $derived(isBookmarkableUrl(activeUrl) && !activeBookmark);
  let visibleBookmarks = $derived(filterBookmarks(bookmarksState.items, searchQuery));
  let emptyLabel = $derived(searchQuery.trim() ? 'No matching bookmarks' : 'No bookmarks saved');
  let countLabel = $derived(formatCount(visibleBookmarks.length, 'bookmark'));

  $effect(() => {
    if (open && !wasOpen) {
      void bookmarksState.load();
    }

    if (!open && wasOpen) {
      searchQuery = '';
    }

    wasOpen = open;
  });

  function handleSearchInput(event: Event): void {
    searchQuery = (event.currentTarget as HTMLInputElement).value;
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (open && event.key === 'Escape') {
      onClose();
    }
  }

  async function addCurrentPage(): Promise<void> {
    const tab = activeTab;

    if (!tab || !isBookmarkableUrl(tab.url)) {
      return;
    }

    const bookmark = await bookmarksState.addBookmark(tab.title.trim() || tab.url, tab.url);
    if (bookmark) {
      toastsState.success('Bookmark saved');
    }
  }

  async function navigateTo(url: string): Promise<void> {
    try {
      await onNavigate(url);
      onClose();
    } catch (error) {
      toastsState.error(`Failed to open bookmark: ${errorMessage(error)}`);
    }
  }

  async function deleteBookmark(id: string): Promise<void> {
    deletingId = id;

    try {
      await bookmarksState.removeBookmark(id);
    } finally {
      deletingId = null;
    }
  }

  function markFaviconFailed(id: string): void {
    failedFavicons = {
      ...failedFavicons,
      [id]: true
    };
  }

  function filterBookmarks(bookmarks: Bookmark[], query: string): Bookmark[] {
    const normalizedQuery = query.trim().toLocaleLowerCase();

    if (!normalizedQuery) {
      return bookmarks;
    }

    return bookmarks.filter((bookmark) => {
      const title = bookmark.title.toLocaleLowerCase();
      const url = bookmark.url.toLocaleLowerCase();
      return title.includes(normalizedQuery) || url.includes(normalizedQuery);
    });
  }

  function isBookmarkableUrl(url: string): boolean {
    return url.startsWith('http://') || url.startsWith('https://');
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

  function formatCount(count: number, singular: string): string {
    return `${count} ${count === 1 ? singular : `${singular}s`}`;
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
  <aside class="library-panel" aria-label="Bookmarks">
    <header class="panel-header">
      <div class="panel-heading">
        <BookmarkIcon size={18} strokeWidth={2.25} />
        <div class="panel-heading__text">
          <h2>Bookmarks</h2>
          <span>{countLabel}</span>
        </div>
      </div>

      <button class="icon-button" type="button" aria-label="Close bookmarks" title="Close" onclick={onClose}>
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
          aria-label="Search bookmarks"
          placeholder="Search"
          oninput={handleSearchInput}
        />
      </label>

      {#if canAddCurrent}
        <button
          class="add-current"
          type="button"
          disabled={bookmarksState.isSaving}
          onclick={addCurrentPage}
        >
          <BookmarkPlus size={16} strokeWidth={2.35} />
          <span>Add Current Page</span>
        </button>
      {/if}
    </div>

    <div class="panel-body">
      {#if bookmarksState.isLoading}
        <div class="empty-state" role="status">Loading bookmarks</div>
      {:else if visibleBookmarks.length === 0}
        <div class="empty-state" role="status">{emptyLabel}</div>
      {:else}
        <div class="bookmark-list" role="list">
          {#each visibleBookmarks as bookmark (bookmark.id)}
            {@const iconUrl = faviconUrl(bookmark.url)}
            <article class="bookmark-row" role="listitem">
              <button
                class="bookmark-main"
                type="button"
                title={bookmark.url}
                onclick={() => navigateTo(bookmark.url)}
              >
                <span class="favicon" aria-hidden="true">
                  {#if iconUrl && !failedFavicons[bookmark.id]}
                    <img
                      src={iconUrl}
                      alt=""
                      draggable="false"
                      onerror={() => markFaviconFailed(bookmark.id)}
                    />
                  {:else}
                    <Globe size={16} strokeWidth={2.15} />
                  {/if}
                </span>

                <span class="bookmark-text">
                  <span class="bookmark-title">{bookmark.title || domainFromUrl(bookmark.url)}</span>
                  <span class="bookmark-url">{displayUrl(bookmark.url)}</span>
                </span>
              </button>

              <button
                class="icon-button row-action"
                type="button"
                aria-label={`Delete ${bookmark.title || bookmark.url}`}
                title="Delete"
                disabled={deletingId === bookmark.id}
                onclick={() => deleteBookmark(bookmark.id)}
              >
                <Trash2 size={16} strokeWidth={2.25} />
              </button>
            </article>
          {/each}
        </div>
      {/if}
    </div>
  </aside>
{/if}

<style>
  .library-panel {
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
    gap: 8px;
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

  .add-current {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    height: 34px;
    min-width: 0;
    padding: 0 10px;
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-accent) 38%, var(--color-border));
    border-radius: 8px;
    cursor: default;
  }

  .add-current span {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 680;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .add-current:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-accent) 26%, transparent);
  }

  .panel-body {
    min-height: 0;
    overflow: hidden;
  }

  .bookmark-list {
    display: grid;
    gap: 2px;
    height: 100%;
    padding: 8px;
    overflow-y: auto;
  }

  .bookmark-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 30px;
    gap: 4px;
    align-items: center;
    min-width: 0;
    min-height: 44px;
    border-radius: 8px;
  }

  .bookmark-row:hover {
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  }

  .bookmark-main {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    min-width: 0;
    height: 44px;
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

  .bookmark-text {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  .bookmark-title,
  .bookmark-url {
    min-width: 0;
    overflow: hidden;
    letter-spacing: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bookmark-title {
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 670;
    line-height: 1.1;
  }

  .bookmark-url {
    color: var(--color-text-secondary);
    font-size: 11px;
    font-weight: 520;
    line-height: 1.1;
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
  .add-current:disabled {
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

  :global(.app-shell[data-theme='light']) .library-panel {
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.72), transparent 180px),
      color-mix(in srgb, var(--color-surface) 96%, var(--color-background));
    box-shadow: -16px 0 42px rgb(27 31 38 / 0.1);
  }

  :global(.app-shell[data-theme='light']) .bookmark-row:hover {
    background: color-mix(in srgb, var(--color-accent) 10%, transparent);
  }

  :global(.app-shell[data-theme='light']) .icon-button:hover:not(:disabled) {
    background: rgb(0 0 0 / 0.065);
  }
</style>
