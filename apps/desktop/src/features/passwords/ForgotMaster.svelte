<script lang="ts">
  // Forgot master password: set a new one with the recovery key (nothing is lost), or, without
  // it, delete the vault and start over. Nothing is deleted without typing DELETE.
  import Icon from '$shared/ui/Icon.svelte';
  import NewMasterForm from './NewMasterForm.svelte';
  import RecoveryKey from './RecoveryKey.svelte';
  import { formatRecoveryKey } from './passwords';
  import { ui } from '$shared/ui/ui.svelte';
  import { vault, vaultApi } from './vault.svelte';

  let { ondone }: { ondone: () => void } = $props();

  const st = $derived(vault.status);
  let mode = $state<'choose' | 'recover' | 'reset'>(vault.status.hasRecovery && !vault.status.damaged ? 'recover' : 'choose');
  let keyText = $state('');
  let newKey = $state('');
  let confirmText = $state('');
  let busy = $state(false);
  let error = $state('');

  async function recover(master: string) {
    newKey = await vaultApi.recover(keyText, master);
    await vault.refresh();
  }

  async function reset() {
    busy = true;
    error = '';
    try {
      await vaultApi.reset(confirmText);
      await vault.refresh();
      ui.toast('Saved passwords deleted. Create a new master password to start again.', { icon: 'key', timeout: 5000 });
      ondone();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="forgot">
  <div class="card">
    {#if newKey}
      <h2>New master password set</h2>
      <p>Your passwords are unchanged. The recovery key you used no longer works; here is a new one.</p>
      <RecoveryKey value={newKey} ondone={ondone} doneLabel="Go to my passwords" />
    {:else}
      <button class="back" onclick={ondone}>‹ Back</button>
      <h2>Forgot your master password?</h2>
      <p>
        Your passwords are encrypted with your master password. Zenith never stores it, so nobody, including Zenith, can
        show it to you or send it to you.
      </p>

      {#if mode !== 'reset' && st.hasRecovery && !st.damaged}
        <section class:open={mode === 'recover'}>
          <h3><Icon name="key" size={16} /> Use your recovery key</h3>
          <p class="small">The key you saved when you set up the master password. Your passwords are kept.</p>
          <input
            class="field key"
            placeholder="XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX"
            spellcheck="false"
            autocomplete="off"
            aria-label="Recovery key"
            value={keyText}
            oninput={(e) => (keyText = formatRecoveryKey((e.currentTarget as HTMLInputElement).value))}
          />
          {#if keyText.replace(/-/g, '').length >= 32}
            <NewMasterForm onsubmit={recover} submitLabel="Set new master password" autofocus={false} />
          {/if}
        </section>
      {/if}

      {#if mode === 'reset'}
        <section class="danger-zone">
          <h3><Icon name="alert" size={16} /> Delete saved passwords and start over</h3>
          <p class="small">
            This permanently deletes {st.count === 1 ? 'your saved password' : `all ${st.count} saved passwords`} and your master
            password. They can't be recovered afterwards. If you can still sign in to your sites some other way, you can save the
            passwords again later.
          </p>
          <label class="small" for="confirm-reset">Type <b>DELETE</b> to confirm</label>
          <input id="confirm-reset" class="field" bind:value={confirmText} autocomplete="off" spellcheck="false" />
          {#if error}<p class="error">{error}</p>{/if}
          <div class="row">
            <button class="btn" onclick={() => (mode = st.hasRecovery ? 'recover' : 'choose')}>Cancel</button>
            <button class="btn danger" disabled={busy || confirmText !== 'DELETE'} onclick={reset}>
              {busy ? 'Deleting…' : 'Delete passwords and start over'}
            </button>
          </div>
        </section>
      {:else}
        <section>
          <h3><Icon name="trash" size={16} /> {st.hasRecovery ? "Don't have the recovery key?" : 'No recovery key'}</h3>
          <p class="small">
            Without the master password or the recovery key, the saved passwords can't be decrypted. The only way forward is to
            delete them and create a new master password.
          </p>
          <div class="row"><button class="btn" onclick={() => (mode = 'reset')}>Start over…</button></div>
        </section>
      {/if}
    {/if}
  </div>
</div>

<style>
  .forgot {
    min-height: 100%;
    display: grid;
    place-items: center;
    padding: 40px 24px;
  }
  .card {
    width: min(480px, 100%);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 28px;
    border-radius: var(--radius-xl);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .back {
    align-self: flex-start;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .back:hover {
    color: var(--text);
  }
  h2 {
    margin: 0;
    font: 600 19px/1.3 var(--font-display);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font: 600 14px/1.3 var(--font-display);
  }
  p {
    margin: 0;
    line-height: 1.5;
    color: var(--text-2);
  }
  .small {
    font-size: 12.5px;
    color: var(--text-2);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--line);
    background: var(--page-bg);
  }
  .danger-zone {
    border-color: color-mix(in srgb, var(--danger) 45%, transparent);
  }
  .danger-zone h3 {
    color: var(--danger);
  }
  .key {
    font-family: var(--mono);
    letter-spacing: 0.03em;
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .error {
    color: var(--danger);
    font-size: 12px;
  }
</style>
