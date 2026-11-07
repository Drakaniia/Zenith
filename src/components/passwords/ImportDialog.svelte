<script lang="ts">
  // Import preview: what each row of a CSV export or Zenith backup would do, with a choice for
  // accounts already saved with a different password. Nothing is saved until "Import".
  import { untrack } from 'svelte';
  import Icon from '../Icon.svelte';
  import Modal from './Modal.svelte';
  import MasterPasswordForm from './MasterPasswordForm.svelte';
  import type { ImportOpened, ImportPreviewRow } from '../../lib/passwords';
  import { ui } from '../../lib/ui.svelte';
  import { vault, vaultApi } from '../../lib/vault.svelte';

  let { opened, onclose }: { opened: ImportOpened; onclose: () => void } = $props();

  // The dialog belongs to one opened file; later updates come from importUnlock.
  const first = untrack(() => opened);
  let token = $state(first.token);
  let rows = $state<ImportPreviewRow[]>(first.rows);
  let needsPassword = $state(first.needsPassword);
  const fileName = first.fileName;
  /** Row index -> "add" | "skip" (new rows) or "skip" | "replace" | "both" (conflicts). */
  let choices = $state<Record<number, string>>({});
  let busy = $state(false);
  let done = false;

  const counts = $derived({
    new: rows.filter((r) => r.status === 'new').length,
    duplicate: rows.filter((r) => r.status === 'duplicate').length,
    conflict: rows.filter((r) => r.status === 'conflict').length,
    invalid: rows.filter((r) => r.status === 'invalid').length,
  });
  const adding = $derived(
    rows.filter((r) => (r.status === 'new' && choices[r.index] !== 'skip') || (r.status === 'conflict' && (choices[r.index] ?? 'skip') !== 'skip')).length,
  );

  /** A wrong password rejects, and the form shows why; the file stays open for another try. */
  async function unlockBackup(password: string) {
    const o = await vaultApi.importUnlock(token, password);
    token = o.token;
    rows = o.rows;
    needsPassword = false;
  }

  async function commit() {
    busy = true;
    try {
      const sent: Record<number, string> = {};
      for (const r of rows) {
        const c = choices[r.index];
        if (r.status === 'new' && c === 'skip') sent[r.index] = 'skip';
        if (r.status === 'conflict' && c) sent[r.index] = c;
      }
      const sum = await vaultApi.importCommit(token, sent);
      done = true;
      vault.revision++;
      void vault.refresh();
      const parts = [`${sum.added} added`];
      if (sum.replaced) parts.push(`${sum.replaced} updated`);
      if (sum.skipped) parts.push(`${sum.skipped} skipped`);
      ui.toast(`Passwords imported: ${parts.join(', ')}`, { icon: 'check', timeout: 5000 });
      onclose();
    } catch (e) {
      ui.toast(String(e), { icon: 'alert' });
    } finally {
      busy = false;
    }
  }

  function cancel() {
    if (!done) void vaultApi.importCancel(token).catch(() => {});
    onclose();
  }

  const STATUS: Record<string, string> = { new: 'New', duplicate: 'Already saved', conflict: 'Different password', invalid: 'Skipped' };
</script>

<Modal title="Import passwords" onclose={cancel} width={620}>
  {#if needsPassword}
    <div class="backup">
      <p><Icon name="lock" size={15} /> <b>{fileName}</b> is an encrypted Zenith backup.</p>
      <p class="muted">Enter the master password you had when you made it.</p>
      <MasterPasswordForm onsubmit={unlockBackup} submitLabel="Open backup" forgot={false} />
    </div>
  {:else}
    <p class="summary">
      {#if fileName}<b>{fileName}</b>: {/if}{rows.length} {rows.length === 1 ? 'entry' : 'entries'} ·
      <span class="ok">{counts.new} new</span>
      {#if counts.conflict}· <span class="warn">{counts.conflict} with a different password</span>{/if}
      {#if counts.duplicate}· {counts.duplicate} already saved{/if}
      {#if counts.invalid}· {counts.invalid} can't be imported{/if}
    </p>
    {#if counts.conflict}
      <p class="muted small">For accounts you've already saved with a different password, choose which password to keep.</p>
    {/if}
    <div class="rows">
      {#each rows as r (r.index)}
        <div class="r" class:dim={r.status === 'duplicate' || r.status === 'invalid'}>
          {#if r.status === 'new'}
            <input
              type="checkbox"
              checked={choices[r.index] !== 'skip'}
              aria-label="Import {r.site}"
              onchange={(e) => (choices[r.index] = (e.currentTarget as HTMLInputElement).checked ? 'add' : 'skip')}
            />
          {:else}
            <span class="spacer"></span>
          {/if}
          <span class="who">
            <b class="ellipsis">{r.site || '(no address)'}</b>
            <small class="ellipsis">{r.username || 'No username'}{r.label ? ` · ${r.label}` : ''}</small>
          </span>
          {#if r.status === 'conflict'}
            <select class="field select" value={choices[r.index] ?? 'skip'} onchange={(e) => (choices[r.index] = (e.currentTarget as HTMLSelectElement).value)}>
              <option value="skip">Keep saved password</option>
              <option value="replace">Use imported password</option>
              <option value="both">Keep both</option>
            </select>
          {:else}
            <span class="tag {r.status}" title={r.reason ?? ''}>{r.status === 'invalid' ? (r.reason ?? STATUS.invalid) : STATUS[r.status]}</span>
          {/if}
        </div>
      {/each}
    </div>
    <div class="actions">
      <button class="btn" onclick={cancel}>Cancel</button>
      <button class="btn primary" disabled={busy || adding === 0} onclick={commit}>
        {busy ? 'Importing…' : `Import ${adding} ${adding === 1 ? 'password' : 'passwords'}`}
      </button>
    </div>
  {/if}
</Modal>

<style>
  .backup {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .backup p {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .summary {
    margin: 0 0 6px;
  }
  .small {
    font-size: 12.5px;
    margin: 0 0 8px;
  }
  .ok {
    color: var(--ok);
  }
  .warn {
    color: var(--warn);
  }
  .rows {
    display: flex;
    flex-direction: column;
    max-height: 340px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
  }
  .r {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 46px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--line);
  }
  .r:last-child {
    border-bottom: 0;
  }
  .r.dim {
    opacity: 0.55;
  }
  .spacer {
    width: 15px;
  }
  .r input[type='checkbox'] {
    width: 15px;
    height: 15px;
    accent-color: var(--accent);
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .who b {
    font-weight: 500;
  }
  .who small {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .select {
    height: 30px;
    padding: 0 6px;
  }
  .tag {
    flex: none;
    font-size: 11.5px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--field);
    color: var(--text-2);
  }
  .tag.new {
    color: var(--ok);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
</style>
