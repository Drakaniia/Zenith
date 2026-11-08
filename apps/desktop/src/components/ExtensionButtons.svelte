<script lang="ts">
  import Icon from './Icon.svelte';
  import { extensions } from '../lib/extensions.svelte';

  // Pinned extension icons (open the extension's popup) and the extensions menu button.
  // `maxPinned` keeps the narrow sidebar row from overflowing.
  let { maxPinned = 6, below = true }: { maxPinned?: number; below?: boolean } = $props();

  const shown = $derived(extensions.pinned.slice(0, maxPinned));

  function toggleMenu(e: MouseEvent) {
    // A click on this button while the menu is open first closes it through the menu's
    // click-catcher (pointerdown); the click that follows must not open it again.
    if (extensions.popover || extensions.justClosed()) {
      extensions.close();
      return;
    }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    void extensions.refresh();
    extensions.popover = below ? { x: r.right - 320, y: r.bottom + 6, below } : { x: r.left, y: r.top - 6, below };
  }

  /** Where an extension's popup opens: under (or above) its toolbar icon, like Chrome. */
  function anchorOf(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return below ? { x: r.left, y: r.bottom + 6, below } : { x: r.left, y: r.top - 6, below };
  }
</script>

{#each shown as ext (ext.id)}
  <button class="icon-btn ext" title={ext.name} onclick={(e) => extensions.open(ext, anchorOf(e))}>
    {#if ext.icon}<img src={ext.icon} alt="" width="16" height="16" />{:else}<Icon name="puzzle" size={15} />{/if}
  </button>
{/each}
<button class="icon-btn" class:on={!!extensions.popover} title="Extensions" data-action="extensions" onclick={toggleMenu}>
  <Icon name="puzzle" />
</button>

<style>
  .ext img {
    display: block;
    object-fit: contain;
  }
  .icon-btn.on {
    background: var(--element-bg);
  }
</style>
