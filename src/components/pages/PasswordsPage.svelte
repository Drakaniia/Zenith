<script lang="ts">
  // zenith://passwords — the password manager. Sections: passwords (the default), wallet,
  // personal, settings, and forgot (master password recovery). Everything that shows or
  // changes saved passwords needs the vault unlocked; autofill settings don't.
  import Icon from '../Icon.svelte';
  import VaultGate from '../passwords/VaultGate.svelte';
  import ForgotMaster from '../passwords/ForgotMaster.svelte';
  import PasswordList from '../passwords/PasswordList.svelte';
  import CardList from '../passwords/CardList.svelte';
  import AutofillSettings from '../passwords/AutofillSettings.svelte';
  import { openInternal } from '../../lib/actions';
  import { browser } from '../../lib/browser.svelte';
  import { vault } from '../../lib/vault.svelte';
  import type { IconName } from '../../lib/icons';

  let { section = '' }: { section?: string } = $props();

  const NAV: { id: string; label: string; icon: IconName }[] = [
    { id: 'wallet', label: 'Wallet & cards', icon: 'card' },
    { id: 'passwords', label: 'Passwords', icon: 'key' },
    { id: 'personal', label: 'Personal data', icon: 'idCard' },
    { id: 'settings', label: 'Autofill settings', icon: 'settings' },
  ];

  const current = $derived(NAV.some((n) => n.id === section) || section === 'forgot' ? section : 'passwords');
  const st = $derived(vault.status);

  $effect(() => {
    if (!vault.loaded) void vault.refresh();
  });

  function go(id: string) {
    const tab = browser.activeTab;
    const url = `zenith://passwords${id === 'passwords' ? '' : '/' + id}`;
    if (tab && tab.url.startsWith('zenith://passwords')) tab.url = url;
    else openInternal('passwords', id === 'passwords' ? undefined : id);
  }
</script>

<div class="pw">
  <nav class="nav">
    <h1>Passwords</h1>
    {#each NAV as n (n.id)}
      <button class="nav-item" class:on={current === n.id || (n.id === 'passwords' && current === 'forgot')} onclick={() => go(n.id)}>
        <Icon name={n.icon} size={16} />
        {n.label}
        {#if n.id === 'passwords' && st.exists && st.count}<span class="count">{st.count}</span>{/if}
        {#if n.id === 'wallet' && st.exists && st.cardCount}<span class="count">{st.cardCount}</span>{/if}
      </button>
    {/each}
    <div class="grow"></div>
    {#if st.exists}
      <div class="state" class:open={st.unlocked}>
        <Icon name={st.unlocked ? 'unlock' : 'lock'} size={15} />
        <span>{st.unlocked ? 'Unlocked' : 'Locked'}</span>
        {#if st.unlocked}<button class="btn sm" onclick={() => vault.lock()}>Lock</button>{/if}
      </div>
    {/if}
    <button class="nav-item" onclick={() => openInternal('settings')}>
      <Icon name="settings" size={16} />
      All settings
      <Icon name="external" size={13} class="faint-icon" />
    </button>
  </nav>

  <div class="main">
    {#if current === 'settings'}
      <AutofillSettings onreset={() => go('forgot')} />
    {:else if current === 'personal'}
      <div class="soon">
        <div class="badge"><Icon name="idCard" size={26} /></div>
        <h2>Personal data</h2>
        <p>Saving names, addresses and phone numbers for forms is coming to Zenith. They will be kept in the same encrypted vault as your passwords.</p>
        <button class="btn" onclick={() => go('passwords')}><Icon name="key" size={15} /> Go to passwords</button>
      </div>
    {:else if current === 'forgot'}
      <ForgotMaster ondone={() => go('passwords')} />
    {:else if !vault.loaded}
      <div class="soon"><p>Loading…</p></div>
    {:else if !st.unlocked || st.damaged}
      <VaultGate onforgot={() => go('forgot')} />
    {:else if current === 'wallet'}
      <CardList />
    {:else}
      <PasswordList />
    {/if}
  </div>
</div>

<style>
  .pw {
    position: relative;
    display: flex;
    height: 100%;
    user-select: none;
  }
  .nav {
    width: 230px;
    flex: none;
    padding: 28px 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-right: 1px solid var(--line);
  }
  h1 {
    margin: 0 0 14px 10px;
    font: 600 22px/1.2 var(--font-display);
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--radius);
    color: var(--text-2);
    text-align: left;
  }
  .nav-item:hover {
    background: var(--hover);
    color: var(--text);
  }
  .nav-item.on {
    background: var(--selected);
    color: var(--text);
    font-weight: 600;
  }
  .nav-item :global(.faint-icon) {
    margin-left: auto;
    opacity: 0.5;
  }
  .count {
    margin-left: auto;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-3);
  }
  .grow {
    flex: 1;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 4px 8px;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--field);
    font-size: 12.5px;
    color: var(--text-2);
  }
  .state.open {
    color: var(--ok);
  }
  .state .btn {
    margin-left: auto;
  }
  .btn.sm {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
  .main {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    user-select: text;
  }
  .soon {
    min-height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px;
    text-align: center;
  }
  .soon .badge {
    width: 56px;
    height: 56px;
    border-radius: 16px;
    display: grid;
    place-items: center;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .soon h2 {
    margin: 0;
    font: 600 19px/1.3 var(--font-display);
  }
  .soon p {
    margin: 0;
    max-width: 440px;
    line-height: 1.5;
    color: var(--text-2);
  }
</style>
