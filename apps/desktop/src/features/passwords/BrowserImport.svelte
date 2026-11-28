<script lang="ts">
  // "Import from a browser…": saved passwords and payment cards of a Chromium browser on this PC.
  // Cards are added right away; passwords go to the same preview as a CSV import.
  import Modal from './Modal.svelte';
  import { TILES } from '$features/bookmarks/BookmarkImport.svelte';
  import { bookmarksApi } from '$features/bookmarks/api';
  import type { ImportSource } from '$features/bookmarks/types';
  import type { ImportOpened } from './passwords';
  import { ui } from '$shared/ui/ui.svelte';
  import { vault, vaultApi } from './vault.svelte';

  let { onclose, onopened }: { onclose: () => void; onopened: (o: ImportOpened) => void } = $props();

  let sources = $state<ImportSource[] | null>(null);
  let chosen = $state('');
  let wantPasswords = $state(true);
  let wantCards = $state(true);
  let busy = $state(false);
  let errors = $state<string[]>([]);

  void bookmarksApi
    .importSources()
    .catch(() => [] as ImportSource[])
    .then((list) => {
      sources = list.filter((s) => s.passwords);
      chosen = sources[0]?.id ?? '';
    });

  function profileOf(src: ImportSource) {
    return (sources ?? []).filter((x) => x.browser === src.browser).length > 1 ? src.profile : '';
  }

  async function run() {
    if (!chosen || busy) return;
    busy = true;
    errors = [];
    try {
      const r = await vaultApi.importBrowser(chosen, wantPasswords, wantCards);
      if (r.cards) {
        vault.revision++;
        ui.toast(`${r.cards} ${r.cards === 1 ? 'card' : 'cards'} added to the wallet`, { icon: 'check', timeout: 5000 });
      }
      if (r.passwords) {
        if (r.errors.length) ui.toast(r.errors.join(' '), { icon: 'alert', timeout: 12000 });
        onopened(r.passwords);
        return;
      }
      errors = r.errors.length ? r.errors : r.cards ? [] : ['Nothing new to import.'];
      if (!errors.length) onclose();
    } catch (e) {
      errors = [String(e)];
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Import from a browser" {onclose} width={480}>
  {#if sources === null}
    <p class="muted">Looking for other browsers…</p>
  {:else if sources.length === 0}
    <p class="muted">No Chromium browser with saved passwords was found. Export your passwords as a CSV file and use Import passwords instead.</p>
  {:else}
    <div class="sources">
      {#each sources as src (src.id)}
        {@const tile = TILES[src.browser] ?? ['#777', src.name[0]]}
        <label class="source" class:on={chosen === src.id}>
          <input type="radio" name="pw-import-source" value={src.id} bind:group={chosen} />
          <span class="tile" style:--c={tile[0]} aria-hidden="true">{tile[1]}</span>
          <span class="name">
            {src.name}
            {#if profileOf(src)}<small>{profileOf(src)}</small>{/if}
          </span>
        </label>
      {/each}
    </div>
    <div class="what">
      <label><input type="checkbox" bind:checked={wantPasswords} /> Passwords <small>(you choose which ones next)</small></label>
      <label><input type="checkbox" bind:checked={wantCards} /> Payment cards <small>(cards you already saved are kept)</small></label>
    </div>
  {/if}
  {#each errors as e (e)}<p class="err">{e}</p>{/each}
  <div class="actions">
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={busy || !chosen || (!wantPasswords && !wantCards)} onclick={run}>
      {busy ? 'Reading…' : 'Import'}
    </button>
  </div>
</Modal>

<style>
  .sources {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 8px;
  }
  .source {
    position: relative;
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
  .name small,
  .what small {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 14px;
  }
  .what input {
    accent-color: var(--accent);
  }
  .err {
    margin: 10px 0 0;
    color: var(--warn);
    font-size: 12.5px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
</style>
