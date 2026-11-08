<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { browser } from '$features/browser/browser.svelte';

  // Zen's "Workspaces" field of the bookmark panel: the workspaces a bookmark shows in on the
  // toolbar. None picked ("-") means every workspace.
  let { value = $bindable() }: { value: string[] } = $props();

  let open = $state(false);

  const live = $derived(new Set(browser.spaces.map((s) => s.id)));
  const picked = $derived(value.filter((id) => live.has(id)));
  const summary = $derived(
    picked.length ? browser.spaces.filter((s) => picked.includes(s.id)).map((s) => s.name).join(', ') : '-',
  );

  function toggle(id: string) {
    // Ids of workspaces that were deleted are dropped on the first change.
    value = picked.includes(id) ? picked.filter((x) => x !== id) : [...picked, id];
  }
</script>

<div class="spaces">
  <div class="row">
    <button type="button" class="select" aria-expanded={open} onclick={() => (open = !open)}>
      <span class="name ellipsis">{summary}</span>
    </button>
    <button
      type="button"
      class="expander"
      class:on={open}
      title={open ? 'Hide the workspaces' : 'Pick workspaces'}
      aria-label={open ? 'Hide the workspaces' : 'Pick workspaces'}
      onclick={() => (open = !open)}
    >
      <Icon name={open ? 'chevronUp' : 'chevronDown'} size={15} />
    </button>
  </div>
  {#if open}
    <div class="checks">
      {#each browser.spaces as s (s.id)}
        <label class="check">
          <input type="checkbox" checked={picked.includes(s.id)} onchange={() => toggle(s.id)} />
          <span class="emoji" aria-hidden="true">{s.icon}</span>
          <span class="ellipsis">{s.name}</span>
        </label>
      {/each}
      <p class="hint">Show this bookmark on the toolbar only in the ticked workspaces. With none ticked it shows in all of them.</p>
    </div>
  {/if}
</div>

<style>
  .spaces {
    min-width: 0;
    flex: 1;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .select,
  .expander {
    height: 32px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
    transition: background var(--fast);
  }
  .select {
    flex: 1;
    min-width: 0;
    padding: 0 10px;
    text-align: left;
    font-size: 13px;
  }
  .name {
    display: block;
  }
  .expander {
    flex: none;
    width: 34px;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  .select:hover,
  .expander:hover,
  .expander.on {
    background: var(--field-focus);
  }
  .checks {
    margin-top: 8px;
    padding: 6px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    padding: 0 6px;
    border-radius: var(--r-item);
    font-size: 13px;
  }
  .check:hover {
    background: var(--hover);
  }
  .check input {
    accent-color: var(--accent);
  }
  .emoji {
    width: 18px;
    text-align: center;
  }
  .hint {
    margin: 6px 6px 2px;
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--text-3);
  }
</style>
