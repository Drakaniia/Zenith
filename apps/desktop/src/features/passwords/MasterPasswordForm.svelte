<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { openInternal } from '$features/browser/target';

  let {
    onsubmit,
    submitLabel = 'Unlock',
    autofocus = true,
    forgot = true,
    compact = false,
  }: {
    /** Throws (or rejects) with the message to show. */
    onsubmit: (password: string) => Promise<void>;
    submitLabel?: string;
    autofocus?: boolean;
    /** Show "Forgot master password?". */
    forgot?: boolean;
    compact?: boolean;
  } = $props();

  let value = $state('');
  let show = $state(false);
  let busy = $state(false);
  let error = $state('');
  let input: HTMLInputElement | undefined = $state();

  onMount(() => {
    if (autofocus) requestAnimationFrame(() => input?.focus());
  });

  async function submit(e: Event) {
    e.preventDefault();
    if (!value || busy) return;
    busy = true;
    error = '';
    try {
      await onsubmit(value);
      value = '';
    } catch (err) {
      error = String(err);
      input?.select();
    } finally {
      busy = false;
    }
  }
</script>

<form class="master" class:compact onsubmit={submit}>
  <div class="field-row" class:bad={!!error}>
    <Icon name="lock" size={15} />
    <input
      bind:this={input}
      bind:value
      type={show ? 'text' : 'password'}
      autocomplete="off"
      spellcheck="false"
      placeholder="Master password"
      aria-label="Master password"
      aria-invalid={!!error}
      disabled={busy}
    />
    <button type="button" class="icon-btn sm" title={show ? 'Hide' : 'Show'} onclick={() => (show = !show)}>
      <Icon name={show ? 'eyeOff' : 'eye'} size={15} />
    </button>
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="actions">
    {#if forgot}
      <button type="button" class="link" onclick={() => openInternal('passwords', 'forgot')}>Forgot master password?</button>
    {/if}
    <button class="btn primary" type="submit" disabled={!value || busy}>{busy ? 'Checking…' : submitLabel}</button>
  </div>
</form>

<style>
  .master {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
  }
  .field-row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 6px 0 12px;
    border-radius: var(--radius);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-2, var(--text));
  }
  .compact .field-row {
    height: 34px;
  }
  .field-row:focus-within {
    background: var(--field-focus);
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .field-row.bad {
    border-color: color-mix(in srgb, var(--danger) 70%, transparent);
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
  .error {
    margin: -4px 0 0;
    font-size: 12px;
    color: var(--danger);
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }
  .link {
    margin-right: auto;
    padding: 0;
    font-size: 12px;
    color: var(--accent-fg);
  }
  .link:hover {
    text-decoration: underline;
  }
</style>
