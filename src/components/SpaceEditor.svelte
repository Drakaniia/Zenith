<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { tick } from 'svelte';
  import { browser, SPACE_COLORS, SPACE_ICONS } from '../lib/browser.svelte';
  import { ui } from '../lib/ui.svelte';
  import { overlayLease } from '../lib/layout.svelte';

  const editing = $derived(ui.spaceEditor);
  const existing = $derived(editing?.spaceId ? browser.space(editing.spaceId) : undefined);

  let name = $state('');
  let icon = $state(SPACE_ICONS[0]);
  let color = $state(SPACE_COLORS[0]);
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (!editing) return;
    const s = existing;
    const n = browser.spaces.length;
    name = s?.name ?? '';
    icon = s?.icon ?? SPACE_ICONS[n % SPACE_ICONS.length];
    color = s?.color ?? SPACE_COLORS[n % SPACE_COLORS.length];
    void tick().then(() => input?.focus());
  });

  function save() {
    if (existing) {
      existing.name = name.trim() || existing.name;
      existing.icon = icon;
      existing.color = color;
    } else {
      const s = browser.createSpace(name.trim() || undefined, icon, color);
      browser.switchSpace(s.id);
    }
    ui.spaceEditor = null;
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (editing && e.key === 'Escape') ui.spaceEditor = null;
  }}
/>

{#if editing}
  <div class="root" use:overlayLease>
    <div class="backdrop" role="presentation" transition:fade={{ duration: 120 }} onpointerdown={() => (ui.spaceEditor = null)}></div>
    <form
      class="dialog glass"
      transition:scale={{ start: 0.96, duration: 150, opacity: 0 }}
      onsubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <h3>{existing ? 'Edit space' : 'New space'}</h3>
      <div class="preview" style:--c={color}>
        <span class="pi">{icon}</span>
        <input class="field" bind:this={input} bind:value={name} placeholder="Space name" maxlength="40" />
      </div>
      <div class="label">Icon</div>
      <div class="icons">
        {#each SPACE_ICONS as i (i)}
          <button type="button" class="pick" class:on={icon === i} onclick={() => (icon = i)}>{i}</button>
        {/each}
      </div>
      <div class="label">Color</div>
      <div class="colors">
        {#each SPACE_COLORS as c (c)}
          <button type="button" class="swatch" class:on={color === c} style:--c={c} aria-label={`Color ${c}`} onclick={() => (color = c)}></button>
        {/each}
      </div>
      <div class="actions">
        <button type="button" class="btn ghost" onclick={() => (ui.spaceEditor = null)}>Cancel</button>
        <button type="submit" class="btn primary">{existing ? 'Save' : 'Create space'}</button>
      </div>
    </form>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 88;
    display: grid;
    place-items: center;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    background: var(--backdrop);
  }
  .dialog {
    position: relative;
    width: 380px;
    padding: 20px;
    border-radius: var(--radius-xl);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h3 {
    margin: 0 0 4px;
    font: 600 16px/1.2 var(--font-display);
  }
  .preview {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .pi {
    width: 40px;
    height: 40px;
    flex: none;
    display: grid;
    place-items: center;
    font-size: 20px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--c) 28%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--c) 60%, transparent);
  }
  .preview .field {
    flex: 1;
    height: 40px;
    font-size: 14px;
  }
  .label {
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-3);
  }
  .icons {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 4px;
  }
  .pick {
    height: 36px;
    border-radius: 9px;
    font-size: 17px;
  }
  .pick:hover {
    background: var(--hover);
  }
  .pick.on {
    background: var(--selected);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .colors {
    display: flex;
    gap: 8px;
  }
  .swatch {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--c);
    transition: transform var(--fast);
  }
  .swatch:hover {
    transform: scale(1.1);
  }
  .swatch.on {
    box-shadow:
      0 0 0 2px var(--surface-solid),
      0 0 0 4px var(--c);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
</style>
