<script lang="ts">
  // What the passwords page shows until the vault is unlocked: first-time setup, the lock
  // screen, or an explanation when the password file can't be used.
  import Icon from '$shared/ui/Icon.svelte';
  import MasterPasswordForm from './MasterPasswordForm.svelte';
  import NewMasterForm from './NewMasterForm.svelte';
  import RecoveryKey from './RecoveryKey.svelte';
  import { vault, vaultApi } from './vault.svelte';

  let { onforgot }: { onforgot: () => void } = $props();
  let recoveryKey = $state('');

  async function setup(master: string) {
    const key = await vaultApi.setup(master);
    recoveryKey = key;
  }

  async function unlock(master: string) {
    vault.apply(await vaultApi.unlock(master));
  }

  async function finish() {
    recoveryKey = '';
    await vault.refresh();
  }

  const st = $derived(vault.status);
</script>

<div class="gate">
  <div class="card">
    {#if recoveryKey}
      <RecoveryKey value={recoveryKey} ondone={finish} doneLabel="Go to my passwords" />
    {:else if st.damaged}
      <div class="badge warn"><Icon name="alert" size={26} /></div>
      <h2>Saved passwords can't be opened</h2>
      <p>{st.damaged}</p>
      <p class="small">Zenith hasn't changed or deleted the file. You can start over: the unreadable file is set aside, not deleted.</p>
      <button class="btn danger" onclick={onforgot}>Start over…</button>
    {:else if !st.exists}
      <div class="badge"><Icon name="key" size={26} /></div>
      <h2>Protect your passwords</h2>
      <p>
        Choose a master password. Zenith uses it to encrypt every password it saves, and asks for it before showing or
        filling them in. It never leaves this PC and isn't stored anywhere.
      </p>
      <NewMasterForm onsubmit={setup} />
    {:else}
      <div class="badge"><Icon name="lock" size={26} /></div>
      <h2>Passwords are locked</h2>
      <p>Enter your master password to see and manage your {st.count === 1 ? 'saved password' : `${st.count} saved passwords`}.</p>
      {#if st.retryAfter > 0}
        <p class="small warn-text">Too many wrong attempts. Wait {st.retryAfter} seconds before trying again.</p>
      {/if}
      <MasterPasswordForm onsubmit={unlock} />
    {/if}
  </div>
</div>

<style>
  .gate {
    min-height: 100%;
    display: grid;
    place-items: center;
    padding: 40px 24px;
  }
  .card {
    width: min(420px, 100%);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 28px;
    border-radius: var(--radius-xl);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .badge {
    width: 52px;
    height: 52px;
    border-radius: 16px;
    display: grid;
    place-items: center;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .badge.warn {
    color: var(--warn);
    background: color-mix(in srgb, var(--warn) 16%, transparent);
  }
  h2 {
    margin: 4px 0 0;
    font: 600 19px/1.3 var(--font-display);
  }
  p {
    margin: 0;
    line-height: 1.5;
    color: var(--text-2);
  }
  .small {
    font-size: 12.5px;
  }
  .warn-text {
    color: var(--warn);
  }
</style>
