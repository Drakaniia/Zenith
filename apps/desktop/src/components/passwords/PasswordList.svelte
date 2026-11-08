<script lang="ts">
  // Saved passwords (unlocked vault): search, grouped by site, reveal/copy/edit/delete, and the
  // import, export and backup actions.
  import { onDestroy } from 'svelte';
  import Icon from '../Icon.svelte';
  import Favicon from '../Favicon.svelte';
  import Modal from './Modal.svelte';
  import MasterPasswordForm from './MasterPasswordForm.svelte';
  import EntryEditor from './EntryEditor.svelte';
  import ImportDialog from './ImportDialog.svelte';
  import { filterEntries, groupBySite, needsMaster, type ImportOpened, type PasswordEntry } from '../../lib/passwords';
  import { ui } from '../../lib/ui.svelte';
  import { vault, vaultApi } from '../../lib/vault.svelte';
  import { timeAgo } from '../../lib/ui.svelte';

  let entries = $state<PasswordEntry[]>([]);
  let loading = $state(true);
  let query = $state('');
  /** Revealed passwords by entry id (hidden again after a minute or on lock). */
  let shown = $state<Record<string, string>>({});
  let editor = $state<{ entry: PasswordEntry | null; initial: { password: string; notes: string } | null } | null>(null);
  let removing = $state<PasswordEntry | null>(null);
  let importing = $state<ImportOpened | null>(null);
  /** An action waiting for the master password. */
  let ask = $state<{ title: string; text?: string; warn?: string; label: string; run: (master: string) => Promise<void> } | null>(null);
  const timers = new Map<string, ReturnType<typeof setTimeout>>();

  const visible = $derived(filterEntries(entries, query));
  const groups = $derived(groupBySite(visible));
  const siteCount = $derived(groupBySite(entries).length);

  async function load() {
    try {
      entries = await vaultApi.list();
    } catch (e) {
      if (!needsMaster(e)) ui.toast(String(e), { icon: 'alert' });
      entries = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void vault.revision;
    void load();
  });

  // Locking forgets everything shown.
  $effect(() => {
    if (!vault.status.unlocked) hideAll();
  });

  function hideAll() {
    for (const t of timers.values()) clearTimeout(t);
    timers.clear();
    shown = {};
  }
  onDestroy(hideAll);

  /** Runs a vault call, asking for the master password first when the backend wants it. */
  async function withMaster<T>(title: string, call: (master: string | null) => Promise<T>): Promise<T | undefined> {
    try {
      return await call(null);
    } catch (e) {
      if (!needsMaster(e)) {
        ui.toast(String(e), { icon: 'alert' });
        return undefined;
      }
    }
    return new Promise<T | undefined>((resolve) => {
      ask = {
        title,
        label: 'Continue',
        run: async (master) => {
          const r = await call(master);
          ask = null;
          resolve(r);
        },
      };
    });
  }

  async function toggleReveal(e: PasswordEntry) {
    if (shown[e.id] !== undefined) {
      const { [e.id]: _, ...rest } = shown;
      shown = rest;
      return;
    }
    const r = await withMaster('Show password', (m) => vaultApi.reveal(e.id, m));
    if (!r) return;
    shown = { ...shown, [e.id]: r.password };
    clearTimeout(timers.get(e.id));
    timers.set(
      e.id,
      setTimeout(() => {
        const { [e.id]: _, ...rest } = shown;
        shown = rest;
      }, 60_000),
    );
  }

  async function copy(e: PasswordEntry, field: 'username' | 'password') {
    const ok = await withMaster('Copy password', (m) => vaultApi.copy(e.id, field, m).then(() => true));
    if (ok)
      ui.toast(field === 'password' ? 'Password copied. The clipboard is cleared in 30 seconds.' : 'Username copied', {
        icon: 'copy',
      });
  }

  async function edit(e: PasswordEntry) {
    const r = await withMaster('Edit password', (m) => vaultApi.reveal(e.id, m));
    if (r) editor = { entry: e, initial: { password: r.password, notes: r.notes } };
  }

  async function remove() {
    const e = removing;
    removing = null;
    if (!e) return;
    try {
      await vaultApi.remove([e.id]);
      ui.toast(`Password for ${e.site} deleted`, { icon: 'trash' });
      vault.revision++;
      void vault.refresh();
    } catch (err) {
      ui.toast(String(err), { icon: 'alert' });
    }
  }

  async function startImport() {
    try {
      const o = await vaultApi.importOpen();
      if (o) importing = o;
    } catch (e) {
      ui.toast(String(e), { icon: 'alert', timeout: 6000 });
    }
  }

  function exportCsv() {
    ask = {
      title: 'Export passwords',
      warn: "The exported file isn't encrypted: anyone who can open it can read every password in it. Import it where you need it, then delete it.",
      text: 'Enter your master password to export.',
      label: 'Export…',
      run: async (master) => {
        const path = await vaultApi.exportCsv(master);
        ask = null;
        if (path) ui.toast(`Passwords exported to ${path}`, { icon: 'download', timeout: 6000 });
      },
    };
  }

  function backup() {
    ask = {
      title: 'Back up passwords',
      text:
        'The backup is encrypted with your current master password and can be restored with Import. Remember that password: without it the backup can\'t be opened.',
      label: 'Save backup…',
      run: async (master) => {
        const path = await vaultApi.backup(master);
        ask = null;
        if (path) ui.toast(`Backup saved to ${path}`, { icon: 'save', timeout: 6000 });
      },
    };
  }

  function more(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    ui.openMenu(r.right - 220, r.bottom + 6, [
      { label: 'Import passwords…', icon: 'upload', run: () => void startImport() },
      { separator: true },
      { label: 'Back up (encrypted)…', icon: 'save', run: backup },
      { label: 'Export as CSV (not encrypted)…', icon: 'download', run: exportCsv },
      { separator: true },
      { label: 'Lock now', icon: 'lock', run: () => void vault.lock() },
    ]);
  }

  function rowMenu(ev: MouseEvent, e: PasswordEntry) {
    ev.preventDefault();
    ui.openMenu(ev.clientX, ev.clientY, [
      { label: 'Copy username', icon: 'user', run: () => void copy(e, 'username') },
      { label: 'Copy password', icon: 'copy', run: () => void copy(e, 'password') },
      { label: shown[e.id] !== undefined ? 'Hide password' : 'Show password', icon: 'eye', run: () => void toggleReveal(e) },
      { separator: true },
      { label: 'Edit…', icon: 'edit', run: () => void edit(e) },
      { label: 'Delete', icon: 'trash', danger: true, run: () => (removing = e) },
    ]);
  }
