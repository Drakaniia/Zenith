<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Modal from './Modal.svelte';
  import { strength, type PasswordEntry } from './passwords';
  import { vaultApi } from './vault.svelte';

  let {
    entry = null,
    initial = null,
    onclose,
    onsaved,
  }: {
    /** Editing an existing entry (null: adding). */
    entry?: PasswordEntry | null;
    /** The entry's password and notes, already revealed. */
    initial?: { password: string; notes: string } | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  // The form starts from the entry it was opened for (the dialog is recreated for another one).
  const start = untrack(() => ({ entry, initial }));
  let url = $state(start.entry?.origin ?? '');
  let username = $state(start.entry?.username ?? '');
  let password = $state(start.initial?.password ?? '');
  let label = $state(start.entry?.label ?? '');
  let notes = $state(start.initial?.notes ?? '');
  let show = $state(!start.entry);
  let busy = $state(false);
  let error = $state('');
  let first: HTMLInputElement | undefined = $state();
  const s = $derived(strength(password));

  onMount(() => requestAnimationFrame(() => first?.focus()));

  async function generate() {
    password = await vaultApi.generate(20);
    show = true;
  }

  async function save(e: Event) {
    e.preventDefault();
    if (!url.trim()) return void (error = 'Enter the website address.');
    if (!password) return void (error = 'Enter a password.');
    busy = true;
    error = '';
    const input = { url: url.trim(), username: username.trim(), password, label: label.trim(), notes };
    try {
      if (entry) await vaultApi.update(entry.id, input);
      else await vaultApi.add(input);
      password = '';
      notes = '';
      onsaved();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function close() {
    password = '';
    notes = '';
    onclose();
  }
</script>

<Modal title={entry ? 'Edit password' : 'Add password'} onclose={close} width={460}>
  <form class="editor" onsubmit={save}>
    <label for="pe-url">Website</label>
    <input id="pe-url" class="field" bind:this={first} bind:value={url} placeholder="example.com" spellcheck="false" autocomplete="off" />

    <label for="pe-user">Username or email</label>
    <input id="pe-user" class="field" bind:value={username} spellcheck="false" autocomplete="off" />

    <label for="pe-pass">Password</label>
    <div class="pass">
      <input id="pe-pass" class="field mono" type={show ? 'text' : 'password'} bind:value={password} spellcheck="false" autocomplete="new-password" />
      <button type="button" class="icon-btn" title={show ? 'Hide password' : 'Show password'} onclick={() => (show = !show)}>
        <Icon name={show ? 'eyeOff' : 'eye'} size={16} />
      </button>
      <button type="button" class="icon-btn" title="Generate a strong password" onclick={generate}><Icon name="dice" size={16} /></button>
    </div>
    {#if password}
      <div class="meter" data-score={s.score}>
        {#each [0, 1, 2, 3] as i (i)}<span class:on={s.score > i}></span>{/each}
        <em>{s.label}</em>
      </div>
    {/if}

    <label for="pe-label">Account name <span class="opt">optional</span></label>
    <input id="pe-label" class="field" bind:value={label} placeholder="Work, Personal…" autocomplete="off" />

    <label for="pe-notes">Notes <span class="opt">optional, encrypted</span></label>
    <textarea id="pe-notes" class="field" rows="3" bind:value={notes}></textarea>

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
  textarea.field {
    resize: vertical;
    min-height: 64px;
    padding: 8px 10px;
    font: inherit;
  }
  .pass {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .pass .field {
    flex: 1;
  }
  .mono {
    font-family: var(--mono);
  }
  .meter {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .meter span {
    flex: 1;
    max-width: 52px;
    height: 4px;
    border-radius: 2px;
    background: var(--line-strong);
  }
  .meter[data-score='0'] span.on,
  .meter[data-score='1'] span.on {
    background: var(--danger);
  }
  .meter[data-score='2'] span.on {
    background: var(--warn);
  }
  .meter[data-score='3'] span.on,
  .meter[data-score='4'] span.on {
    background: var(--ok);
  }
  .meter em {
    margin-left: 6px;
    font-style: normal;
    font-size: 11.5px;
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
