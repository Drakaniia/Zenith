<script lang="ts">
  // Saved addresses (unlocked vault): add, edit, delete. Pages fill them from the list under an
  // address field.
  import Icon from '$shared/ui/Icon.svelte';
  import Modal from '../Modal.svelte';
  import AddressEditor from './AddressEditor.svelte';
  import { addressLine, addressTitle, type SavedAddress } from './addresses';
  import { needsMaster } from '../passwords';
  import { ui } from '$shared/ui/ui.svelte';
  import { vault, vaultApi } from '../vault.svelte';

  let list = $state<SavedAddress[]>([]);
  let loading = $state(true);
  let editor = $state<{ address: SavedAddress | null } | null>(null);
  let removing = $state<SavedAddress | null>(null);

  async function load() {
    try {
      list = await vaultApi.addresses();
    } catch (e) {
      if (!needsMaster(e)) ui.toast(String(e), { icon: 'alert' });
      list = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void vault.revision;
    void load();
  });

  async function remove() {
    const a = removing;
    removing = null;
    if (!a) return;
    try {
      await vaultApi.addressesRemove([a.id]);
      ui.toast(`${addressTitle(a)} deleted`, { icon: 'trash' });
      vault.revision++;
      void vault.refresh();
    } catch (err) {
      ui.toast(String(err), { icon: 'alert' });
    }
  }
</script>

<div class="list-page">
  <div class="toolbar">
    <h2>Addresses</h2>
    <button class="btn primary" onclick={() => (editor = { address: null })}><Icon name="plus" size={15} /> Add address</button>
  </div>

  {#if !loading && list.length === 0}
    <div class="empty">
      <div class="badge"><Icon name="idCard" size={24} /></div>
      <h3>Your addresses will appear here</h3>
      <p>
        Add an address, or import the ones saved in Chrome, Edge or another Chromium browser. Click a name, address or phone field on a
        page and Zenith offers to fill the whole form.
      </p>
      <button class="btn primary" onclick={() => (editor = { address: null })}><Icon name="plus" size={15} /> Add address</button>
    </div>
  {:else if !loading}
    <div class="rows">
      {#each list as a (a.id)}
        <div class="row">
          <div class="who">
            <span class="title ellipsis">{addressTitle(a)}</span>
            <span class="meta ellipsis">{[addressLine(a), a.phone, a.email].filter(Boolean).join(' · ')}</span>
          </div>
          <div class="acts">
            <button class="icon-btn sm" title="Edit" onclick={() => (editor = { address: a })}><Icon name="edit" size={15} /></button>
            <button class="icon-btn sm del" title="Delete" onclick={() => (removing = a)}><Icon name="trash" size={15} /></button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if editor}
  <AddressEditor
    address={editor.address}
    onclose={() => (editor = null)}
    onsaved={() => {
      const added = !editor?.address;
      editor = null;
      vault.revision++;
      void vault.refresh();
      ui.toast(added ? 'Address saved' : 'Address updated', { icon: 'check' });
    }}
  />
{/if}

{#if removing}
  <Modal title="Delete address?" onclose={() => (removing = null)} width={400}>
    <p class="confirm"><b>{addressTitle(removing)}</b> will be deleted from Zenith. This can't be undone.</p>
    <div class="actions">
      <button class="btn" onclick={() => (removing = null)}>Cancel</button>
      <button class="btn danger" onclick={remove}><Icon name="trash" size={14} /> Delete</button>
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
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 14px;
  }
  h2 {
    margin: 0;
    font: 600 19px/1.3 var(--font-display);
  }
  .rows {
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 14px;
    min-height: 56px;
    padding: 8px 10px 8px 14px;
  }
  .row + .row {
    border-top: 1px solid var(--line);
  }
  .row:hover {
    background: var(--hover);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    font-weight: 500;
  }
  .meta {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .acts {
    display: flex;
    gap: 2px;
    opacity: 0.55;
    transition: opacity var(--fast) ease;
  }
  .row:hover .acts,
  .row:focus-within .acts {
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
</style>
