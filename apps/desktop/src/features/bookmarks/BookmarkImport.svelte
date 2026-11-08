<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { bookmarksApi } from './api';
  import { webviewApi } from '$platform/webview-api';
  import { focusAfterLayout, overlayLease } from '$platform/layout.svelte';
  import { plural } from '$features/onboarding/onboarding-flow';
  import { importBookmarksFile } from './bookmark-actions';
  import type { ImportSource } from './types';

  // "Import bookmarks…": from a browser found on this PC, or from an exported HTML file.
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

  const open = $derived(ui.bookmarkImport);
  let sources = $state<ImportSource[] | null>(null);
  let chosen = $state('');
  let wantBookmarks = $state(true);
  let wantHistory = $state(false);
  let busy = $state(false);
  let result = $state('');
  let errors = $state<string[]>([]);

  const source = $derived(sources?.find((s) => s.id === chosen));
  const canHistory = $derived(!!source?.history && settings.value.recordHistory);

  $effect(() => {
    if (!open) return;
    sources = null;
    result = '';
    errors = [];
    wantBookmarks = true;
    wantHistory = false;
    void webviewApi.focusView(null);
    void bookmarksApi
      .importSources()
      .catch(() => [] as ImportSource[])
      .then((list) => {
        sources = list;
        chosen = list[0]?.id ?? '';
      });
  });

  /** Profile names only matter when a browser has more than one. */
  function profileOf(src: ImportSource) {
    return (sources ?? []).filter((x) => x.browser === src.browser).length > 1 ? src.profile : '';
  }

  function close() {
    ui.bookmarkImport = false;
    focusAfterLayout(browser.activeTab?.live ? browser.activeTab.id : null);
  }

  async function run() {
    if (!source || busy) return;
    busy = true;
    result = '';
    errors = [];
    try {
      const r = await bookmarksApi.importRun(
        source.id,
        wantBookmarks && source.bookmarks,
        wantHistory && canHistory,
        settings.value.historyDays,
      );
      await bookmarks.load(true);
      const parts: string[] = [];
      if (wantBookmarks && source.bookmarks) parts.push(plural(r.bookmarks, 'new bookmark'));
      if (wantHistory && canHistory) parts.push(plural(r.history, 'history entry', 'history entries'));
      result = parts.length ? `Imported ${parts.join(' and ')} from ${source.name}.` : '';
      errors = r.errors;
    } catch (e) {
      errors = [String(e)];
    } finally {
      busy = false;
    }
  }

  async function fromFile() {
    close();
    await importBookmarksFile();
  }
</script>

{#if open}
  <div class="root" role="presentation" use:overlayLease>
    <div class="scrim" role="presentation" transition:fade={{ duration: 120 }} onpointerdown={close}></div>
    <div
      class="dialog glass"
      role="dialog"
      aria-modal="true"
      aria-label="Import bookmarks"
      tabindex="-1"
      onkeydown={(e) => {
        if (e.key === 'Escape') {
          e.stopPropagation();
          close();
        }
      }}
      transition:scale={{ start: 0.97, duration: 140 }}
    >
      <header>
        <h2>Import bookmarks</h2>
        <button class="icon-btn sm" title="Close" onclick={close}><Icon name="close" size={14} /></button>
      </header>

      <div class="body">
        {#if sources === null}
          <p class="note">Looking for other browsers…</p>
        {:else if sources.length}
          <fieldset>
            <legend>Import from a browser</legend>
            <div class="sources">
              {#each sources as src (src.id)}
                {@const tile = TILES[src.browser] ?? ['#777', src.name[0]]}
                <label class="source" class:on={chosen === src.id}>
                  <input type="radio" name="bm-import-source" value={src.id} bind:group={chosen} />
                  <span class="tile" style:--c={tile[0]} aria-hidden="true">{tile[1]}</span>
                  <span class="name">
                    {src.name}
                    {#if profileOf(src)}<small>{profileOf(src)}</small>{/if}
                  </span>
                </label>
              {/each}
            </div>
          </fieldset>
          <div class="what">
            <label class:off={!source?.bookmarks}>
              <input type="checkbox" bind:checked={wantBookmarks} disabled={!source?.bookmarks} />
              <span><b>Bookmarks</b><small>{source?.bookmarks ? 'Into a folder named after the browser, on the toolbar' : 'None found'}</small></span>
            </label>
            <label class:off={!canHistory}>
              <input type="checkbox" bind:checked={wantHistory} disabled={!canHistory} />
              <span><b>History</b><small>{canHistory ? 'Pages visited lately' : settings.value.recordHistory ? 'None found' : 'History is turned off'}</small></span>
            </label>
          </div>
          <div class="go">
            <button class="btn primary" disabled={busy || (!wantBookmarks && !wantHistory)} aria-busy={busy} onclick={run}>
              {busy ? 'Importing…' : 'Import'}
            </button>
          </div>
        {:else}
          <p class="note">No other browsers with bookmarks were found on this PC.</p>
        {/if}

        {#if result}<p class="ok" role="status">{result}</p>{/if}
        {#each errors as err (err)}<p class="error" role="alert">{err}</p>{/each}

        <div class="file">
          <div>
            <b>From a file</b>
            <small>An exported bookmarks file (.html) from Firefox, Chrome, Edge or another browser.</small>
          </div>
          <button class="btn" onclick={fromFile}><Icon name="arrowDownToLine" size={15} />Choose file…</button>
        </div>
      </div>

      <footer><button class="btn" onclick={close}>Done</button></footer>
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 88;
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .scrim {
    position: absolute;
    inset: 0;
    background: var(--backdrop);
  }
  .dialog {
    position: relative;
    width: min(500px, 100%);
    max-height: 100%;
    display: flex;
    flex-direction: column;
    border-radius: 16px;
    corner-shape: var(--sq);
    background: var(--dialog-bg);
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 16px 4px 20px;
  }
  h2 {
    margin: 0;
    font: 600 16px/1.3 var(--font-display);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 10px 20px 4px;
    overflow-y: auto;
  }
  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }
  legend {
    margin-bottom: 8px;
    padding: 0;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-2);
  }
  .sources {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .source {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 0 10px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .source:hover {
    background: var(--field-focus);
  }
  .source.on {
    border-color: color-mix(in srgb, var(--accent) 70%, transparent);
    background: color-mix(in srgb, var(--accent) 14%, var(--field));
  }
  .source input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .tile {
    width: 26px;
    height: 26px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 7px;
    font-size: 12px;
    font-weight: 700;
    color: #fff;
    background: var(--c);
  }
  .name {
    min-width: 0;
    display: flex;
    flex-direction: column;
    font-size: 13px;
  }
  .name small {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .what label {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .what label.off {
    opacity: 0.5;
  }
  .what input {
    margin-top: 3px;
    accent-color: var(--accent);
  }
  .what span {
    display: flex;
    flex-direction: column;
    font-size: 13px;
  }
  small {
    color: var(--text-3);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .go {
    display: flex;
    justify-content: flex-end;
  }
  .note {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
  .ok {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--r-btn);
    font-size: 12.5px;
    color: var(--ok);
    background: color-mix(in srgb, var(--ok) 12%, transparent);
  }
  .error {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--r-btn);
    font-size: 12.5px;
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }
  .file {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
    font-size: 13px;
  }
  .file div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    padding: 12px 20px 18px;
  }
</style>
