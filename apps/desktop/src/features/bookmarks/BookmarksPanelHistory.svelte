<script lang="ts">
  import Favicon from '$shared/ui/Favicon.svelte';
  import { historyApi } from '$features/history/api';
  import { faviconFor } from '$shared/favicons';
  import { mouseHow, openBookmark } from './bookmark-actions';
  import { displayHost, prettyUrl } from '$shared/url';
  import type { HistoryEntry } from '$features/history/types';

  // The bookmarks sidebar's history view: pages visited, grouped by day, filtered by the
  // sidebar's search box. The panel keeps the list, so switching views shows the last one
  // until the new search answers.

  let {
    query,
    history = $bindable(),
    list = $bindable(),
  }: {
    query: string;
    history: HistoryEntry[];
    list?: HTMLDivElement;
  } = $props();

  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;
  $effect(() => {
    const text = query;
    clearTimeout(timer);
    const mine = ++seq;
    timer = setTimeout(
      async () => {
        const r = await historyApi.historySearch(text.trim(), 80).catch(() => []);
        if (mine === seq) history = r;
      },
      text ? 120 : 0,
    );
  });

  function dayLabel(ms: number) {
    const d = new Date(ms);
    if (d.toDateString() === new Date().toDateString()) return 'Today';
    if (d.toDateString() === new Date(Date.now() - 86_400_000).toDateString()) return 'Yesterday';
    return d.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' });
  }
  const days = $derived.by(() => {
    const out: { label: string; items: HistoryEntry[] }[] = [];
    for (const h of history) {
      const l = query.trim() ? 'Results' : dayLabel(h.lastVisit);
      const last = out[out.length - 1];
      if (last?.label === l) last.items.push(h);
      else out.push({ label: l, items: [h] });
    }
    return out;
  });
</script>

<div class="list" bind:this={list}>
  {#each days as d (d.label)}
    <div class="group">{d.label}</div>
    {#each d.items as h (h.id)}
      <button
        class="row leaf"
        title={h.url}
        onclick={(e) => openBookmark(h.url, mouseHow(e))}
        onauxclick={(e) => e.button === 1 && openBookmark(h.url, 'background')}
      >
        <span class="tw"></span>
        <Favicon url={h.url} favicon={faviconFor(h.url)} size={16} />
        <span class="label ellipsis">{h.title || prettyUrl(h.url)}</span>
        <span class="sub ellipsis">{displayHost(h.url)}</span>
      </button>
    {/each}
  {:else}
    <p class="empty">{query ? 'No matching pages' : 'Pages you visit will show up here'}</p>
  {/each}
</div>

<style>
  /* The list and row rules match BookmarksPanel's bookmark lists. */
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 4px 8px;
    outline: none;
  }
  .group {
    padding: 10px 8px 4px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  .row {
    position: relative;
    width: 100%;
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 28px;
    padding-right: 8px;
    border-radius: var(--r-item);
    corner-shape: var(--sq);
    text-align: left;
    font-size: 13px;
    color: var(--text);
  }
  .row.leaf {
    padding-left: 4px;
  }
  .row:hover {
    background: var(--hover);
  }
  .tw {
    width: 18px;
    height: 24px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .label {
    min-width: 0;
    flex: 1;
  }
  .sub {
    flex: 0 1 38%;
    min-width: 0;
    font-size: 11.5px;
    color: var(--text-3);
    text-align: right;
  }
  .empty {
    margin: 24px 16px;
    text-align: center;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-3);
  }
</style>
