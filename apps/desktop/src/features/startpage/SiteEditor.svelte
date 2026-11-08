<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { untrack } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { startPage } from './startpage.svelte';
  import { SITE_TITLE_MAX } from './startpage';

  // Add or edit a site saved to the Tableau. Opened through startPage.editing.
  const initial = untrack(() => startPage.editing!);
  const editing = !!initial.id;
  let url = $state(initial.url);
  let title = $state(initial.title);
  let error = $state('');
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    input?.focus();
    input?.select();
  });

  function submit(e: SubmitEvent) {
    e.preventDefault();
    error = startPage.saveSite({ url, title });
    if (error) input?.focus();
  }

  function cancel() {
    startPage.editing = null;
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape') return;
    e.stopPropagation();
    cancel();
  }}
/>

<div class="scrim" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && cancel()} transition:fade={{ duration: 140 }}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="site-editor-title"
    in:scale={{ start: 0.96, duration: 180, easing: cubicOut }}
    out:scale={{ start: 0.98, duration: 110 }}
  >
  <form onsubmit={submit}>
    <header>
      <h2 id="site-editor-title">{editing ? 'Edit site' : 'Add a site'}</h2>
      <button type="button" class="icon-btn sm" aria-label="Close" title="Close" onclick={cancel}><Icon name="close" size={14} /></button>
    </header>
    <label>
      <span>Address</span>
      <input
        bind:this={input}
        class="field"
        type="text"
        inputmode="url"
        spellcheck="false"
        autocomplete="off"
        placeholder="example.com"
        bind:value={url}
        oninput={() => (error = '')}
        aria-invalid={!!error}
        aria-describedby={error ? 'site-editor-error' : undefined}
      />
    </label>
    <label>
      <span>Name <small>(optional)</small></span>
      <input class="field" type="text" maxlength={SITE_TITLE_MAX} autocomplete="off" placeholder="Shown under the tile" bind:value={title} />
    </label>
    {#if error}<p id="site-editor-error" class="error" role="alert">{error}</p>{/if}
    <footer>
      <button type="button" class="btn ghost" onclick={cancel}>Cancel</button>
      <button type="submit" class="btn primary" disabled={!url.trim()}>{editing ? 'Save' : 'Add site'}</button>
    </footer>
  </form>
  </div>
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(0, 0, 0, 0.28);
  }
  .dialog {
    width: min(360px, 100%);
    padding: 14px 16px 16px;
    border-radius: 14px;
    corner-shape: var(--sq);
    background: var(--dialog-bg);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow-lg);
    color: var(--text);
    font-size: 13px;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font: 600 14px/1.3 var(--font);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  label > span {
    font-size: 12px;
    color: var(--text-2);
  }
  small {
    color: var(--text-3);
  }
  .field {
    height: 34px;
    padding: 0 10px;
    font-size: 13px;
  }
  .error {
    margin: -4px 0 0;
    font-size: 12px;
    color: var(--danger);
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 2px;
  }
</style>
