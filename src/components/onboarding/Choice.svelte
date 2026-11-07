<script lang="ts" generics="T extends string | number | boolean">
  // A radio button drawn as a card: an optional picture, a title and a line of description.
  // A real <input type="radio">, so arrow keys move within its group and screen readers
  // announce it as one of a set.
  import type { Snippet } from 'svelte';

  let {
    name,
    value,
    selected,
    title,
    desc = '',
    onselect,
    art,
    children,
  }: {
    name: string;
    value: T;
    selected: T;
    title: string;
    desc?: string;
    onselect: (v: T) => void;
    art?: Snippet;
    /** Extra content shown inside the card while it is selected. */
    children?: Snippet;
  } = $props();

  const on = $derived(selected === value);
  const uid = $props.id();
</script>

<label class="choice" class:on class:has-art={!!art}>
  <input
    type="radio"
    {name}
    value={String(value)}
    checked={on}
    aria-labelledby="{uid}-t"
    aria-describedby={desc ? `${uid}-d` : undefined}
    onchange={() => onselect(value)}
  />
  {#if art}<span class="art" aria-hidden="true">{@render art()}</span>{/if}
  <span class="text">
    <b id="{uid}-t">{title}</b>
    {#if desc}<span class="desc" id="{uid}-d">{desc}</span>{/if}
  </span>
  <span class="tick" aria-hidden="true">
    <svg viewBox="0 0 12 12" width="10" height="10"><path d="M2.5 6.2 5 8.6l4.6-5" /></svg>
  </span>
  {#if on && children}<span class="extra">{@render children()}</span>{/if}
</label>

<style>
  .choice {
    position: relative;
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: start;
    align-content: start;
    gap: 10px;
    padding: 12px 14px;
    border-radius: var(--radius-lg);
    corner-shape: var(--sq);
    background: color-mix(in srgb, var(--text) 3.5%, transparent);
    box-shadow: inset 0 0 0 1px var(--line);
    transition:
      background-color 150ms ease,
      box-shadow 150ms ease,
      scale 160ms ease-out;
  }
  .choice.has-art .art {
    grid-column: 1 / -1;
  }
  @media (hover: hover) and (pointer: fine) {
    .choice:hover {
      background: color-mix(in srgb, var(--text) 6%, transparent);
    }
  }
  .choice:active {
    scale: 0.985;
  }
  .choice.on {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--accent) 80%, transparent);
  }
  .choice:has(input:focus-visible) {
    outline: 2px solid color-mix(in srgb, var(--accent) 80%, transparent);
    outline-offset: 2px;
  }
  input {
    position: absolute;
    opacity: 0;
    width: 1px;
    height: 1px;
    margin: 0;
    pointer-events: none;
  }
  .art {
    display: block;
    color: var(--text);
  }
  .text {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  b {
    font-weight: 600;
    font-size: 13.5px;
  }
  .desc {
    color: var(--text-2);
    font-size: 12.5px;
    line-height: 1.4;
  }
  .tick {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    margin-top: 1px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.5px var(--line-strong);
    transition:
      background-color 150ms ease,
      box-shadow 150ms ease;
  }
  .tick svg {
    fill: none;
    stroke: #fff;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
    opacity: 0;
    scale: 0.6;
    transition:
      opacity 150ms ease,
      scale 200ms cubic-bezier(0.23, 1, 0.32, 1);
  }
  .on .tick {
    background: var(--accent-button);
    box-shadow: none;
  }
  .on .tick svg {
    opacity: 1;
    scale: 1;
  }
  .extra {
    grid-column: 1 / -1;
    display: block;
  }
</style>
