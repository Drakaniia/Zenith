<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { shell } from '$platform/shell';
  import { ui } from '$shared/ui/ui.svelte';

  let { value, ondone, doneLabel = 'Done' }: { value: string; ondone: () => void; doneLabel?: string } = $props();
  let stored = $state(false);

  async function copy() {
    try {
      await shell.clipboardWrite(value);
      ui.toast('Recovery key copied. Store it somewhere safe, then clear your clipboard.', { icon: 'copy', timeout: 5000 });
    } catch (e) {
      ui.toast(`Could not copy: ${e}`, { icon: 'alert' });
    }
  }
</script>

<div class="recovery">
  <div class="head">
    <Icon name="key" size={18} />
    <b>Your recovery key</b>
  </div>
  <p>
    If you forget your master password, this key is the only way to set a new one without losing your saved passwords.
    Write it down or keep it in a safe place. Zenith won't show it again.
  </p>
  <code class="key" aria-label="Recovery key">{value}</code>
  <div class="row">
    <button class="btn" onclick={copy}><Icon name="copy" size={14} /> Copy</button>
  </div>
  <label class="check"><input type="checkbox" bind:checked={stored} /> I've stored my recovery key somewhere safe</label>
  <button class="btn primary" disabled={!stored} onclick={ondone}>{doneLabel}</button>
</div>

<style>
  .recovery {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--accent-fg);
  }
  .head b {
    color: var(--text);
    font: 600 15px/1.3 var(--font-display);
  }
  p {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-2);
  }
  .key {
    display: block;
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--field);
    border: 1px dashed var(--line-strong);
    font: 600 15px/1.6 var(--mono);
    letter-spacing: 0.04em;
    text-align: center;
    word-break: break-all;
    user-select: all;
  }
  .row {
    display: flex;
    justify-content: flex-end;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
  }
  .check input {
    accent-color: var(--accent);
  }
  .btn.primary {
    align-self: flex-end;
  }
</style>
