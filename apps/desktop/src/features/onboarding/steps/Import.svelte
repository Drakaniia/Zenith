<script lang="ts">
  import { onMount } from 'svelte';
  import { onboarding } from '../onboarding.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { bookmarks as bookmarkStore } from '$features/bookmarks/bookmarks.svelte';
  import { bookmarksApi } from '$features/bookmarks/api';
  import { plural } from '../onboarding-flow';
  import type { ImportSource } from '$features/bookmarks/types';

  /** Brand-ish colours for the letter tiles (no logos are bundled). */
  const TILES: Record<string, [string, string]> = {
    chrome: ['#1a73e8', 'C'],
    edge: ['#0c7bd4', 'E'],
    brave: ['#fb542b', 'B'],
    vivaldi: ['#ef3939', 'V'],
    chromium: ['#5f6368', 'C'],
    opera: ['#ff1b2d', 'O'],
    operagx: ['#fa1e4e', 'GX'],
    firefox: ['#ff7139', 'F'],
    zen: ['#f76f53', 'Z'],
  };

  let sources = $state<ImportSource[] | null>(null);
  let chosen = $state('');
  let wantBookmarks = $state(true);
  let wantHistory = $state(settings.value.recordHistory);
  let busy = $state(false);
  let status = $state<{ text: string; errors: string[] } | null>(null);
  let done = $state<string[]>([]);

  const source = $derived(sources?.find((x) => x.id === chosen));
  const days = $derived(settings.value.historyDays);
  const span = $derived(
    days === 0 ? 'all of it' : days >= 365 ? 'the last year' : days >= 90 ? 'the last 3 months' : days >= 30 ? 'the last month' : 'the last week',
  );

  onMount(async () => {
    sources = await bookmarksApi.importSources().catch(() => []);
    chosen = sources[0]?.id ?? '';
  });

  /** Profile names only matter when a browser has more than one. */
  function profileOf(src: ImportSource) {
    const many = (sources ?? []).filter((x) => x.browser === src.browser).length > 1;
    return many ? src.profile : '';
  }

  function record(from: string, b: number, h: number) {
    const sum = onboarding.imported;
    sum.bookmarks += b;
    sum.history += h;
    if (!sum.from.includes(from)) sum.from.push(from);
    void bookmarkStore.load(true);
  }

  async function run() {
    if (!source || busy) return;
    busy = true;
    status = null;
    try {
      const r = await bookmarksApi.importRun(
        source.id,
        wantBookmarks && source.bookmarks,
        wantHistory && source.history && settings.value.recordHistory,
        days,
      );
      const parts = [];
      if (wantBookmarks && source.bookmarks) parts.push(plural(r.bookmarks, 'new bookmark'));
      if (wantHistory && source.history) parts.push(plural(r.history, 'history entry', 'history entries'));
      status = { text: parts.length ? `Imported ${parts.join(' and ')} from ${source.name}.` : '', errors: r.errors };
      record(source.name, r.bookmarks, r.history);
      if (!r.errors.length) done.push(source.id);
    } catch (e) {
      status = { text: '', errors: [String(e)] };
    } finally {
      busy = false;
    }
  }

  async function fromFile() {
    try {
      const n = await bookmarksApi.bookmarksImport();
      if (n > 0) {
        status = { text: `Imported ${plural(n, 'bookmark')} from the file.`, errors: [] };
        record('a bookmarks file', n, 0);
      }
    } catch (e) {
      status = { text: '', errors: [String(e)] };
    }
  }
</script>

