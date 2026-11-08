<script lang="ts">
  import { untrack } from 'svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { formatTags, kids, parseTags } from './bookmarks';
  import { openAll } from './bookmark-actions';
  import { isWebUrl } from '$shared/url';
  import type { BookmarkNode, BookmarkPatch } from './types';

  // The Library's details pane: the chosen bookmark's Name, URL, Tags and Keyword, edited in place.

  let {
    one,
    picked,
  }: {
    /** The one selected node, if exactly one is selected. */
    one: BookmarkNode | undefined;
    picked: BookmarkNode[];
  } = $props();

  const ix = $derived(bookmarks.index);

  let dName = $state('');
  let dUrl = $state('');
  let dTags = $state('');
  let dKeyword = $state('');
  const signature = $derived(one ? [one.id, one.title, one.url, one.tags.join(','), one.keyword].join('\u0001') : '');
  $effect(() => {
    void signature;
    untrack(() => {
      dName = one?.title ?? '';
      dUrl = one?.url ?? '';
      dTags = formatTags(one?.tags ?? []);
      dKeyword = one?.keyword ?? '';
    });
  });

  async function commit(field: 'title' | 'url' | 'tags' | 'keyword') {
    const n = one;
    if (!n) return;
    const patch: BookmarkPatch = {};
    if (field === 'title' && dName.trim() !== n.title) patch.title = dName.trim();
    if (field === 'url' && n.kind === 'bookmark' && dUrl.trim() !== n.url) {
      if (!isWebUrl(dUrl.trim())) {
        ui.toast('Bookmarks need an http(s) address', { icon: 'alert' });
        dUrl = n.url;
        return;
      }
      patch.url = dUrl.trim();
    }
    if (field === 'tags') {
      const tags = parseTags(dTags);
      if (formatTags(tags) !== formatTags(n.tags)) patch.tags = tags;
    }
    if (field === 'keyword') {
      const word = dKeyword.trim().toLowerCase();
      if (/\s/.test(word)) {
        ui.toast("A keyword can't contain spaces", { icon: 'alert' });
        dKeyword = n.keyword;
        return;
      }
      if (word !== n.keyword) patch.keyword = word;
    }
    if (!Object.keys(patch).length) return;
    try {
      await bookmarks.update(n.id, patch);
    } catch (e) {
      ui.toast(String(e), { icon: 'alert' });
      dName = n.title;
      dUrl = n.url;
      dTags = formatTags(n.tags);
      dKeyword = n.keyword;
    }
  }

  const enterBlurs = (e: KeyboardEvent) => {
    if (e.key === 'Enter') (e.currentTarget as HTMLElement).blur();
  };
</script>

<div class="details" class:idle={!one || one.kind === 'separator'}>
  {#if one && one.kind !== 'separator'}
    <div class="drow">
      <label for="lib-name">Name</label>
      <input id="lib-name" class="field" bind:value={dName} spellcheck="false" onchange={() => void commit('title')} onkeydown={enterBlurs} disabled={one.parent === null} />
    </div>
    {#if one.kind === 'bookmark'}
      <div class="drow">
        <label for="lib-url">URL</label>
        <input id="lib-url" class="field" bind:value={dUrl} spellcheck="false" onchange={() => void commit('url')} onkeydown={enterBlurs} />
      </div>
      <div class="drow top">
        <label for="lib-tags">Tags</label>
        <div class="stack">
          <input id="lib-tags" class="field" bind:value={dTags} placeholder="Separate tags with commas" spellcheck="false" onchange={() => void commit('tags')} onkeydown={enterBlurs} />
          <small>Use tags to organize and search for bookmarks from the address bar</small>
        </div>
      </div>
      <div class="drow top">
        <label for="lib-kw">Keyword</label>
        <div class="stack">
          <input id="lib-kw" class="field" bind:value={dKeyword} spellcheck="false" onchange={() => void commit('keyword')} onkeydown={enterBlurs} />
          <small>Use a single keyword to open bookmarks directly from the address bar</small>
        </div>
      </div>
    {:else}
      <p class="count">
        {kids(ix, one.id).length} {kids(ix, one.id).length === 1 ? 'item' : 'items'}
        {#if kids(ix, one.id).some((k) => k.kind === 'bookmark')}
          <button class="link" onclick={() => openAll(one)}>Open all in tabs</button>
        {/if}
      </p>
    {/if}
  {:else if picked.length > 1}
    <p class="count">{picked.length} items selected</p>
  {:else}
    <p class="count">Select a bookmark to see and change its details.</p>
  {/if}
</div>

<style>
  .details {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px 16px;
    border-top: 1px solid var(--line-strong);
    background: color-mix(in srgb, var(--accent) 5%, var(--surface));
    user-select: text;
  }
  .details.idle {
    padding: 14px 16px;
  }
  .drow {
    display: grid;
    grid-template-columns: 70px minmax(0, 1fr);
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }
  .drow.top {
    align-items: start;
  }
  .drow.top label {
    padding-top: 7px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  small {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .count {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .link {
    color: var(--accent-fg);
    text-decoration: underline;
  }
</style>
