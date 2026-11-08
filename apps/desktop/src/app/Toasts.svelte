<script lang="ts">
  import { flip } from 'svelte/animate';
  import { scale } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { overlayHold } from '$platform/layout.svelte';
  import { springEase } from '$shared/ui/motion';
  import { isStrip } from '$features/sidebar/sidebar-position';

  // Zen's toasts (#zen-toast-container). With the sidebar visible they sit over the sidebar
  // (pixels the chrome owns), so the page stays clickable; in compact mode, where the sidebar is
  // hidden, they float at the top right of the window like Zen's and raise the chrome above the
  // page (a hold, not a lease: a passing toast doesn't dim the page the way a menu does). A tab
  // strip (sidebar at the top or bottom) has no room for them either.
  const floating = $derived(settings.value.compact || isStrip(settings.value.sidebarPosition));
</script>

{#snippet list()}
  {#each ui.toasts as t (t.id)}
    <div class="toast" role="status" animate:flip={{ duration: 200, easing: springEase }} transition:scale={{ start: 0.85, duration: 200, opacity: 0, easing: springEase }}>
      {#if t.icon}<span class="icon"><Icon name={t.icon} size={15} /></span>{/if}
      <span class="text">{t.text}</span>
      {#if t.action}
        <button
          class="act"
          onclick={() => {
            t.action?.run();
            ui.dismiss(t.id);
          }}>{t.action.label}</button
        >
      {/if}
      {#if t.timeout <= 0}
        <button class="close" aria-label="Dismiss" title="Dismiss" onclick={() => ui.dismiss(t.id)}><Icon name="close" size={14} /></button>
      {/if}
    </div>
  {/each}
{/snippet}

{#if ui.toasts.length}
  {#if floating}
    <div class="toasts floating" use:overlayHold>{@render list()}</div>
  {:else}
    <div class="toasts">{@render list()}</div>
  {/if}
{/if}

<style>
  .toasts {
    position: fixed;
    left: calc(var(--zen-pad) + 2px);
    bottom: 52px;
    width: calc(var(--sidebar-w) - 14px);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    z-index: 70;
    pointer-events: none;
  }
  :global(.app.side-right) .toasts {
    left: auto;
    right: calc(var(--zen-pad) + 2px);
    align-items: flex-end;
  }
  .toasts.floating {
    left: auto;
    right: max(4px, var(--zen-sep));
    top: max(4px, var(--zen-sep));
    bottom: auto;
    width: auto;
    align-items: flex-end;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 100%;
    min-height: 42px;
    padding: 6px 6px 6px 12px;
    border-radius: 14px;
    corner-shape: var(--sq);
    font-size: 13px;
    font-weight: 600;
    color: #fff;
    background: linear-gradient(to bottom, var(--accent-button) -40%, color-mix(in srgb, var(--accent-button), #0f0f0f 20%));
    box-shadow: rgba(0, 0, 0, 0.4) 0 0 22px 2px;
    transform-origin: top center;
    /* Toasts sit over the sidebar; on a short window they cover its buttons, so let clicks go
       through to what is underneath. Only an action button (Pin, Update…) takes them. */
    pointer-events: none;
  }
  :global(:root[data-theme='light']) .toast {
    box-shadow: rgba(0, 0, 0, 0.1) 0 0 22px 2px;
  }
  .icon {
    display: grid;
    opacity: 0.85;
  }
  .text {
    flex: 1;
    min-width: 0;
    padding-inline-end: 6px;
  }
  .act {
    flex: none;
    pointer-events: auto;
    min-height: 28px;
    padding: 0 10px;
    border-radius: 10px;
    border-top: 2px solid rgba(255, 255, 255, 0.1);
    font-size: 12px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.85);
    background: rgba(255, 255, 255, 0.12);
    box-shadow: 0 0 1px 1px rgba(0, 0, 0, 0.05);
  }
  .act:hover {
    background: rgba(255, 255, 255, 0.2);
  }
  .close {
    flex: none;
    pointer-events: auto;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 10px;
    color: rgba(255, 255, 255, 0.7);
  }
  .close:hover {
    color: #fff;
    background: rgba(255, 255, 255, 0.12);
  }
</style>
