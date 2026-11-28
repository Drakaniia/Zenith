<script lang="ts">
  // Add or edit a saved address. Every field is optional, but at least one must be filled in.
  import { onMount, untrack } from 'svelte';
  import Modal from '../Modal.svelte';
  import { emptyAddress, type SavedAddress } from './addresses';
  import { vaultApi } from '../vault.svelte';

  let {
    address = null,
    onclose,
    onsaved,
  }: {
    /** Editing a saved address (null: adding). */
    address?: SavedAddress | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  let a = $state<SavedAddress>({ ...(untrack(() => address) ?? emptyAddress()) });
  let busy = $state(false);
  let error = $state('');
  let first: HTMLInputElement | undefined = $state();

  onMount(() => requestAnimationFrame(() => first?.focus()));

  async function save(e: Event) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      if (address) await vaultApi.addressUpdate(address.id, a);
      else await vaultApi.addressAdd(a);
      onsaved();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={address ? 'Edit address' : 'Add address'} onclose={onclose} width={480}>
  <form class="editor" onsubmit={save}>
    <label for="ae-name">Name</label>
    <input id="ae-name" class="field" bind:this={first} bind:value={a.name} autocomplete="off" />
    <label for="ae-org">Organization <span class="opt">optional</span></label>
    <input id="ae-org" class="field" bind:value={a.organization} autocomplete="off" />
    <label for="ae-street">Street address</label>
    <textarea id="ae-street" class="field" rows="2" bind:value={a.street} autocomplete="off"></textarea>
    <div class="two">
      <div>
        <label for="ae-city">City</label>
        <input id="ae-city" class="field" bind:value={a.city} autocomplete="off" />
      </div>
      <div>
        <label for="ae-region">State / region</label>
        <input id="ae-region" class="field" bind:value={a.region} autocomplete="off" />
      </div>
      <div>
        <label for="ae-postal">Postal code</label>
        <input id="ae-postal" class="field" bind:value={a.postal} autocomplete="off" />
      </div>
      <div>
        <label for="ae-country">Country</label>
        <input id="ae-country" class="field" bind:value={a.country} autocomplete="off" />
      </div>
      <div>
        <label for="ae-phone">Phone</label>
        <input id="ae-phone" class="field" type="tel" bind:value={a.phone} autocomplete="off" />
      </div>
      <div>
        <label for="ae-email">Email</label>
        <input id="ae-email" class="field" type="email" bind:value={a.email} autocomplete="off" />
      </div>
    </div>
    <p class="hint">Addresses are kept in your encrypted vault and can be filled into forms without your master password.</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>Cancel</button>
      <button type="submit" class="btn primary" disabled={busy}>{busy ? 'Saving…' : 'Save'}</button>
    </div>
  </form>
</Modal>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label {
    margin-top: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  .opt {
    font-weight: 400;
    color: var(--text-3);
  }
  .field {
    width: 100%;
  }
  textarea.field {
    resize: vertical;
    font: inherit;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0 10px;
  }
  .two > div {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .hint {
    margin: 8px 0 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-3);
  }
  .error {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
</style>
