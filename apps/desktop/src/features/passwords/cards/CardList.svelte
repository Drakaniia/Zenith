<script lang="ts">
  // Saved cards (unlocked vault): search, add, edit, delete. Showing or copying a number or
  // security code asks for the master password every time.
  import { onDestroy } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Modal from '../Modal.svelte';
  import MasterPasswordForm from '../MasterPasswordForm.svelte';
  import CardEditor from './CardEditor.svelte';
  import { brandName, filterCards, formatExpiry, groupNumber, isExpired, type SavedCard } from './cards';
  import { needsMaster } from '../passwords';
  import { timeAgo, ui } from '$shared/ui/ui.svelte';
  import { vault, vaultApi } from '../vault.svelte';

  let cards = $state<SavedCard[]>([]);
  let loading = $state(true);
  let query = $state('');
  /** Revealed numbers and codes by card id (hidden again after 30 seconds or on lock). */
  let shown = $state<Record<string, { number: string; cvc: string }>>({});
  let editor = $state<{ card: SavedCard | null } | null>(null);
  let removing = $state<SavedCard | null>(null);
  /** An action waiting for the master password. */
  let ask = $state<{ title: string; label: string; run: (master: string) => Promise<void> } | null>(null);
  const timers = new Map<string, ReturnType<typeof setTimeout>>();

  const visible = $derived(filterCards(cards, query));

  async function load() {
    try {
      cards = await vaultApi.cards();
    } catch (e) {
      if (!needsMaster(e)) ui.toast(String(e), { icon: 'alert' });
      cards = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void vault.revision;
    void load();
  });

  // Locking forgets everything shown.
  $effect(() => {
    if (!vault.status.unlocked) hideAll();
  });

  function hideAll() {
    for (const t of timers.values()) clearTimeout(t);
    timers.clear();
    shown = {};
  }
  onDestroy(hideAll);

  function hide(id: string) {
    clearTimeout(timers.get(id));
    timers.delete(id);
    const { [id]: _, ...rest } = shown;
    shown = rest;
  }

  /** Runs a vault call that needs the master password typed for it. */
  function withMaster<T>(title: string, label: string, call: (master: string) => Promise<T>): Promise<T | undefined> {
    return new Promise<T | undefined>((resolve) => {
      ask = {
        title,
        label,
        run: async (master) => {
          const r = await call(master);
          ask = null;
          resolve(r);
        },
      };
    });
  }

  async function toggleReveal(c: SavedCard) {
    if (shown[c.id]) return hide(c.id);
    const r = await withMaster('Show card', 'Show', (m) => vaultApi.cardReveal(c.id, m));
    if (!r) return;
    shown = { ...shown, [c.id]: r };
    timers.set(c.id, setTimeout(() => hide(c.id), 30_000));
  }

  async function copy(c: SavedCard, field: 'number' | 'cvc' | 'holder') {
    if (field === 'holder') {
      try {
        await vaultApi.cardCopy(c.id, field);
        ui.toast('Name copied', { icon: 'copy' });
      } catch (e) {
        ui.toast(String(e), { icon: 'alert' });
      }
      return;
    }
    const ok = await withMaster(field === 'number' ? 'Copy card number' : 'Copy security code', 'Copy', (m) =>
      vaultApi.cardCopy(c.id, field, m).then(() => true),
    );
    if (ok) ui.toast(`${field === 'number' ? 'Card number' : 'Security code'} copied. The clipboard is cleared in 30 seconds.`, { icon: 'copy' });
  }

  async function remove() {
    const c = removing;
    removing = null;
    if (!c) return;
    try {
      await vaultApi.cardsRemove([c.id]);
      hide(c.id);
      ui.toast(`${brandName(c.brand)} •••• ${c.last4} deleted`, { icon: 'trash' });
      vault.revision++;
      void vault.refresh();
    } catch (err) {
      ui.toast(String(err), { icon: 'alert' });
    }
  }

  function rowMenu(ev: MouseEvent, c: SavedCard) {
    ev.preventDefault();
    ui.openMenu(ev.clientX, ev.clientY, [
      { label: 'Copy card number', icon: 'copy', run: () => void copy(c, 'number') },
      ...(c.hasCvc ? [{ label: 'Copy security code', icon: 'copy' as const, run: () => void copy(c, 'cvc') }] : []),
      ...(c.holder ? [{ label: 'Copy name', icon: 'user' as const, run: () => void copy(c, 'holder') }] : []),
      { label: shown[c.id] ? 'Hide card' : 'Show card', icon: 'eye', run: () => void toggleReveal(c) },
      { separator: true },
      { label: 'Edit…', icon: 'edit', run: () => (editor = { card: c }) },
      { label: 'Delete', icon: 'trash', danger: true, run: () => (removing = c) },
    ]);
  }
</script>

