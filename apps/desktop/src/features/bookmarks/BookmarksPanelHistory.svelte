<script lang="ts">
  import Favicon from '$shared/ui/Favicon.svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { applyClick, prune, selectAll } from '$features/history/selection';
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

  // Ctrl/Cmd-click toggles rows, Shift-click selects a range, Delete removes the selection.
  let selected = $state(new Set<number>());
  let anchor: number | null = null;

  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;
  $effect(() => {
    const text = query;
    clearTimeout(timer);
    const mine = ++seq;
    timer = setTimeout(
      async () => {
        const r = await historyApi.historySearch(text.trim(), 80).catch(() => []);
        if (mine !== seq) return;
        history = r;
        const kept = prune({ selected, anchor }, r.map((h) => h.id));
        selected = kept.selected;
        anchor = kept.anchor;
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
  const ids = () => history.map((h) => h.id);

  function click(e: MouseEvent, h: HistoryEntry) {
    if (e.ctrlKey || e.metaKey || e.shiftKey) {
      const s = applyClick(ids(), { selected, anchor }, h.id, { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey });
      selected = s.selected;
      anchor = s.anchor;
    } else openBookmark(h.url, mouseHow(e));
  }

  async function removeSelected() {
    const gone = [...selected];
    if (!gone.length) return;
    await historyApi.historyDelete(gone);
    history = history.filter((h) => !selected.has(h.id));
    selected = new Set();
    anchor = null;
    ui.toast(gone.length === 1 ? 'Removed from history' : `Removed ${gone.length} entries`, { icon: 'trash' });
  }

  function onkeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      const s = selectAll(ids());
      selected = s.selected;
      anchor = s.anchor;
    } else if (e.key === 'Delete' && selected.size) {
      e.preventDefault();
      void removeSelected();
    } else if (e.key === 'Escape' && selected.size) {
      e.stopPropagation();
      selected = new Set();
      anchor = null;
    }
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

<!-- The keys reach the list from the focused row; the list itself is not a control. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="list" bind:this={list} role="group" tabindex="-1" {onkeydown}>
  {#if selected.size}
    <div class="bar">
      <span>{selected.size} selected</span>
      <button class="btn danger" onclick={removeSelected}><Icon name="trash" size={14} />Delete</button>
    </div>
  {/if}
  {#each days as d (d.label)}
    <div class="group">{d.label}</div>
    {#each d.items as h (h.id)}
      <button
        class="row leaf"
        class:sel={selected.has(h.id)}
        onmousedown={(e) => e.shiftKey && e.preventDefault()}
        title={h.url}
        onclick={(e) => click(e, h)}
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
  .row.sel {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .bar {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 2px;
    padding: 4px 6px 4px 8px;
    border-radius: var(--r-item);
    background: var(--field);
    font-size: 12px;
    color: var(--text-2);
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
