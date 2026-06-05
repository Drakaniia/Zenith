<script lang="ts">
  import { CaseSensitive, ChevronDown, ChevronUp, Search, X } from 'lucide-svelte';
  import type { FindResult } from '../../../shared/types/backend';
  import { toastsState } from '../../../shell/state/toasts.svelte';

  interface Props {
    open: boolean;
    onClose: () => void;
    onFind: (
      query: string,
      direction: 'next' | 'previous',
      caseSensitive: boolean
    ) => Promise<FindResult>;
    onClear: () => Promise<void>;
    stepRequest?: number;
    stepDirection?: 'next' | 'previous';
  }

  const EMPTY_RESULT: FindResult = {
    active_match: 0,
    total_matches: 0
  };

  let { open, onClose, onFind, onClear, stepRequest = 0, stepDirection = 'next' }: Props = $props();

  let query = $state('');
  let caseSensitive = $state(false);
  let result = $state<FindResult>(EMPTY_RESULT);
  let isBusy = $state(false);
  let inputElement: HTMLInputElement | undefined = $state();
  let findVersion = 0;
  let wasOpen = false;
  let hasReportedUnavailable = false;
  let lastStepRequest = 0;

  let matchLabel = $derived(formatMatchLabel(query, result));

  $effect(() => {
    if (open && !wasOpen) {
      hasReportedUnavailable = false;
      window.setTimeout(() => {
        inputElement?.focus();
        inputElement?.select();
      }, 0);
    }

    if (!open && wasOpen) {
      query = '';
      result = EMPTY_RESULT;
      isBusy = false;
      void clearFindHighlights();
    }

    wasOpen = open;
  });

  $effect(() => {
    const currentOpen = open;
    const currentQuery = query.trim();
    const currentCaseSensitive = caseSensitive;

    if (!currentOpen) {
      return;
    }

    if (!currentQuery) {
      result = EMPTY_RESULT;
      void clearFindHighlights();
      return;
    }

    let cancelled = false;
    const timeout = window.setTimeout(() => {
      void runFind(currentQuery, 'next', currentCaseSensitive, () => cancelled);
    }, 120);

    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  });

  $effect(() => {
    const request = stepRequest;

    if (!open || request <= 0 || request === lastStepRequest) {
      return;
    }

    lastStepRequest = request;
    void runFind(query.trim(), stepDirection, caseSensitive);
  });

  function handleInput(event: Event): void {
    query = (event.currentTarget as HTMLInputElement).value;
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      void closeBar();
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      void runFind(query.trim(), event.shiftKey ? 'previous' : 'next', caseSensitive);
    }
  }

  async function move(direction: 'next' | 'previous'): Promise<void> {
    await runFind(query.trim(), direction, caseSensitive);
  }

  async function closeBar(): Promise<void> {
    await clearFindHighlights();
    onClose();
  }

  async function clearFindHighlights(): Promise<void> {
    try {
      await onClear();
    } catch {
      // Clearing highlights is best-effort because inactive/internal tabs may not have a webview.
    }
  }

  async function runFind(
    currentQuery: string,
    direction: 'next' | 'previous',
    currentCaseSensitive: boolean,
    isCancelled: () => boolean = () => false
  ): Promise<void> {
    if (!currentQuery) {
      result = EMPTY_RESULT;
      await clearFindHighlights();
      return;
    }

    const version = ++findVersion;
    isBusy = true;

    try {
      const nextResult = await onFind(currentQuery, direction, currentCaseSensitive);

      if (version === findVersion && !isCancelled()) {
        result = nextResult;
      }
    } catch (error) {
      if (version === findVersion && !isCancelled()) {
        result = EMPTY_RESULT;
        reportFindError(error);
      }
    } finally {
      if (version === findVersion && !isCancelled()) {
        isBusy = false;
      }
    }
  }

  function reportFindError(error: unknown): void {
    const message = errorMessage(error);

    if (isUnavailablePlatformError(message)) {
      if (!hasReportedUnavailable) {
        hasReportedUnavailable = true;
        toastsState.error(`Find in page is unavailable: ${message}`);
      }
      return;
    }

    toastsState.error(`Find failed: ${message}`);
  }

  function isUnavailablePlatformError(message: string): boolean {
    return message.toLocaleLowerCase().includes('find in page is unavailable');
  }

  function formatMatchLabel(currentQuery: string, currentResult: FindResult): string {
    if (!currentQuery.trim()) {
      return '0 / 0';
    }

    if (currentResult.total_matches <= 0) {
      return 'No matches';
    }

    return `${currentResult.active_match} / ${currentResult.total_matches}`;
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

{#if open}
  <section class="find-bar" aria-label="Find in page">
    <div class="find-input-wrap">
      <Search size={15} strokeWidth={2.25} />
      <input
        bind:this={inputElement}
        class="find-input"
        type="search"
        value={query}
        autocomplete="off"
        spellcheck="false"
        aria-label="Find text"
        placeholder="Find in page"
        oninput={handleInput}
        onkeydown={handleKeydown}
      />
    </div>

    <span
      class={`match-count ${result.total_matches === 0 && query.trim() ? 'match-count--empty' : ''}`}
    >
      {matchLabel}
    </span>

    <button
      class="icon-button"
      type="button"
      aria-label="Previous match"
      title="Previous match"
      disabled={!query.trim() || isBusy}
      onclick={() => move('previous')}
    >
      <ChevronUp size={17} strokeWidth={2.35} />
    </button>

    <button
      class="icon-button"
      type="button"
      aria-label="Next match"
      title="Next match"
      disabled={!query.trim() || isBusy}
      onclick={() => move('next')}
    >
      <ChevronDown size={17} strokeWidth={2.35} />
    </button>

    <button
      class={`icon-button case-button ${caseSensitive ? 'case-button--active' : ''}`}
      type="button"
      aria-label="Match case"
      aria-pressed={caseSensitive}
      title="Match case"
      onclick={() => {
        caseSensitive = !caseSensitive;
      }}
    >
      <CaseSensitive size={18} strokeWidth={2.2} />
    </button>

    <button
      class="icon-button"
      type="button"
      aria-label="Close find bar"
      title="Close"
      onclick={closeBar}
    >
      <X size={17} strokeWidth={2.35} />
    </button>
  </section>
{/if}

<style>
  .find-bar {
    display: grid;
    grid-template-columns: minmax(160px, 320px) minmax(68px, auto) 30px 30px 30px 30px;
    gap: 6px;
    align-items: center;
    justify-content: end;
    width: 100%;
    min-height: 42px;
    padding: 5px 8px;
    color: var(--color-text-primary);
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.035), transparent),
      color-mix(in srgb, var(--color-surface) 92%, var(--color-background));
    border-bottom: 1px solid var(--color-border);
    user-select: none;
  }

  .find-input-wrap {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr);
    align-items: center;
    height: 30px;
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    background: color-mix(in srgb, var(--color-background) 72%, var(--color-surface));
    border: 1px solid color-mix(in srgb, var(--color-border) 86%, transparent);
    border-radius: 8px;
  }

  .find-input-wrap:focus-within {
    color: var(--color-text-primary);
    border-color: color-mix(in srgb, var(--color-accent) 76%, var(--color-border));
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 18%, transparent);
  }

  .find-input-wrap :global(svg) {
    justify-self: center;
  }

  .find-input {
    min-width: 0;
    height: 100%;
    padding: 0 8px 0 0;
    color: var(--color-text-primary);
    background: transparent;
    border: 0;
    outline: 0;
  }

  .find-input::-webkit-search-cancel-button {
    display: none;
  }

  .find-input::placeholder {
    color: color-mix(in srgb, var(--color-text-secondary) 72%, transparent);
  }

  .match-count {
    min-width: 68px;
    overflow: hidden;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0;
    line-height: 1;
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .match-count--empty {
    color: var(--color-warning);
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

  .icon-button:disabled {
    color: color-mix(in srgb, var(--color-text-secondary) 42%, transparent);
  }

  .case-button--active {
    color: var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 16%, transparent);
  }

  :global(.app-shell[data-theme='light']) .find-bar {
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.86), transparent),
      color-mix(in srgb, var(--color-surface) 94%, var(--color-background));
  }

  :global(.app-shell[data-theme='light']) .find-input-wrap {
    background: color-mix(in srgb, var(--color-surface) 84%, var(--color-background));
  }

  :global(.app-shell[data-theme='light']) .icon-button:hover:not(:disabled) {
    background: rgb(0 0 0 / 0.065);
  }

  @media (max-width: 560px) {
    .find-bar {
      grid-template-columns: minmax(112px, 1fr) 56px 30px 30px 30px 30px;
      gap: 4px;
      padding-inline: 5px;
    }

    .match-count {
      min-width: 56px;
      font-size: 11px;
    }
  }
</style>
