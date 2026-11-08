<script lang="ts">
  // "Save password?" after a login on the visible tab. The password itself stays in the backend;
  // this prompt only knows the site and username. Saving the first password sets up the master
  // password (and shows the recovery key); saving into a locked vault asks for it.
  import { fly } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import MasterPasswordForm from './MasterPasswordForm.svelte';
  import NewMasterForm from './NewMasterForm.svelte';
  import RecoveryKey from './RecoveryKey.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { overlayLease } from '$platform/layout.svelte';
  import { openInternal } from '../../app/actions';
  import { ui } from '$shared/ui/ui.svelte';
  import { vault, vaultApi } from './vault.svelte';
  import { needsMaster } from './passwords';

  const offer = $derived([...vault.offers].reverse().find((o) => browser.isVisible(o.tabId)) ?? null);
  /** ask: the question; unlock: master password; setup: new master password; recovery: show key. */
  let step = $state<'ask' | 'unlock' | 'setup' | 'recovery'>('ask');
  let recoveryKey = $state('');
  let busy = $state(false);
  let shownFor = 0;

  $effect(() => {
    const id = offer?.offerId ?? 0;
    if (id !== shownFor) {
      shownFor = id;
      if (step !== 'recovery') step = 'ask';
    }
  });

  async function respond(action: 'save' | 'never' | 'dismiss', master: string | null = null) {
    if (!offer) return;
    const o = offer;
    if (action === 'save' && !master) {
      if (!vault.status.exists) return void (step = 'setup');
      if (!vault.status.unlocked) return void (step = 'unlock');
    }
    busy = true;
    try {
      await vaultApi.respondOffer(o.offerId, action, master);
      vault.dropOffer(o.offerId);
      if (action === 'save') {
        vault.revision++;
        ui.toast(o.update ? `Password updated for ${o.site}` : `Password saved for ${o.site}`, {
          icon: 'key',
          action: { label: 'View', run: () => openInternal('passwords') },
        });
      } else if (action === 'never') {
        ui.toast(`Zenith won't offer to save passwords on ${o.site}`, { icon: 'key' });
      }
      step = 'ask';
    } catch (e) {
      if (needsMaster(e)) step = 'unlock';
      else if (master) throw e;
      else ui.toast(String(e), { icon: 'alert' });
    } finally {
      busy = false;
    }
  }

  async function setup(master: string) {
    recoveryKey = await vaultApi.setup(master);
    await vault.refresh();
    step = 'recovery';
  }

  async function finishSetup() {
    recoveryKey = '';
    step = 'ask';
    await respond('save');
  }
</script>

{#if step === 'recovery' && recoveryKey}
  <div class="prompt glass wide" use:overlayLease transition:fly={{ y: -10, duration: 160 }} role="dialog" aria-label="Recovery key">
    <RecoveryKey value={recoveryKey} ondone={finishSetup} doneLabel="Save password" />
  </div>
{:else if offer}
  <div class="prompt glass" class:wide={step === 'setup'} use:overlayLease transition:fly={{ y: -10, duration: 160 }} role="dialog" aria-label="Save password">
    <div class="top">
      <div class="icon"><Icon name="key" size={20} /></div>
      <div class="body">
        <p class="title">{offer.update ? 'Update password' : 'Save password'} for <strong>{offer.site}</strong>?</p>
        <div class="cred">
          <span class="user ellipsis"><Icon name="user" size={13} /> {offer.username || 'No username'}</span>
          <span class="dots">••••••••••</span>
        </div>
      </div>
      <button class="icon-btn sm close" title="Not now" onclick={() => respond('dismiss')}><Icon name="close" size={14} /></button>
    </div>

    {#if step === 'unlock'}
      <p class="note">Enter your master password to save it.</p>
      <MasterPasswordForm onsubmit={(m) => respond('save', m)} submitLabel="Unlock and save" compact />
    {:else if step === 'setup'}
      <p class="note">
        Create a master password. It encrypts every password Zenith saves and is asked for before they are filled in or shown.
      </p>
      <NewMasterForm onsubmit={setup} submitLabel="Create and continue" />
    {:else}
      <div class="actions">
        {#if !offer.update}
          <button class="btn ghost" disabled={busy} onclick={() => respond('never')}>Never for this site</button>
        {/if}
        <button class="btn" disabled={busy} onclick={() => respond('dismiss')}>Not now</button>
        <button class="btn primary" disabled={busy} onclick={() => respond('save')}>{offer.update ? 'Update' : 'Save'}</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .prompt {
    position: fixed;
    top: calc(var(--topbar) + 14px);
    right: 20px;
    z-index: 76;
    width: 360px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    border-radius: var(--radius-xl);
  }
  .prompt.wide {
    width: 400px;
  }
  .top {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .icon {
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .title {
    margin: 1px 0 8px;
    line-height: 1.4;
  }
  .cred {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--field);
    font-size: 12.5px;
  }
  .user {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dots {
    color: var(--text-3);
    letter-spacing: 0.1em;
  }
  .close {
    margin: -4px -4px 0 0;
  }
  .note {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-2);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
