<script lang="ts" generics="T extends string | number">
  let {
    value = $bindable(),
    options,
    onchange,
  }: { value: T; options: { value: T; label: string }[]; onchange?: (v: T) => void } = $props();
</script>

<div class="seg" role="radiogroup">
  {#each options as o (o.value)}
    <button
      type="button"
      role="radio"
      aria-checked={value === o.value}
      class:on={value === o.value}
      onclick={() => {
        value = o.value;
        onchange?.(o.value);
      }}>{o.label}</button
    >
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    flex: none;
    padding: 3px;
    gap: 2px;
    border-radius: 9px;
    background: var(--field);
    border: 1px solid var(--line);
  }
  button {
    height: 26px;
    padding: 0 12px;
    border-radius: 6px;
    font-size: 12.5px;
    white-space: nowrap;
    color: var(--text-2);
    transition: background var(--fast), color var(--fast);
  }
  button:hover {
    color: var(--text);
  }
  button.on {
    background: var(--selected);
    color: var(--text);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15), 0 0 0 1px var(--line);
  }
</style>