</script>

<div class="list-page">
  <div class="toolbar">
    <div class="search">
      <Icon name="search" size={15} />
      <input bind:value={query} placeholder="Search passwords" aria-label="Search passwords" spellcheck="false" />
      {#if query}<button class="icon-btn sm" title="Clear" onclick={() => (query = '')}><Icon name="close" size={13} /></button>{/if}
    </div>
    <button class="btn primary" onclick={() => (editor = { entry: null, initial: null })}><Icon name="plus" size={15} /> Add</button>
    <button class="icon-btn" title="More" onclick={more}><Icon name="more" size={16} /></button>
  </div>

  {#if !loading}
    <p class="count">
      {entries.length === 0 ? 'No saved passwords yet' : `${entries.length} ${entries.length === 1 ? 'password' : 'passwords'} on ${siteCount} ${siteCount === 1 ? 'site' : 'sites'}`}
      {#if query && entries.length}· {visible.length} shown{/if}
    </p>
  {/if}

  {#if !loading && entries.length === 0}
    <div class="empty">
      <div class="badge"><Icon name="key" size={24} /></div>
      <h3>Your passwords will appear here</h3>
      <p>Sign in to a site and Zenith offers to save the password. You can also add one yourself or import them from another browser or password manager.</p>
      <div class="row">
        <button class="btn" onclick={startImport}><Icon name="upload" size={15} /> Import</button>
        <button class="btn primary" onclick={() => (editor = { entry: null, initial: null })}><Icon name="plus" size={15} /> Add password</button>
      </div>
      <details>
        <summary>How do I export passwords from another browser?</summary>
        <ul>
          <li><b>Chrome, Brave, Opera, Vivaldi:</b> Settings › Passwords (or Google Password Manager) › Settings › Export passwords.</li>
          <li><b>Edge:</b> Settings › Passwords › ⋯ › Export passwords.</li>
          <li><b>Firefox:</b> Passwords (about:logins) › ⋯ › Export passwords.</li>
          <li><b>Bitwarden, 1Password, LastPass, Proton Pass, KeePass:</b> export as CSV.</li>
        </ul>
        <p>Then choose Import here and pick the CSV file. Delete the file afterwards: it isn't encrypted.</p>
      </details>
    </div>
  {:else if !loading && groups.length === 0}
    <p class="none">Nothing matches “{query}”.</p>
  {:else}
    <div class="groups">
      {#each groups as g (g.site)}
        <section class="group">
          <header>
            <Favicon url={g.origin} size={18} />
            <b class="ellipsis">{g.site}</b>
            {#if g.entries.length > 1}<span class="n">{g.entries.length} accounts</span>{/if}
          </header>
          {#each g.entries as e (e.id)}
            <div class="entry" role="row" tabindex="-1" oncontextmenu={(ev) => rowMenu(ev, e)}>
              <div class="who">
                <span class="user ellipsis">{e.username || 'No username'}</span>
                <span class="meta ellipsis">
                  {#if e.label}<span class="chip">{e.label}</span>{/if}
                  {e.lastUsed ? `Used ${timeAgo(e.lastUsed)}` : `Saved ${timeAgo(e.created)}`}
                  {#if e.hasNotes}· <Icon name="note" size={11} /> note{/if}
                </span>
              </div>
              <div class="secret" class:revealed={shown[e.id] !== undefined}>
                {#if shown[e.id] !== undefined}<span class="mono">{shown[e.id]}</span>{:else}<span class="dots">••••••••••</span>{/if}
              </div>
              <div class="acts">
                <button class="icon-btn sm" title={shown[e.id] !== undefined ? 'Hide password' : 'Show password'} onclick={() => toggleReveal(e)}>
                  <Icon name={shown[e.id] !== undefined ? 'eyeOff' : 'eye'} size={15} />
                </button>
                <button class="icon-btn sm" title="Copy username" onclick={() => copy(e, 'username')}><Icon name="user" size={15} /></button>
                <button class="icon-btn sm" title="Copy password" onclick={() => copy(e, 'password')}><Icon name="copy" size={15} /></button>
                <button class="icon-btn sm" title="Edit" onclick={() => edit(e)}><Icon name="edit" size={15} /></button>
                <button class="icon-btn sm del" title="Delete" onclick={() => (removing = e)}><Icon name="trash" size={15} /></button>
              </div>
            </div>
          {/each}
        </section>
      {/each}
    </div>
  {/if}
</div>

{#if editor}
  <EntryEditor
    entry={editor.entry}
    initial={editor.initial}
    onclose={() => (editor = null)}
    onsaved={() => {
      const added = !editor?.entry;
      editor = null;
      vault.revision++;
      void vault.refresh();
      ui.toast(added ? 'Password saved' : 'Password updated', { icon: 'check' });
    }}
  />
{/if}

{#if removing}
  <Modal title="Delete password?" onclose={() => (removing = null)} width={400}>
    <p class="confirm">
      The password for <b>{removing.username || 'this account'}</b> on <b>{removing.site}</b> will be deleted from Zenith. This can't be undone.
    </p>
    <div class="actions">
      <button class="btn" onclick={() => (removing = null)}>Cancel</button>
      <button class="btn danger" onclick={remove}><Icon name="trash" size={14} /> Delete</button>
    </div>
  </Modal>
{/if}

{#if importing}
  <ImportDialog opened={importing} onclose={() => (importing = null)} />
{/if}

{#if ask}
  <Modal title={ask.title} onclose={() => (ask = null)} width={420}>
    <div class="ask">
      {#if ask.warn}<p class="warnbox"><Icon name="alert" size={15} /> {ask.warn}</p>{/if}
      <p>{ask.text ?? 'Enter your master password to continue.'}</p>
      <MasterPasswordForm onsubmit={ask.run} submitLabel={ask.label} />
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
    gap: 8px;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 8px 0 12px;
    border-radius: var(--radius);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-3);
  }
  .search:focus-within {
    background: var(--field-focus);
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--text);
    font: inherit;
  }
  .count {
    margin: 14px 2px 10px;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .none {
    color: var(--text-3);
    padding: 20px 2px;
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .group {
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .group header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px 6px;
  }
  .group header b {
    font-weight: 600;
  }
  .n {
    margin-left: auto;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .entry {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr) auto;
    align-items: center;
    gap: 14px;
    min-height: 52px;
    padding: 6px 10px 6px 42px;
    border-top: 1px solid var(--line);
  }
  .entry:hover {
    background: var(--hover);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .user {
    font-weight: 500;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .chip {
    padding: 0 6px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent-fg);
  }
  .secret {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-3);
  }
  .secret.revealed {
    color: var(--text);
    user-select: text;
  }
  .dots {
    letter-spacing: 0.12em;
  }
  .mono {
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .acts {
    display: flex;
    gap: 2px;
    opacity: 0.55;
    transition: opacity var(--fast) ease;
  }
  .entry:hover .acts,
  .entry:focus-within .acts {
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
    margin-top: 8px;
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
  .row {
    display: flex;
    gap: 8px;
  }
  details {
    margin-top: 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  summary {
    cursor: pointer;
    color: var(--accent-fg);
  }
  details ul {
    margin: 8px 0;
    padding-left: 18px;
    line-height: 1.6;
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
  .ask {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .ask p {
    margin: 0;
    line-height: 1.5;
    color: var(--text-2);
  }
  .warnbox {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    color: var(--text) !important;
    font-size: 12.5px;
  }
  .warnbox :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--warn);
  }
</style>
