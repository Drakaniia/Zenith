<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { extensions } from './extensions.svelte';
  import { extensionMenu } from './menus';
  import type { ExtensionInfo } from './types';

  // Pinned extension icons (open the extension's popup / action) and the extensions menu button.
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
    extensions.popover = below ? { x: r.right - 340, y: r.bottom + 6, below } : { x: r.left, y: r.top - 6, below };
  }

  /** Where an extension's popup opens: under (or above) its toolbar icon, like Chrome & Zen. */
  function anchorOf(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return below ? { x: r.left, y: r.bottom + 6, below } : { x: r.left, y: r.top - 6, below };
  }

  function openContextMenu(e: MouseEvent, ext: ExtensionInfo) {
    e.preventDefault();
    e.stopPropagation();
    extensions.close();
    ui.openMenu(e.clientX, e.clientY, extensionMenu(ext, anchorOf(e)));
  }
</script>

{#each shown as ext (ext.id)}
  {@const badge = extensions.badgeFor(ext.id, browser.activeTab?.id)}
  <button
    class="icon-btn ext"
    class:on={extensions.activePopup?.extId === ext.id}
    title={ext.name}
    data-ext-id={ext.id}
    onclick={(e) => void extensions.open(ext, anchorOf(e))}
    oncontextmenu={(e) => openContextMenu(e, ext)}
  >
    {#if ext.icon}<img src={ext.icon} alt="" width="16" height="16" />{:else}<Icon name="puzzle" size={15} />{/if}
    {#if badge?.text}
      <span
        class="ext-badge"
        style:background={badge.backgroundColor ?? undefined}
        style:color={badge.textColor ?? undefined}
      >{badge.text}</span>
    {/if}
  </button>
{/each}
<button class="icon-btn" class:on={!!extensions.popover} title="Extensions" data-action="extensions" onclick={toggleMenu}>
  <Icon name="puzzle" />
</button>

<style>
  .ext {
    position: relative;
  }
  .ext img {
    display: block;
    object-fit: contain;
  }
  .ext-badge {
    position: absolute;
    right: 1px;
    bottom: 1px;
    min-width: 12px;
    height: 12px;
    padding: 0 3px;
    border-radius: 6px;
    background: var(--accent);
    color: #fff;
    font: 700 8.5px/12px var(--font-sans, system-ui);
    text-align: center;
    pointer-events: none;
  }
  .icon-btn.on {
    background: var(--element-bg);
  }
</style>
