<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import Segmented from '$shared/ui/Segmented.svelte';
  import Modal from './Modal.svelte';
  import MasterPasswordForm from './MasterPasswordForm.svelte';
  import NewMasterForm from './NewMasterForm.svelte';
  import RecoveryKey from './RecoveryKey.svelte';
  import { settings, type Settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { vault, vaultApi } from './vault.svelte';

  let { onreset }: { onreset: () => void } = $props();

  const s = $derived(settings.value);
  const st = $derived(vault.status);
  let never = $state<string[]>([]);
  let dialog = $state<'change' | 'recovery' | null>(null);
  let current = $state('');
  let newKey = $state('');

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }

  async function loadNever() {
    never = await vaultApi.neverList().catch(() => []);
  }
  $effect(() => {
    void vault.revision;
    void loadNever();
  });

  async function changeMaster(next: string) {
    await vaultApi.changeMaster(current, next);
    current = '';
    dialog = null;
    ui.toast('Master password changed', { icon: 'check' });
  }

  async function makeRecoveryKey(master: string) {
    newKey = await vaultApi.newRecoveryKey(master);
  }

  const off = $derived(!s.passwordsEnabled);
  const lockOptions = [
    { value: 1, label: 'After 1 minute' },
    { value: 5, label: 'After 5 minutes' },
    { value: 15, label: 'After 15 minutes' },
    { value: 30, label: 'After 30 minutes' },
    { value: 60, label: 'After 1 hour' },
    { value: 240, label: 'After 4 hours' },
    { value: 0, label: 'Only when Zenith closes' },
  ];
</script>

