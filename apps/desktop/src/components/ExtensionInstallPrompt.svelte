<script lang="ts">
  import { fly } from 'svelte/transition';
  import Icon from './Icon.svelte';
  import { extensions } from '../lib/extensions.svelte';
  import { overlayLease } from '../lib/layout.svelte';

  // Chrome's "Add <extension>?" dialog for extensions found in pages: one offer at a time.
  const offer = $derived(extensions.offers[0]);
</script>

{#if offer}
  {#key offer.token}
    <div class="prompt glass" use:overlayLease transition:fly={{ y: -10, duration: 160 }} role="alertdialog" aria-label="Add extension">
      <div class="icon">
        {#if offer.icon}<img src={offer.icon} alt="" width="32" height="32" />{:else}<Icon name="puzzle" size={22} />{/if}
      </div>
      <div class="body">
        <p class="title">{offer.replaces ? 'Update' : 'Add'} <strong>{offer.name}</strong>?</p>
        <p class="meta">
          {offer.source}{#if offer.version} · version {offer.version}{/if}{#if offer.replaces} · replaces {offer.replaces}{/if}
        </p>
        {#if offer.warnings.length}
          <p class="can">It can:</p>
          <ul>
            {#each offer.warnings as w (w)}<li>{w}</li>{/each}
          </ul>
        {/if}
        {#if offer.replaces}
          <p class="note">Reinstalling may reset this extension's settings.</p>
        {/if}
      </div>
      <div class="actions">
        <button class="btn" onclick={() => extensions.decline(offer)}>Cancel</button>
        <button class="btn primary" onclick={() => void extensions.accept(offer)}>{offer.replaces ? 'Update extension' : 'Add extension'}</button>
      </div>
    </div>
  {/key}
{/if}

<style>
  .prompt {
    position: fixed;
    top: calc(var(--topbar) + 14px);
    left: 50%;
    transform: translateX(-50%);
    z-index: 76;
    width: 400px;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 12px 14px;
    padding: 16px;
    border-radius: var(--radius-xl);
  }
  .icon {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: var(--surface);
    border: 1px solid var(--line);
    color: var(--accent);
  }
  .body {
    min-width: 0;
  }
  .title {
    margin: 2px 0 2px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .meta {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--text-3);
  }
  .can {
    margin: 0 0 4px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  ul {
    margin: 0;
    padding-left: 18px;
    font-size: 12.5px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .note {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
