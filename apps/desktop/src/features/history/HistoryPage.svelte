<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { api } from '$platform/ipc';
  import { browser } from '$features/browser/browser.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { openInternal } from '../../app/actions';
  import { displayHost, prettyUrl } from '$shared/url';
  import type { HistoryEntry } from './types';

  const PAGE = 80;
  let query = $state('');
  let rows = $state<HistoryEntry[]>([]);
  let done = $state(false);
  let loading = $state(false);
  let selected = $state(new Set<number>());
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function load(reset: boolean) {
    if (loading) return;
    loading = true;
    try {
      const batch = await api.historySearch(query, PAGE, reset ? 0 : rows.length);
      rows = reset ? batch : [...rows, ...batch];
      done = batch.length < PAGE;
      if (reset) selected = new Set();
    } finally {
      loading = false;
    }
  }

  function onSearch() {
    clearTimeout(timer);
    timer = setTimeout(() => load(true), 120);
  }

  function dayLabel(ms: number) {
    const d = new Date(ms);
    const today = new Date();
    const yesterday = new Date(Date.now() - 86400000);
    if (d.toDateString() === today.toDateString()) return 'Today';
    if (d.toDateString() === yesterday.toDateString()) return 'Yesterday';
    return d.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' });
  }

  const groups = $derived.by(() => {
    const out: { label: string; items: HistoryEntry[] }[] = [];
    for (const r of rows) {
      const label = query ? 'Results' : dayLabel(r.lastVisit);
      const last = out[out.length - 1];
      if (last?.label === label) last.items.push(r);
      else out.push({ label, items: [r] });
    }
    return out;
  });

  async function remove(ids: number[]) {
    await api.historyDelete(ids);
    rows = rows.filter((r) => !ids.includes(r.id));
    selected = new Set();
    ui.toast(ids.length === 1 ? 'Removed from history' : `Removed ${ids.length} entries`, { icon: 'trash' });
  }

  function toggle(id: number) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }

  function onScroll(e: Event) {
    const el = e.currentTarget as HTMLElement;
    if (!done && el.scrollTop + el.clientHeight > el.scrollHeight - 400) void load(false);
  }

  onMount(() => void load(true));
</script>

<div class="page" onscroll={onScroll}>
  <div class="inner">
    <header>
      <h1>History</h1>
      <div class="tools">
        {#if selected.size}
          <button class="btn danger" onclick={() => remove([...selected])}><Icon name="trash" size={15} />Delete {selected.size}</button>
        {/if}
        <button class="btn" onclick={() => openInternal('settings', 'privacy')}><Icon name="eraser" size={15} />Clear browsing data</button>
      </div>
    </header>
    <div class="search">
      <Icon name="search" size={16} />
      <input placeholder="Search history" bind:value={query} oninput={onSearch} spellcheck="false" />
    </div>

    {#each groups as g (g.label)}
      <h2>{g.label}</h2>
      <div class="list">
        {#each g.items as r (r.id)}
          <div class="row" class:sel={selected.has(r.id)}>
            <input type="checkbox" checked={selected.has(r.id)} onchange={() => toggle(r.id)} aria-label="Select" />
            <span class="time">{new Date(r.lastVisit).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}</span>
            <button class="link" onclick={(e) => browser.openTab(r.url, { activate: !(e.ctrlKey || e.metaKey) })} title={r.url}>
              <span class="letter">{displayHost(r.url)[0]?.toUpperCase()}</span>
              <span class="title ellipsis">{r.title || prettyUrl(r.url)}</span>
              <span class="host ellipsis">{displayHost(r.url)}</span>
            </button>
            {#if r.visitCount > 1}<span class="count" title="Visits">{r.visitCount}×</span>{/if}
            <button class="icon-btn sm del" title="Remove" onclick={() => remove([r.id])}><Icon name="close" size={14} /></button>
          </div>
        {/each}
      </div>
    {:else}
      {#if !loading}
        <div class="empty"><Icon name="history" size={28} /><p>{query ? 'No matching pages' : 'Pages you visit will show up here'}</p></div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .page {
    height: 100%;
    overflow-y: auto;
  }
  .inner {
    max-width: 860px;
    margin: 0 auto;
    padding: 28px 32px 60px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  h1 {
    margin: 0;
    font: 600 22px/1.2 var(--font-display);
  }
  .tools {
    display: flex;
    gap: 8px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    margin: 18px 0 8px;
    padding: 0 14px;
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-3);
  }
  .search input {
    flex: 1;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 14px;
  }
  h2 {
    margin: 22px 0 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-2);
  }
  .list {
    display: flex;
    flex-direction: column;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 40px;
    padding: 0 8px;
    border-radius: var(--radius);
  }
  .row:hover,
  .row.sel {
    background: var(--hover);
  }
  .row input {
    accent-color: var(--accent);
    opacity: 0.4;
  }
  .row:hover input,
  .row.sel input {
    opacity: 1;
  }
  .time {
    width: 46px;
    font-size: 12px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .link {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
  }
  .letter {
    width: 20px;
    height: 20px;
    flex: none;
    border-radius: 5px;
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 700;
    background: color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .title {
    min-width: 0;
    max-width: 60%;
  }
  .host {
    color: var(--text-3);
    font-size: 12px;
    min-width: 0;
  }
  .count {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .del {
    opacity: 0;
  }
  .row:hover .del {
    opacity: 1;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 80px 0;
    color: var(--text-3);
  }
</style>
