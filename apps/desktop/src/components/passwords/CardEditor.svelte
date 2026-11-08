<script lang="ts">
  // Add or edit a saved card. The number is never loaded back into the form: when editing, leaving
  // it empty keeps the saved one. The security code is optional and kept encrypted.
  import { onMount, untrack } from 'svelte';
  import Modal from './Modal.svelte';
  import {
    brandName,
    brandOfNumber,
    digitsOf,
    formatExpiry,
    groupNumber,
    numberProblem,
    parseExpiry,
    type SavedCard,
  } from '../../lib/cards';
  import { vaultApi } from '../../lib/vault.svelte';

  let {
    card = null,
    onclose,
    onsaved,
  }: {
    /** Editing a saved card (null: adding). */
    card?: SavedCard | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  const start = untrack(() => card);
  let number = $state('');
  let holder = $state(start?.holder ?? '');
  let expiry = $state(start ? formatExpiry(start.expMonth, start.expYear) : '');
  let cvc = $state('');
  let removeCvc = $state(false);
  let label = $state(start?.label ?? '');
  let busy = $state(false);
  let error = $state('');
  let first: HTMLInputElement | undefined = $state();
  const brand = $derived(brandOfNumber(number));

  onMount(() => requestAnimationFrame(() => first?.focus()));

  function onNumber(e: Event) {
    const el = e.currentTarget as HTMLInputElement;
    number = groupNumber(el.value);
    el.value = number;
  }

  function onExpiry(e: Event) {
    const el = e.currentTarget as HTMLInputElement;
    const d = digitsOf(el.value).slice(0, 4);
    // 0831 → 08/31, and a lone 2–9 becomes 02–09 so the month is never ambiguous.
    expiry = d.length > 2 ? `${d.slice(0, 2)}/${d.slice(2)}` : d.length === 1 && d > '1' ? `0${d}/` : d;
    el.value = expiry;
  }

  async function save(e: Event) {
    e.preventDefault();
    const digits = digitsOf(number);
    if (!card || digits) {
      const problem = numberProblem(number);
      if (problem) return void (error = problem);
    }
    const exp = parseExpiry(expiry);
    if (!exp) return void (error = 'Enter the expiry as MM/YY.');
    const code = digitsOf(cvc);
    if (cvc && (code.length < 3 || code.length > 4)) return void (error = 'The security code is 3 or 4 digits.');
    busy = true;
    error = '';
    const input = {
      number: digits || null,
      holder: holder.trim(),
      expMonth: exp.month,
      expYear: exp.year,
      label: label.trim(),
      // Editing: empty keeps the saved code unless "remove" is ticked.
      cvc: card ? (removeCvc ? '' : code || null) : code,
    };
    try {
      if (card) await vaultApi.cardUpdate(card.id, input);
      else await vaultApi.cardAdd(input);
      number = '';
      cvc = '';
      onsaved();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function close() {
    number = '';
    cvc = '';
    onclose();
  }
</script>

<Modal title={card ? 'Edit card' : 'Add card'} onclose={close} width={460}>
  <form class="editor" onsubmit={save}>
    <label for="ce-number">Card number {#if number}<span class="opt">{brandName(brand)}</span>{/if}</label>
    <input
      id="ce-number"
      class="field mono"
      bind:this={first}
      value={number}
      oninput={onNumber}
      inputmode="numeric"
      placeholder={card ? `${brandName(card.brand)} •••• ${card.last4} — leave empty to keep` : '1234 5678 9012 3456'}
      spellcheck="false"
      autocomplete="off"
    />

    <div class="two">
      <div>
        <label for="ce-exp">Expires</label>
        <input id="ce-exp" class="field mono" value={expiry} oninput={onExpiry} inputmode="numeric" placeholder="MM/YY" autocomplete="off" />
      </div>
      <div>
        <label for="ce-cvc">Security code <span class="opt">optional</span></label>
        <input
          id="ce-cvc"
          class="field mono"
          type="password"
          bind:value={cvc}
          inputmode="numeric"
          maxlength="4"
          placeholder={card?.hasCvc ? 'Saved — leave empty to keep' : 'CVC'}
          autocomplete="off"
        />
      </div>
    </div>
    {#if card?.hasCvc}
      <label class="check"><input type="checkbox" bind:checked={removeCvc} /> Remove the saved security code</label>
    {/if}

    <label for="ce-holder">Name on card</label>
    <input id="ce-holder" class="field" bind:value={holder} autocomplete="off" />

    <label for="ce-label">Nickname <span class="opt">optional</span></label>
    <input id="ce-label" class="field" bind:value={label} placeholder="Personal, Work…" autocomplete="off" />

    <p class="hint">Cards are encrypted with your master password. It is asked for again each time a card is filled in or shown.</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <button type="button" class="btn" onclick={close}>Cancel</button>
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
  .mono {
    font-family: var(--mono);
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .two > div {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 400;
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
