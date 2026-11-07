<script lang="ts">
  import Icon from './Icon.svelte';
  import type { IconName } from '../lib/icons';
  import { internalPage, isNewTab, displayHost } from '../lib/url';

  let { url, favicon, size = 16 }: { url: string; favicon?: string; size?: number } = $props();

  let failed = $state(false);
  $effect(() => {
    void favicon;
    failed = false;
  });

  const INTERNAL_ICONS: Record<string, IconName> = {
    newtab: 'search',
    settings: 'settings',
    history: 'history',
    bookmarks: 'bookmark',
    downloads: 'download',
    extensions: 'puzzle',
    notes: 'note',
    passwords: 'key',
  };
  const internal = $derived<IconName | null>(
    isNewTab(url) ? 'search' : (INTERNAL_ICONS[internalPage(url) ?? ''] ?? null),
  );
  const letter = $derived((displayHost(url)[0] ?? '•').toUpperCase());
</script>

{#if internal}
  <span class="fav internal" style:width="{size}px" style:height="{size}px">
    <Icon name={internal} size={Math.round(size * 0.85)} />
  </span>
{:else if favicon && !failed}
  <img class="fav" src={favicon} alt="" width={size} height={size} draggable="false" onerror={() => (failed = true)} />
{:else}
  <span class="fav letter" style:width="{size}px" style:height="{size}px" style:font-size="{Math.round(size * 0.62)}px">
    {letter}
  </span>
{/if}

<style>
  .fav {
    display: grid;
    place-items: center;
    flex: none;
    border-radius: 4px;
    object-fit: contain;
  }
  .internal {
    color: var(--text-2);
  }
  .letter {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    color: var(--text);
    font-weight: 600;
    line-height: 1;
  }
</style>