<div class="body">
  <h2>Autofill settings</h2>

  <h3>Passwords</h3>
  <div class="group">
    <div class="row">
      <div class="text"><b>Password manager</b><span>Save, suggest and fill in passwords. Turning it off keeps your saved passwords.</span></div>
      <Toggle checked={s.passwordsEnabled} onchange={(v) => set('passwordsEnabled', v)} label="Password manager" />
    </div>
    <div class="row" class:dim={off}>
      <div class="text"><b>Offer to save passwords</b><span>Ask after you sign in to a site. Never asked in incognito windows.</span></div>
      <Toggle checked={s.passwordsOfferSave} disabled={off} onchange={(v) => set('passwordsOfferSave', v)} label="Offer to save passwords" />
    </div>
    <div class="row" class:dim={off}>
      <div class="text"><b>Suggest strong passwords</b><span>When you create an account or change a password, offer a random one.</span></div>
      <Toggle checked={s.passwordGenerator} disabled={off} onchange={(v) => set('passwordGenerator', v)} label="Suggest strong passwords" />
    </div>
  </div>

  <h3>Payment cards</h3>
  <div class="group">
    <div class="row" class:dim={off}>
      <div class="text"><b>Offer to save cards</b><span>Ask after you pay on a site. Never asked in incognito windows. The security code is only saved if you tick the box.</span></div>
      <Toggle checked={s.cardsOfferSave} disabled={off} onchange={(v) => set('cardsOfferSave', v)} label="Offer to save cards" />
    </div>
    <div class="row" class:dim={off}>
      <div class="text"><b>Show saved cards on payment forms</b><span>Click a card number field to pick a card. Your master password is asked for every time a card is filled in, and cards are only filled on secure (https) pages.</span></div>
      <Toggle checked={s.autofillCards} disabled={off} onchange={(v) => set('autofillCards', v)} label="Show saved cards" />
    </div>
  </div>

  <h3>Autofill</h3>
  <div class="group">
    <div class="row" class:dim={off}>
      <div class="text"><b>Show saved accounts on login fields</b><span>Click a username or password field to pick one of your accounts for the site.</span></div>
      <Toggle checked={s.autofillSuggest} disabled={off} onchange={(v) => set('autofillSuggest', v)} label="Show saved accounts" />
    </div>
    <div class="row" class:dim={off}>
      <div class="text"><b>Fill usernames and emails</b></div>
      <Toggle checked={s.autofillUsernames} disabled={off} onchange={(v) => set('autofillUsernames', v)} label="Fill usernames" />
    </div>
    <div class="row" class:dim={off}>
      <div class="text"><b>Fill passwords</b><span>Off: Zenith fills in the username only and you type the password.</span></div>
      <Toggle checked={s.autofillPasswords} disabled={off} onchange={(v) => set('autofillPasswords', v)} label="Fill passwords" />
    </div>
    <div class="row" class:dim={off}>
      <div class="text">
        <b>When to fill</b>
        <span
          >{s.autofillOnLoad
            ? 'Fills a login form as soon as it appears, when you have exactly one account saved for that exact site and passwords are unlocked.'
            : 'Fills in only after you pick an account.'}</span
        >
      </div>
      <Segmented
        value={s.autofillOnLoad ? 'auto' : 'pick'}
        options={[
          { value: 'pick', label: 'When I pick an account' },
          { value: 'auto', label: 'Automatically' },
        ]}
        onchange={(v) => set('autofillOnLoad', v === 'auto')}
      />
    </div>
    <div class="row" class:dim={off}>
      <div class="text"><b>Use on other parts of the same site</b><span>For example, offer an accounts.google.com password on mail.google.com. Never across different sites or hosting services.</span></div>
      <Toggle checked={s.autofillSameSite} disabled={off} onchange={(v) => set('autofillSameSite', v)} label="Same site" />
    </div>
    <div class="row">
      <div class="text"><b>Only on the site a password belongs to</b><span>Zenith checks the page's address before filling and never fills passwords saved for HTTPS into an insecure page.</span></div>
      <span class="ok"><Icon name="shieldCheck" size={16} /> Always on</span>
    </div>
  </div>

  <h3>Master password</h3>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>Ask for the master password</b>
        <span>{s.vaultAskEveryTime ? 'Before every password is filled in, shown or copied.' : 'Once, then not again until passwords lock.'}</span>
      </div>
      <Segmented
        value={s.vaultAskEveryTime ? 'always' : 'once'}
        options={[
          { value: 'once', label: 'When locked' },
          { value: 'always', label: 'Every time' },
        ]}
        onchange={(v) => set('vaultAskEveryTime', v === 'always')}
      />
    </div>
    <div class="row">
      <div class="text"><b>Lock passwords</b><span>After this long without using a password. Passwords always lock when Zenith closes or restarts.</span></div>
      <select class="field select" value={s.vaultLockMinutes} onchange={(e) => set('vaultLockMinutes', Number((e.currentTarget as HTMLSelectElement).value))}>
        {#each lockOptions as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
    </div>
    {#if st.exists}
      <div class="row">
        <div class="text"><b>{st.unlocked ? 'Unlocked' : 'Locked'}</b><span>{st.unlocked ? 'Saved passwords can be filled in and viewed.' : 'The master password is needed to fill in or view passwords.'}</span></div>
        {#if st.unlocked}<button class="btn" onclick={() => vault.lock()}><Icon name="lock" size={14} /> Lock now</button>{/if}
      </div>
      <div class="row">
        <div class="text"><b>Change master password</b><span>Your saved passwords stay as they are.</span></div>
        <button class="btn" onclick={() => (dialog = 'change')}>Change…</button>
      </div>
      <div class="row">
        <div class="text">
          <b>Recovery key</b>
          <span>{st.hasRecovery ? 'Lets you set a new master password if you forget it. Making a new one stops the old one from working.' : 'No recovery key is set up.'}</span>
        </div>
        <button class="btn" onclick={() => (dialog = 'recovery')}>New recovery key…</button>
      </div>
    {/if}
    <div class="row">
      <div class="text"><b>Delete all saved passwords</b><span>Removes the passwords and the master password from this PC. Asks for confirmation first.</span></div>
      <button class="btn danger" disabled={!st.exists && !st.damaged} onclick={onreset}>Delete…</button>
    </div>
  </div>

  <h3>Never saved for</h3>
  <div class="group">
    {#if never.length === 0}
      <p class="note">Sites where you choose “Never for this site” appear here.</p>
    {:else}
      {#each never as site (site)}
        <div class="list-row">
          <Icon name="globe" size={15} />
          <span class="grow">{site}</span>
          <button
            class="icon-btn sm"
            title="Offer to save passwords again"
            onclick={async () => {
              await vaultApi.neverRemove(site);
              void loadNever();
            }}><Icon name="close" size={14} /></button
          >
        </div>
      {/each}
    {/if}
  </div>
</div>

{#if dialog === 'change'}
  <Modal title="Change master password" onclose={() => ((dialog = null), (current = ''))}>
    <div class="dlg">
      <label for="cm-cur">Current master password</label>
      <input id="cm-cur" class="field" type="password" bind:value={current} autocomplete="off" />
      <NewMasterForm onsubmit={changeMaster} submitLabel="Change master password" autofocus={false} />
    </div>
  </Modal>
{:else if dialog === 'recovery'}
  <Modal title="New recovery key" onclose={() => ((dialog = null), (newKey = ''))}>
    <div class="dlg">
      {#if newKey}
        <RecoveryKey value={newKey} ondone={() => ((dialog = null), (newKey = ''))} />
      {:else}
        <p>Your current recovery key will stop working. Enter your master password to create a new one.</p>
        <MasterPasswordForm onsubmit={makeRecoveryKey} submitLabel="Create recovery key" forgot={false} />
      {/if}
    </div>
  </Modal>
{/if}

<style>
  .body {
    max-width: 760px;
    padding: 28px 36px 80px;
  }
  h2 {
    margin: 0 0 12px;
    font: 600 18px/1.3 var(--font-display);
  }
  h3 {
    margin: 22px 0 10px;
    font: 600 14px/1.3 var(--font-display);
    color: var(--text-2);
  }
  .group {
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    padding: 4px 16px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    min-height: 58px;
    padding: 10px 0;
    border-bottom: 1px solid var(--line);
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row.dim .text {
    opacity: 0.5;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .text b {
    font-weight: 500;
  }
  .text span {
    font-size: 12px;
    color: var(--text-3);
    line-height: 1.4;
  }
  .select {
    height: 32px;
    padding: 0 8px;
  }
  .ok {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ok);
    font-weight: 600;
    font-size: 12.5px;
  }
  .note {
    margin: 12px 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .list-row {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 40px;
    border-bottom: 1px solid var(--line);
  }
  .list-row:last-child {
    border-bottom: 0;
  }
  .grow {
    flex: 1;
  }
  .dlg {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .dlg label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  .dlg p {
    margin: 0 0 4px;
    line-height: 1.5;
    color: var(--text-2);
  }
</style>
