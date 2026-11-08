<script lang="ts">
  import { onboarding } from '../../../lib/onboarding.svelte';

  const parts = ['How it looks', 'Where it searches', 'What opens first', 'Your old bookmarks', 'Privacy'];
</script>

<ul class="parts" aria-label="What you'll set up">
  {#each parts as p, i (p)}
    <li style:--i={i}>{p}</li>
  {/each}
</ul>

<p class="later">
  Already know how you like it?
  <button type="button" class="link" onclick={() => onboarding.finish()}>Skip setup and use the defaults</button>
</p>

<style>
  .parts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
  }
  .parts li {
    padding: 5px 11px;
    border-radius: 999px;
    font-size: 12.5px;
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    animation: chip 380ms cubic-bezier(0.23, 1, 0.32, 1) both;
    animation-delay: calc(180ms + var(--i) * 45ms);
  }
  @keyframes chip {
    from {
      opacity: 0;
      translate: 0 6px;
    }
  }
  .later {
    margin: 28px 0 0;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .link {
    color: var(--accent-fg);
    font-weight: 500;
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, currentColor 35%, transparent);
    text-underline-offset: 3px;
    border-radius: 3px;
  }
  .link:hover {
    text-decoration-color: currentColor;
  }
</style>
