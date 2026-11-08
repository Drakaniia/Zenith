<script lang="ts">
  import { icons, type IconName } from './icons';

  let {
    name,
    size = 16,
    stroke = 2,
    fill = false,
    class: className = '',
  }: { name: IconName; size?: number; stroke?: number; fill?: boolean; class?: string } = $props();
</script>

<svg
  class={className}
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={fill ? 'currentColor' : 'none'}
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#each icons[name] as d (d)}
    <path {d} />
  {/each}
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
