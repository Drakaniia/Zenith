<script lang="ts">
  import { ArrowRight, Loader2, Search } from 'lucide-svelte';
  import { normalizeUrl } from '../../../bridge/commands/navigation';
  import { toastsState } from '../../../shell/state/toasts.svelte';

  interface Props {
    onNavigate: (url: string) => Promise<void>;
  }

  let { onNavigate }: Props = $props();

  let query = $state('');
  let searchElement: HTMLInputElement | undefined = $state();
  let isSubmitting = $state(false);
  let validationError = $state('');
  let didFocusSearch = false;

  $effect(() => {
    if (didFocusSearch) {
      return;
    }

    didFocusSearch = true;

    window.setTimeout(() => {
      searchElement?.focus();
    }, 0);
  });

  async function submitSearch(): Promise<void> {
    const rawInput = query.trim();

    if (!rawInput) {
      validationError = 'Enter a URL or search query';
      searchElement?.focus();
      return;
    }

    isSubmitting = true;
    validationError = '';

    try {
      const normalizedUrl = await normalizeUrl(rawInput);
      await onNavigate(normalizedUrl);
      query = '';
    } catch (error) {
      validationError = errorMessage(error);
      toastsState.error(`Navigation failed: ${errorMessage(error)}`);
    } finally {
      isSubmitting = false;
    }
  }

  function handleSearchInput(event: Event): void {
    query = (event.currentTarget as HTMLInputElement).value;
    validationError = '';
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

<section class="home-page" aria-label="Browser home">
  <div class="home-page__glow" aria-hidden="true"></div>

  <form
    class="home-search"
    aria-label="Search or enter address"
    onsubmit={(event) => {
      event.preventDefault();
      void submitSearch();
    }}
  >
    <label class={`home-search__box ${validationError ? 'home-search__box--error' : ''}`}>
      <Search size={21} strokeWidth={2.25} />
      <input
        bind:this={searchElement}
        type="text"
        value={query}
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        aria-label="Search or enter address"
        aria-invalid={validationError ? 'true' : 'false'}
        aria-describedby={validationError ? 'home-search-error' : undefined}
        placeholder="Search or enter an address"
        oninput={handleSearchInput}
      />
      <button
        class="home-search__submit"
        type="submit"
        aria-label="Go"
        title="Go"
        disabled={isSubmitting}
      >
        {#if isSubmitting}
          <Loader2 size={18} strokeWidth={2.35} />
        {:else}
          <ArrowRight size={19} strokeWidth={2.35} />
        {/if}
      </button>
    </label>

    {#if validationError}
      <p class="home-search__error" id="home-search-error" role="alert">{validationError}</p>
    {/if}
  </form>
</section>

<style>
  .home-page {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: grid;
    place-items: center;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    color: var(--color-text-primary);
    background:
      radial-gradient(circle at 34% 48%, rgb(252 128 87 / 0.18), transparent 22rem),
      radial-gradient(circle at 67% 48%, rgb(217 70 239 / 0.18), transparent 24rem),
      linear-gradient(140deg, #17181c 0%, #201b24 48%, #15161a 100%);
  }

  .home-page::before {
    position: absolute;
    inset: 0;
    pointer-events: none;
    content: '';
    background-image:
      linear-gradient(rgb(255 255 255 / 0.022) 1px, transparent 1px),
      linear-gradient(90deg, rgb(255 255 255 / 0.018) 1px, transparent 1px);
    background-size: 44px 44px;
    mask-image: radial-gradient(circle at center, black, transparent 72%);
  }

  .home-page__glow {
    position: absolute;
    width: min(48rem, 82vw);
    height: 14rem;
    background: linear-gradient(
      90deg,
      rgb(251 146 60 / 0.18),
      rgb(236 72 153 / 0.23),
      rgb(34 211 238 / 0.14)
    );
    filter: blur(54px);
    opacity: 0.76;
  }

  .home-search {
    position: relative;
    z-index: 1;
    display: grid;
    justify-items: center;
    gap: 9px;
    width: min(720px, calc(100% - 32px));
  }

  .home-search__box {
    display: grid;
    grid-template-columns: 42px minmax(0, 1fr) 42px;
    align-items: center;
    width: 100%;
    height: 58px;
    min-width: 0;
    overflow: hidden;
    color: #20242a;
    background: rgb(255 255 255 / 0.96);
    border: 1px solid rgb(255 255 255 / 0.8);
    border-radius: 8px;
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 0.1),
      0 22px 70px rgb(236 72 153 / 0.22),
      0 12px 42px rgb(251 146 60 / 0.17);
  }

  .home-search__box:focus-within {
    box-shadow:
      0 0 0 2px rgb(255 255 255 / 0.9),
      0 0 0 5px rgb(34 211 238 / 0.22),
      0 22px 74px rgb(236 72 153 / 0.26);
  }

  .home-search__box--error {
    box-shadow:
      0 0 0 2px rgb(239 68 68 / 0.88),
      0 18px 60px rgb(239 68 68 / 0.18);
  }

  .home-search__box :global(svg) {
    justify-self: center;
  }

  .home-search__box input {
    min-width: 0;
    height: 100%;
    padding: 0;
    color: #16191f;
    font-size: 17px;
    font-weight: 520;
    background: transparent;
    border: 0;
    outline: 0;
  }

  .home-search__box input::placeholder {
    color: #858b94;
  }

  .home-search__submit {
    display: grid;
    width: 34px;
    height: 34px;
    min-width: 34px;
    place-items: center;
    padding: 0;
    color: #111827;
    background: transparent;
    border: 0;
    border-radius: 8px;
    cursor: default;
  }

  .home-search__submit:hover:not(:disabled) {
    background: rgb(17 24 39 / 0.09);
  }

  .home-search__submit:disabled {
    color: #717680;
  }

  .home-search__submit:disabled :global(svg) {
    animation: home-spin 820ms linear infinite;
  }

  .home-search__error {
    max-width: 100%;
    margin: 0;
    color: #fecaca;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0;
    text-align: center;
  }

  :global(.app-shell[data-theme='light']) .home-page {
    color: #111827;
    background:
      radial-gradient(circle at 34% 48%, rgb(251 146 60 / 0.2), transparent 22rem),
      radial-gradient(circle at 67% 48%, rgb(14 165 233 / 0.16), transparent 24rem),
      linear-gradient(140deg, #f4f7fb 0%, #ebe7ef 48%, #f6f6f2 100%);
  }

  :global(.app-shell[data-theme='light']) .home-search__error {
    color: #b91c1c;
  }

  @media (max-width: 760px) {
    .home-search {
      width: min(100% - 24px, 640px);
    }

    .home-search__box {
      height: 52px;
      grid-template-columns: 38px minmax(0, 1fr) 40px;
    }
  }

  @keyframes home-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
