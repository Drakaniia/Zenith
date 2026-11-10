<!-- The downloads page's "Add torrent" bar: paste a magnet link, or open a .torrent file. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { torrents } from './torrents.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let link = $state('');
  let busy = $state(false);
  let input: HTMLInputElement | undefined = $state();

  onMount(() => input?.focus());

  async function add() {
    if (!link.trim() || busy) return;
    busy = true;
    const ok = await torrents.addMagnet(link);
    busy = false;
    if (ok) {
      link = '';
      onclose();
    }
  }

  async function open() {
    busy = true;
    await torrents.openFile();
    busy = false;
  }
</script>

<form
  class="bar"
  onsubmit={(e) => {
    e.preventDefault();
    void add();
  }}
>
  <Icon name="magnet" size={16} />
  <input
    bind:this={input}
    bind:value={link}
    placeholder="Paste a magnet link"
    spellcheck="false"
    aria-label="Magnet link"
    onkeydown={(e) => {
      if (e.key === 'Escape') onclose();
    }}
  />
  <button class="btn primary" type="submit" disabled={busy || !link.trim()}>Add</button>
  <span class="or">or</span>
  <button class="btn" type="button" disabled={busy} onclick={() => void open()}><Icon name="folder" size={15} />Open .torrent file…</button>
  <button class="icon-btn sm" type="button" title="Close" onclick={onclose}><Icon name="close" size={14} /></button>
</form>
<p class="hint">You can also drop .torrent files or magnet links on this page. Magnet links you click on pages download here too.</p>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    padding: 6px 6px 6px 14px;
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-3);
  }
  .bar input {
    flex: 1;
    min-width: 0;
    height: 32px;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 13.5px;
  }
  .or {
    font-size: 12px;
  }
  .hint {
    margin: 6px 4px 0;
    font-size: 12px;
    color: var(--text-3);
  }
</style>