<div class="list-page">
  <div class="toolbar">
    <div class="search">
      <Icon name="search" size={15} />
      <input bind:value={query} placeholder="Search cards" aria-label="Search cards" spellcheck="false" />
      {#if query}<button class="icon-btn sm" title="Clear" onclick={() => (query = '')}><Icon name="close" size={13} /></button>{/if}
    </div>
    <button class="btn primary" onclick={() => (editor = { card: null })}><Icon name="plus" size={15} /> Add card</button>
  </div>

  {#if !loading}
    <p class="count">
      {cards.length === 0 ? 'No saved cards yet' : `${cards.length} ${cards.length === 1 ? 'card' : 'cards'}`}
      {#if query && cards.length}· {visible.length} shown{/if}
    </p>
  {/if}

  {#if !loading && cards.length === 0}
    <div class="empty">
      <div class="badge"><Icon name="card" size={24} /></div>
      <h3>Your cards will appear here</h3>
      <p>
        Pay on a site and Zenith offers to save the card. You can also add one yourself. Cards are encrypted with your master password, which
        is asked for every time one is filled in or shown.
      </p>
      <button class="btn primary" onclick={() => (editor = { card: null })}><Icon name="plus" size={15} /> Add card</button>
    </div>
  {:else if !loading && visible.length === 0}
    <p class="none">Nothing matches “{query}”.</p>
  {:else}
    <div class="cards">
      {#each visible as c (c.id)}
        <div class="card" role="row" tabindex="-1" oncontextmenu={(ev) => rowMenu(ev, c)}>
          <div class="brand" data-brand={c.brand}><Icon name="card" size={18} /></div>
          <div class="who">
            <span class="title ellipsis">
              {#if shown[c.id]}<span class="mono">{groupNumber(shown[c.id].number, c.brand)}</span>{:else}{brandName(c.brand)} •••• {c.last4}{/if}
              {#if c.label}<span class="chip">{c.label}</span>{/if}
            </span>
            <span class="meta ellipsis">
              {c.holder ? c.holder + ' · ' : ''}<span class:expired={isExpired(c.expMonth, c.expYear)}
                >{isExpired(c.expMonth, c.expYear) ? 'Expired ' : 'Expires '}{formatExpiry(c.expMonth, c.expYear)}</span
              >
              · {c.lastUsed ? `Used ${timeAgo(c.lastUsed)}` : `Saved ${timeAgo(c.created)}`}
              {#if shown[c.id]}· {shown[c.id].cvc ? `Code ${shown[c.id].cvc}` : 'No code saved'}{:else if c.hasCvc}· code saved{/if}
            </span>
          </div>
          <div class="acts">
            <button class="icon-btn sm" title={shown[c.id] ? 'Hide card' : 'Show card'} onclick={() => toggleReveal(c)}>
              <Icon name={shown[c.id] ? 'eyeOff' : 'eye'} size={15} />
            </button>
            <button class="icon-btn sm" title="Copy card number" onclick={() => copy(c, 'number')}><Icon name="copy" size={15} /></button>
            <button class="icon-btn sm" title="Edit" onclick={() => (editor = { card: c })}><Icon name="edit" size={15} /></button>
            <button class="icon-btn sm del" title="Delete" onclick={() => (removing = c)}><Icon name="trash" size={15} /></button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if editor}
  <CardEditor
    card={editor.card}
    onclose={() => (editor = null)}
    onsaved={() => {
      const added = !editor?.card;
      if (editor?.card) hide(editor.card.id);
      editor = null;
      vault.revision++;
      void vault.refresh();
      ui.toast(added ? 'Card saved' : 'Card updated', { icon: 'check' });
    }}
  />
{/if}

{#if removing}
  <Modal title="Delete card?" onclose={() => (removing = null)} width={400}>
    <p class="confirm">
      <b>{brandName(removing.brand)} •••• {removing.last4}</b> will be deleted from Zenith. This can't be undone.
    </p>
    <div class="actions">
      <button class="btn" onclick={() => (removing = null)}>Cancel</button>
      <button class="btn danger" onclick={remove}><Icon name="trash" size={14} /> Delete</button>
    </div>
  </Modal>
{/if}

{#if ask}
  <Modal title={ask.title} onclose={() => (ask = null)} width={420}>
    <div class="ask">
      <p>Enter your master password to continue. Cards ask for it every time.</p>
      <MasterPasswordForm onsubmit={ask.run} submitLabel={ask.label} />
    </div>
  </Modal>
{/if}

<style>
  .list-page {
    max-width: 860px;
    padding: 28px 36px 80px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 8px 0 12px;
    border-radius: var(--radius);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-3);
  }
  .search:focus-within {
    background: var(--field-focus);
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--text);
    font: inherit;
  }
  .count {
    margin: 14px 2px 10px;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .none {
    color: var(--text-3);
    padding: 20px 2px;
  }
  .cards {
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .card {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 14px;
    min-height: 60px;
    padding: 8px 10px 8px 14px;
  }
  .card + .card {
    border-top: 1px solid var(--line);
  }
  .card:hover {
    background: var(--hover);
  }
  .brand {
    width: 40px;
    height: 28px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--accent-fg);
    background: color-mix(in srgb, var(--accent) 20%, transparent);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 500;
  }
  .chip {
    padding: 0 6px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 400;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent-fg);
  }
  .meta {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .expired {
    color: var(--danger);
  }
  .mono {
    font-family: var(--mono);
    user-select: text;
  }
  .acts {
    display: flex;
    gap: 2px;
    opacity: 0.55;
    transition: opacity var(--fast) ease;
  }
  .card:hover .acts,
  .card:focus-within .acts {
    opacity: 1;
  }
  .del:hover {
    color: var(--danger);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 28px;
    margin-top: 8px;
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .empty .badge {
    width: 48px;
    height: 48px;
    border-radius: 14px;
    display: grid;
    place-items: center;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .empty h3 {
    margin: 4px 0 0;
    font: 600 16px/1.3 var(--font-display);
  }
  .empty p {
    margin: 0;
    color: var(--text-2);
    line-height: 1.5;
    max-width: 560px;
  }
  .confirm {
    margin: 0 0 16px;
    line-height: 1.5;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .ask {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .ask p {
    margin: 0;
    line-height: 1.5;
    color: var(--text-2);
  }
</style>
