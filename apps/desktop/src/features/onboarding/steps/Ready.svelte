<script lang="ts">
  import { onMount } from 'svelte';
  import { onboarding } from '../onboarding.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { defaultBrowserApi } from '$features/default-browser/api';
  import { SEARCH_ENGINES, prettyUrl } from '$shared/url';
  import { ACCENT_NAMES, plural, type StepId } from '../onboarding-flow';

  const s = $derived(settings.value);
  let isDefault = $state<boolean | null>(null);
  let asked = $state(false);

  onMount(() => {
    void defaultBrowserApi
      .defaultBrowserStatus()
      .then((d) => (isDefault = d.isDefault))
      .catch(() => (isDefault = null));
  });

  const THEME = { system: 'Matches Windows', light: 'Light', dark: 'Dark' } as const;
  const LAYOUT = { single: 'Single toolbar', multiple: 'Multiple toolbars', collapsed: 'Collapsed' } as const;
  const keep = $derived(
    s.historyDays === 0 ? 'kept forever' : `kept ${({ 7: '1 week', 30: '1 month', 90: '3 months', 365: '1 year' } as Record<number, string>)[s.historyDays] ?? `${s.historyDays} days`}`,
  );

  const rows = $derived.by(() => {
    const imp = onboarding.imported;
    const list: { step: StepId; label: string; value: string; swatch?: string }[] = [
      { step: 'appearance', label: 'Look', value: `${THEME[s.theme]} · ${LAYOUT[s.toolbarMode]}`, swatch: onboarding.accent },
      { step: 'search', label: 'Search', value: `${SEARCH_ENGINES[s.searchEngine]?.name ?? 'Google'}${s.searchSuggestions ? ', with suggestions' : ''}` },
      {
        step: 'start',
        label: 'Opens with',
        value: `${s.restoreSession ? 'Your last tabs' : 'A fresh window'} · ${s.homepage ? prettyUrl(s.homepage) : 'Zenith start page'}`,
      },
      {
        step: 'import',
        label: 'Imported',
        value:
          imp.bookmarks || imp.history
            ? [imp.bookmarks && plural(imp.bookmarks, 'bookmark'), imp.history && plural(imp.history, 'history entry', 'history entries')]
                .filter(Boolean)
                .join(', ') + ` from ${imp.from.join(' and ')}`
            : 'Nothing yet',
      },
      {
        step: 'privacy',
        label: 'Privacy',
        value: [
          s.blockerEnabled ? 'Ads and trackers blocked' : 'Ads allowed',
          s.recordHistory ? `history ${keep}` : 'no history',
          s.autoUpdate ? 'automatic updates' : 'manual updates',
        ].join(', '),
      },
    ];
    return list;
  });

  async function makeDefault() {
    asked = true;
    try {
      await defaultBrowserApi.defaultBrowserRegister();
    } catch {
      asked = false;
    }
  }
</script>

<dl class="summary">
  {#each rows as r (r.step)}
    <div class="item">
      <dt>{r.label}</dt>
      <dd>
        {#if r.swatch}<span class="swatch" style:--c={r.swatch} title={ACCENT_NAMES[r.swatch] ?? r.swatch}></span>{/if}
        <span class="value">{r.value}</span>
      </dd>
      <button type="button" class="change" onclick={() => onboarding.goTo(r.step)} aria-label="Change {r.label.toLowerCase()}">Change</button>
    </div>
  {/each}
</dl>

{#if isDefault === false}
  <div class="default">
    {#if asked}
      <p role="status">In the Windows Settings window that opened, choose Zenith as your web browser.</p>
    {:else}
      <p>Open links from other apps in Zenith?</p>
      <button type="button" class="btn" onclick={makeDefault}>Make Zenith my default browser</button>
    {/if}
  </div>
{/if}

<style>
  .summary {
    margin: 0;
    display: grid;
    border-radius: var(--radius-lg);
    corner-shape: var(--sq);
    box-shadow: inset 0 0 0 1px var(--line);
    overflow: hidden;
  }
  .item {
    display: grid;
    grid-template-columns: 110px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 10px 8px 10px 14px;
    animation: rise 420ms cubic-bezier(0.23, 1, 0.32, 1) both;
  }
  .item:nth-child(1) { animation-delay: 120ms; }
  .item:nth-child(2) { animation-delay: 160ms; }
  .item:nth-child(3) { animation-delay: 200ms; }
  .item:nth-child(4) { animation-delay: 240ms; }
  .item:nth-child(5) { animation-delay: 280ms; }
  @keyframes rise {
    from {
      opacity: 0;
      translate: 0 6px;
    }
  }
  .item + .item {
    border-top: 1px solid var(--line);
  }
  dt {
    color: var(--text-3);
    font-size: 12px;
  }
  dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .swatch {
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--c);
  }
  .change {
    height: 26px;
    padding: 0 10px;
    border-radius: var(--r-btn);
    font-size: 12px;
    color: var(--text-2);
    transition: background-color var(--fast) ease;
  }
  .change:hover {
    background: var(--hover);
    color: var(--text);
  }
  .default {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px 16px;
    margin-top: 16px;
  }
  .default p {
    margin: 0;
    color: var(--text-2);
  }
  @media (max-width: 560px) {
    .item {
      grid-template-columns: 1fr auto;
    }
    dt {
      grid-column: 1 / -1;
      margin-bottom: -8px;
    }
  }
</style>
