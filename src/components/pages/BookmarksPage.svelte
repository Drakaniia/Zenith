<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '../Icon.svelte';
  import { api } from '../../lib/ipc';
  import { bookmarks } from '../../lib/bookmarks.svelte';
  import { browser } from '../../lib/browser.svelte';
  import { ui } from '../../lib/ui.svelte';
  import { scoreAny } from '../../lib/fuzzy';
  import { displayHost, prettyUrl } from '../../lib/url';
  import type { Bookmark } from '../../lib/types';

  let query = $state('');
  let folder = $state<string | null>(null);
  let editing = $state<Bookmark | null>(null);

  onMount(() => void bookmarks.load(true));

  const visible = $derived(
    bookmarks.items
      .filter((b) => folder === null || b.folder === folder)
      .map((b) => ({ b, s: query ? scoreAny(query, b.title, b.url, b.folder) : 1 }))
      .filter((x) => x.s > 0)
      .sort((a, b) => (query ? b.s - a.s : 0))
      .map((x) => x.b),
  );

  async function save() {
    if (!editing) return;
    if (!/^https?:\/\//.test(editing.url)) return ui.toast('Bookmarks need an http(s) address', { icon: 'alert' });
    await bookmarks.update(editing);
    editing = null;
  }

  async function doImport() {
    try {
      const n = await api.bookmarksImport();
      if (n) ui.toast(`Imported ${n} bookmarks`, { icon: 'bookmark' });
      await bookmarks.load(true);
    } catch (e) {
      ui.toast(`Import failed: ${e}`, { icon: 'alert' });
    }
  }

  async function doExport() {
    try {
      const path = await api.bookmarksExport();
      if (path) ui.toast('Bookmarks exported', { icon: 'check', action: { label: 'Show', run: () => void api.showInFolder(path) } });
    } catch (e) {
      ui.toast(`Export failed: ${e}`, { icon: 'alert' });
    }
  }
</script>

<div class="page">
  <div class="inner">
    <header>
      <h1>Bookmarks</h1>
      <div class="tools">
        <button class="btn" onclick={doImport}><Icon name="arrowDownToLine" size={15} />Import</button>
        <button class="btn" onclick={doExport}><Icon name="save" size={15} />Export</button>
      </div>
    </header>
    <div class="search">
      <Icon name="search" size={16} />
      <input placeholder="Search bookmarks" bind:value={query} spellcheck="false" />
    </div>
    {#if bookmarks.folders.length}
      <div class="folders">
        <button class="chip" class:on={folder === null} onclick={() => (folder = null)}>All</button>
        <button class="chip" class:on={folder === ''} onclick={() => (folder = '')}>Unsorted</button>
        {#each bookmarks.folders as f (f)}
          <button class="chip" class:on={folder === f} onclick={() => (folder = f)}><Icon name="folder" size={13} />{f}</button>
        {/each}
      </div>
    {/if}

    <div class="list">
      {#each visible as b (b.id)}
        {#if editing?.id === b.id}
          <form
            class="edit"
            onsubmit={(e) => {
              e.preventDefault();
              void save();
            }}
          >
            <input class="field" bind:value={editing.title} placeholder="Name" />
            <input class="field" bind:value={editing.url} placeholder="Address" />
            <input class="field" bind:value={editing.folder} placeholder="Folder (optional)" list="bm-folders" />
            <datalist id="bm-folders">{#each bookmarks.folders as f (f)}<option value={f}></option>{/each}</datalist>
            <div class="edit-actions">
              <button type="button" class="btn ghost" onclick={() => (editing = null)}>Cancel</button>
              <button type="submit" class="btn primary">Save</button>
            </div>
          </form>
        {:else}
          <div class="row">
            <button class="link" title={b.url} onclick={(e) => browser.openTab(b.url, { activate: !(e.ctrlKey || e.metaKey) })}>
              <span class="star"><Icon name="star" size={14} fill /></span>
              <span class="title ellipsis">{b.title || prettyUrl(b.url)}</span>
              <span class="host ellipsis">{displayHost(b.url)}</span>
              {#if b.folder && folder === null}<span class="folder"><Icon name="folder" size={12} />{b.folder}</span>{/if}
            </button>
            <button class="icon-btn sm act" title="Edit" onclick={() => (editing = { ...b })}><Icon name="edit" size={14} /></button>
            <button class="icon-btn sm act" title="Delete" onclick={() => bookmarks.remove({ id: b.id })}><Icon name="trash" size={14} /></button>
          </div>
        {/if}
      {:else}
        <div class="empty">
          <Icon name="star" size={28} />
          <p>{query ? 'No matching bookmarks' : 'Press Ctrl+D on any page to bookmark it'}</p>
        </div>
      {/each}
    </div>
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
    margin: 18px 0 10px;
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
  .folders {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 10px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 11px;
    border-radius: 14px;
    background: var(--field);
    border: 1px solid var(--line);
    font-size: 12.5px;
    color: var(--text-2);
  }
  .chip.on {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    border-color: color-mix(in srgb, var(--accent) 50%, transparent);
    color: var(--text);
  }
  .list {
    display: flex;
    flex-direction: column;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 42px;
    padding: 0 8px;
    border-radius: var(--radius);
  }
  .row:hover {
    background: var(--hover);
  }
  .link {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
  }
  .star {
    color: var(--warn);
    display: grid;
  }
  .title {
    max-width: 55%;
  }
  .host {
    font-size: 12px;
    color: var(--text-3);
  }
  .folder {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    color: var(--text-3);
    margin-left: auto;
  }
  .act {
    opacity: 0;
  }
  .row:hover .act {
    opacity: 1;
  }
  .edit {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    padding: 12px;
    margin: 4px 0;
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line-strong);
  }
  .edit-actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
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
