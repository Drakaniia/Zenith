<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { masterProblem, strength } from './passwords';

  let {
    onsubmit,
    submitLabel = 'Create master password',
    autofocus = true,
  }: { onsubmit: (password: string) => Promise<void>; submitLabel?: string; autofocus?: boolean } = $props();

  let pw = $state('');
  let confirm = $state('');
  let show = $state(false);
  let busy = $state(false);
  let error = $state('');
  let touched = $state(false);
  let first: HTMLInputElement | undefined = $state();

  const s = $derived(strength(pw));
  const problem = $derived(masterProblem(pw, confirm));

  onMount(() => {
    if (autofocus) requestAnimationFrame(() => first?.focus());
  });

  async function submit(e: Event) {
    e.preventDefault();
    touched = true;
    if (problem || busy) return;
    busy = true;
    error = '';
    try {
      await onsubmit(pw);
      pw = confirm = '';
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<form class="new-master" onsubmit={submit}>
  <label class="lbl" for="nm-1">New master password</label>
  <div class="field-row">
    <input id="nm-1" bind:this={first} bind:value={pw} type={show ? 'text' : 'password'} autocomplete="new-password" spellcheck="false" disabled={busy} />
    <button type="button" class="icon-btn sm" title={show ? 'Hide' : 'Show'} onclick={() => (show = !show)}>
      <Icon name={show ? 'eyeOff' : 'eye'} size={15} />
    </button>
  </div>
  <div class="meter" data-score={s.score} aria-hidden={!pw}>
    {#each [0, 1, 2, 3] as i (i)}<span class:on={pw && s.score > i}></span>{/each}
    <em>{pw ? s.label : ''}</em>
  </div>
  <label class="lbl" for="nm-2">Type it again</label>
  <div class="field-row">
    <input id="nm-2" bind:value={confirm} type={show ? 'text' : 'password'} autocomplete="new-password" spellcheck="false" disabled={busy} />
  </div>
  {#if (touched || confirm.length >= pw.length) && pw && problem}
    <p class="error">{problem}</p>
  {:else if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  <p class="hint">Zenith can't show or reset this password for you. A long phrase you can remember works best.</p>
  <button class="btn primary" type="submit" disabled={busy || !pw || !confirm}>{busy ? 'Protecting your vault…' : submitLabel}</button>
</form>

<style>
  .new-master {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lbl {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
    margin-top: 4px;
  }
  .field-row {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 36px;
    padding: 0 6px 0 12px;
    border-radius: var(--radius);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .field-row:focus-within {
    background: var(--field-focus);
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: 0;
    background: none;
    color: var(--text);
    font: inherit;
  }
  .meter {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 16px;
  }
  .meter span {
    flex: 1;
    max-width: 52px;
    height: 4px;
    border-radius: 2px;
    background: var(--line-strong);
    transition: background var(--med) ease;
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
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--danger);
  }
  .hint {
    margin: 4px 0 6px;
    font-size: 12px;
    color: var(--text-3);
    line-height: 1.45;
  }
  .btn {
    align-self: flex-end;
  }
</style>
