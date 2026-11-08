<script lang="ts">
  // Saved accounts under a login field in a page. Drawn by the chrome (raised over the page
  // while it shows), so the page never sees the list. Choosing an account asks for the master
  // password when needed, then the backend fills the form after checking the page's site.
  import { tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import MasterPasswordForm from './MasterPasswordForm.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { overlayLease, paneRectOf } from '$platform/layout.svelte';
  import { webviewApi } from '$platform/webview-api';
  import { openInternal } from '$features/browser/target';
  import { settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { vault, vaultApi } from './vault.svelte';
  import { needsMaster, placeSuggestions, type SavedAccount } from './passwords';
  import { brandName, formatExpiry, isExpired, type SavedCard } from './cards/cards';

  const s = $derived(vault.suggest);
  const visible = $derived(!!s && browser.isVisible(s.tabId));
  /** The account waiting for the master password. */
  let pending = $state<SavedAccount | null>(null);
  /** The card waiting for the master password (typed again for every card fill). */
  let pendingCard = $state<SavedCard | null>(null);
  /** Clicks are ignored for a moment after the list appears, so a page can't make the user
   *  pick an account by opening the list under a click that was already on its way. */
  let armed = $state(false);
  let busy = $state(false);
  let winW = $state(window.innerWidth);
  let winH = $state(window.innerHeight);
  let el: HTMLDivElement | undefined = $state();
  let height = $state(180);

  $effect(() => {
    if (!s) return;
    void s.rect;
    pending = null;
    pendingCard = null;
    armed = false;
    const t = setTimeout(() => (armed = true), 350);
    return () => clearTimeout(t);
  });

  $effect(() => {
    vault.unlocking = pending !== null || pendingCard !== null;
  });

  // The tab went away or another tab is showing.
  $effect(() => {
    if (s && !browser.isVisible(s.tabId)) vault.closeSuggest();
  });

  $effect(() => {
    // Also when typing changes how many accounts match.
    void s?.accounts.length;
    void s?.cards.length;
    if (el) height = el.offsetHeight;
  });

  const pos = $derived.by(() => {
    if (!s) return null;
    void winW;
    void winH;
    const pane = paneRectOf(s.tabId);
    if (!pane) return null;
    return placeSuggestions(s.rect, s.dpr, window.devicePixelRatio || 1, pane, { w: 300, h: height });
  });

  // Which way the list flies in and out. Remembered, because the exit transition starts after the
  // list has been closed, when `pos` is already null: reading it there threw inside Svelte's
  // teardown, which left the list and its full-window backdrop on screen for good.
  let lastAbove = false;
  const above = $derived.by(() => (pos ? (lastAbove = pos.above) : lastAbove));

  function close(refocus = true) {
    const tab = s?.tabId;
    vault.closeSuggest();
    if (refocus && tab) void webviewApi.focusView(tab).catch(() => {});
  }

  async function choose(a: SavedAccount, master: string | null = null) {
    // The settle delay guards a click on a listed account, not the master password prompt.
    if (!s || busy || (!armed && !master)) return;
    const tab = s.tabId;
    const needPrompt = !master && (!vault.status.unlocked || settings.value.vaultAskEveryTime);
    if (needPrompt) {
      pending = a;
      // The master password field needs the keyboard.
      void webviewApi.focusView(null);
      return;
    }
    busy = true;
    try {
      await vaultApi.fill(tab, a.id, master);
      vault.closeSuggest(false);
      void webviewApi.focusView(tab).catch(() => {});
    } catch (e) {
      if (needsMaster(e)) {
        pending = a;
        void webviewApi.focusView(null);
        await tick();
      } else if (master) {
        throw e;
      } else {
        ui.toast(String(e), { icon: 'alert' });
        close();
      }
    } finally {
      busy = false;
    }
  }

  async function unlockAndFill(master: string) {
    if (!pending) return;
    await choose(pending, master);
  }

  function chooseCard(c: SavedCard) {
    if (!s || busy || !armed) return;
    // Cards always need the master password, even when the vault is unlocked.
    pendingCard = c;
    void webviewApi.focusView(null);
  }

  async function unlockAndFillCard(master: string) {
    if (!s || !pendingCard) return;
    const tab = s.tabId;
    busy = true;
    try {
      await vaultApi.fillCard(tab, pendingCard.id, master);
      vault.closeSuggest(false);
      void webviewApi.focusView(tab).catch(() => {});
    } finally {
      busy = false;
    }
  }

  async function useGenerated() {
    if (!s || !armed) return;
    const tab = s.tabId;
    try {
      await vaultApi.fillGenerated(tab);
      ui.toast('Strong password filled. Zenith will offer to save it when you sign up.', { icon: 'key', timeout: 4500 });
    } catch (e) {
      ui.toast(String(e), { icon: 'alert' });
    }
    vault.closeSuggest(false);
    void webviewApi.focusView(tab).catch(() => {});
  }

  function manage() {
    const wallet = s?.field === 'card';
    close(false);
    if (wallet) openInternal('passwords', 'wallet');
    else openInternal('passwords');
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && s) {
      e.preventDefault();
      close();
    }
  }
</script>

<svelte:window bind:innerWidth={winW} bind:innerHeight={winH} onkeydown={onKey} />

{#if s && visible && pos}
  <div class="backdrop" use:overlayLease role="presentation" onpointerdown={() => close()}></div>
  <div
    class="suggest glass"
    bind:this={el}
    style:left="{pos.x}px"
    style:top="{pos.y}px"
    style:width="{pos.w}px"
    role="dialog"
    aria-label={s.field === 'card' ? `Saved cards for ${s.site}` : `Saved passwords for ${s.site}`}
    transition:fly={{ y: above ? 6 : -6, duration: 140 }}
  >
    {#if pendingCard}
      <div class="unlock">
        <p>
          <Icon name="lock" size={14} />
          Enter your master password to fill <b>{brandName(pendingCard.brand)} •••• {pendingCard.last4}</b> on {s.site}
        </p>
        <MasterPasswordForm onsubmit={unlockAndFillCard} submitLabel="Unlock and fill" compact />
        <button class="back" onclick={() => (pendingCard = null)}>‹ Back to cards</button>
      </div>
    {:else if s.field === 'card'}
      <div class="head">
        <Icon name="card" size={13} />
        <span class="ellipsis">Saved cards</span>
        {#if vault.status.exists}
          <Icon name={vault.status.unlocked ? 'unlock' : 'lock'} size={13} class="state" />
        {/if}
      </div>
      <div class="list" role="listbox">
        {#each s.cards as c (c.id)}
          <button class="item" role="option" aria-selected="false" disabled={busy} onclick={() => chooseCard(c)}>
            <span class="avatar"><Icon name="card" size={15} /></span>
            <span class="who">
              <b class="ellipsis">{brandName(c.brand)} •••• {c.last4}</b>
              <small class="ellipsis">
                {c.label ? c.label + ' · ' : ''}{c.holder ? c.holder + ' · ' : ''}{isExpired(c.expMonth, c.expYear) ? 'Expired ' : 'Expires '}{formatExpiry(c.expMonth, c.expYear)}
              </small>
            </span>
          </button>
        {/each}
      </div>
      <div class="foot">
        <button onclick={manage}><Icon name="settings" size={13} /> Manage payment methods</button>
      </div>
    {:else if pending}
      <div class="unlock">
        <p>
          <Icon name="lock" size={14} />
          Enter your master password to sign in as <b>{pending.username || 'this account'}</b>
        </p>
        <MasterPasswordForm onsubmit={unlockAndFill} submitLabel="Unlock and fill" compact />
        <button class="back" onclick={() => (pending = null)}>‹ Back to accounts</button>
      </div>
    {:else}
      <div class="head">
        <Icon name="key" size={13} />
        <span class="ellipsis">{s.accounts.length ? `Saved for ${s.site}` : 'Zenith Passwords'}</span>
        {#if vault.status.exists}
          <Icon name={vault.status.unlocked ? 'unlock' : 'lock'} size={13} class="state" />
        {/if}
      </div>
      <div class="list" role="listbox">
        {#each s.accounts as a (a.id)}
          <button class="item" role="option" aria-selected="false" disabled={busy} onclick={() => choose(a)}>
            <span class="avatar">{(a.label || a.username || a.site).slice(0, 1).toUpperCase()}</span>
            <span class="who">
              <b class="ellipsis">{a.username || '(no username)'}</b>
              <small class="ellipsis">{a.label ? a.label + ' · ' : ''}{a.exact ? '••••••••' : `saved for ${a.site}`}</small>
            </span>
          </button>
        {/each}
        {#if s.generated}
          <button class="item gen" onclick={useGenerated}>
            <span class="avatar"><Icon name="sparkle" size={15} /></span>
            <span class="who">
              <b>Use a strong password</b>
              <small class="mono ellipsis">{s.generated}</small>
            </span>
          </button>
        {/if}
      </div>
      <div class="foot">
        <button onclick={manage}><Icon name="settings" size={13} /> Manage passwords</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 78;
  }
  .suggest {
    position: fixed;
    z-index: 79;
    min-width: 260px;
    max-width: 420px;
    padding: 6px;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 8px 8px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-3);
  }
  .head :global(.state) {
    margin-left: auto;
    opacity: 0.7;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 260px;
    overflow-y: auto;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    border-radius: var(--radius);
    text-align: left;
  }
  .item:hover,
  .item:focus-visible {
    background: var(--hover);
  }
  .avatar {
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-weight: 700;
    font-size: 13px;
    color: var(--accent-fg);
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .who b {
    font-weight: 500;
  }
  .who small {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .mono {
    font-family: var(--mono);
  }
  .foot {
    margin-top: 4px;
    padding-top: 4px;
    border-top: 1px solid var(--line);
  }
  .foot button {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 7px 8px;
    border-radius: var(--radius);
    font-size: 12.5px;
    color: var(--text-2);
  }
  .foot button:hover {
    background: var(--hover);
    color: var(--text);
  }
  .unlock {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 8px;
  }
  .unlock p {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    margin: 0;
    font-size: 12.5px;
    line-height: 1.45;
  }
  .unlock p :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .back {
    align-self: flex-start;
    font-size: 12px;
    color: var(--text-3);
  }
  .back:hover {
    color: var(--text);
  }
</style>