{#if sources === null}
  <p class="searching" role="status"><span class="spinner" aria-hidden="true"></span>Looking for other browsers…</p>
{:else if sources.length}
  <fieldset>
    <legend>Import from</legend>
    <div class="sources">
      {#each sources as src (src.id)}
        {@const tile = TILES[src.browser] ?? ['#777', src.name[0]]}
        <label class="source" class:on={chosen === src.id}>
          <input type="radio" name="fre-source" value={src.id} bind:group={chosen} />
          <span class="tile" style:--c={tile[0]} aria-hidden="true">{tile[1]}</span>
          <span class="name">
            {src.name}
            {#if profileOf(src)}<small>{profileOf(src)}</small>{/if}
          </span>
          {#if done.includes(src.id)}<span class="badge">Imported</span>{/if}
        </label>
      {/each}
    </div>
  </fieldset>

  {#if source}
    <fieldset>
      <legend>What to bring</legend>
      <div class="checks">
        <label class:off={!source.bookmarks}>
          <input type="checkbox" bind:checked={wantBookmarks} disabled={!source.bookmarks} />
          <span><b>Bookmarks</b><small>{source.bookmarks ? 'Into a folder named after the browser' : 'None found'}</small></span>
        </label>
        <label class:off={!source.history || !settings.value.recordHistory}>
          <input type="checkbox" bind:checked={wantHistory} disabled={!source.history || !settings.value.recordHistory} />
          <span>
            <b>History</b>
            <small>{!settings.value.recordHistory ? 'History is turned off' : source.history ? `From ${span}` : 'None found'}</small>
          </span>
        </label>
      </div>
      <div class="act">
        <button type="button" class="btn" disabled={busy || (!wantBookmarks && !wantHistory)} aria-busy={busy} onclick={run}>
          {#if busy}<span class="spinner" aria-hidden="true"></span>Importing…{:else}Import from {source.name}{/if}
        </button>
      </div>
    </fieldset>
  {/if}
{:else}
  <p class="empty">No other browsers were found for this Windows account.</p>
{/if}

<div class="status" role="status" aria-live="polite">
  {#if status}
    {#if status.text}<p class="ok">{status.text}</p>{/if}
    {#each status.errors as err (err)}<p class="err">{err}</p>{/each}
  {/if}
</div>

<p class="file">
  Have an exported bookmarks file?
  <button type="button" class="link" onclick={fromFile}>Choose an .html file…</button>
</p>
<p class="note">Passwords, cookies and open tabs aren't imported. The other browser isn't changed.</p>

<style>
  .sources {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 8px;
  }
  .source {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 52px;
    padding: 8px 12px;
    border-radius: var(--radius-lg);
    corner-shape: var(--sq);
    background: color-mix(in srgb, var(--text) 3.5%, transparent);
    box-shadow: inset 0 0 0 1px var(--line);
    transition:
      background-color 150ms ease,
      box-shadow 150ms ease;
  }
  .source.on {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--accent) 80%, transparent);
  }
  .source:has(input:focus-visible) {
    outline: 2px solid color-mix(in srgb, var(--accent) 80%, transparent);
    outline-offset: 2px;
  }
  .source input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .tile {
    display: grid;
    place-items: center;
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    corner-shape: var(--sq);
    background: var(--c);
    color: #fff;
    font: 700 11.5px/1 var(--font-display);
  }
  .name {
    flex: 1;
    min-width: 0;
    display: grid;
    font-weight: 500;
    white-space: nowrap;
  }
  .name,
  .name small {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name small {
    font-weight: 400;
    font-size: 12px;
    color: var(--text-3);
  }
  .badge {
    font-size: 11px;
    color: var(--ok);
  }
  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 24px;
  }
  .checks label {
    display: flex;
    align-items: flex-start;
    gap: 9px;
  }
  .checks label.off {
    opacity: 0.55;
  }
  .checks input {
    margin: 3px 0 0;
    accent-color: var(--accent-button);
    width: 15px;
    height: 15px;
  }
  .checks span {
    display: grid;
  }
  .checks small {
    color: var(--text-3);
    font-size: 12px;
  }
  .act {
    margin-top: 14px;
  }
  .spinner {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    border: 2px solid color-mix(in srgb, currentColor 25%, transparent);
    border-top-color: currentColor;
    animation: spin 600ms linear infinite;
    flex: none;
  }
  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
  .searching {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-2);
  }
  .empty {
    color: var(--text-2);
    margin: 0 0 4px;
  }
  .status p {
    margin: 10px 0 0;
    font-size: 13px;
  }
  .status .ok {
    color: var(--ok);
  }
  .status .err {
    color: var(--danger);
  }
  .file {
    margin: 18px 0 0;
    font-size: 12.5px;
    color: var(--text-2);
  }
</style>
