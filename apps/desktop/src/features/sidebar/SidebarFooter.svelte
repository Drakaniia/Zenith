<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { downloads } from '$features/downloads/downloads.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { updates } from '$features/updates/updates.svelte';
  import { dnd } from './sidebar-dnd.svelte';
  import { createNewMenu, spaceMenu } from '../../app/menus';
  import { openNotes, withHint } from '../../app/actions';
  import { settings } from '$features/settings/settings.svelte';
  import { internalPage } from '$shared/url';

  const notesKey = $derived(settings.value.shortcuts.openNotes?.[0]);
  const onNotes = $derived(internalPage(browser.activeTab?.url ?? '') === 'notes');

  // Zen's #zen-sidebar-foot-buttons: downloads, the space switcher and the "+" create button.
  // `inline`: part of the horizontal tab strip (sidebar at the top or bottom of the window).
  let { collapsed = false, inline = false }: { collapsed?: boolean; inline?: boolean } = $props();

  // Space buttons can be dragged sideways to reorder spaces (Zen's reorder mode).
  let reorder = $state<{ id: string; startX: number; moved: boolean } | null>(null);

  function startReorder(e: PointerEvent, id: string) {
    if (e.button !== 0) return;
    reorder = { id, startX: e.clientX, moved: false };
    const move = (ev: PointerEvent) => {
      if (!reorder) return;
      if (!reorder.moved && Math.abs(ev.clientX - reorder.startX) < 6 && Math.abs(ev.clientY - e.clientY) < 6) return;
      reorder.moved = true;
      const over = document.elementsFromPoint(ev.clientX, ev.clientY).find((el) => el instanceof HTMLElement && el.dataset.spaceBtn);
      const target = (over as HTMLElement | undefined)?.dataset.spaceBtn;
      if (target && target !== reorder.id) {
        browser.moveSpace(reorder.id, browser.spaces.findIndex((s) => s.id === target));
      }
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      const r = reorder;
      reorder = null;
      if (r?.moved) {
        const stop = (ev: Event) => (ev.stopPropagation(), ev.preventDefault());
        window.addEventListener('click', stop, { capture: true, once: true });
        setTimeout(() => window.removeEventListener('click', stop, { capture: true }), 50);
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  const many = $derived(browser.spaces.length > (collapsed ? 99 : 6));
</script>

<footer class="foot" class:collapsed class:inline>
  <button class="icon-btn dl" title={withHint('Downloads', 'downloads')} onclick={() => (ui.downloadsOpen = !ui.downloadsOpen)} data-downloads-anchor>
    <Icon name="download" />
    {#if downloads.progress >= 0}
      <svg class="ring" viewBox="0 0 36 36" aria-hidden="true">
        <circle cx="18" cy="18" r="15" pathLength="100" stroke-dasharray="{Math.max(3, downloads.progress * 100)} 100" />
      </svg>
    {:else if downloads.active.length}
      <span class="dot"></span>
    {/if}
  </button>
  <button class="icon-btn" class:on={onNotes} title={notesKey ? `Notes (${notesKey})` : 'Notes'} data-action="notes" onclick={() => openNotes()}>
    <Icon name="note" />
  </button>
  {#if updates.ready}
    <button
      class="icon-btn update"
      title={`Zenith ${updates.status.version ?? ''} is ready — restart to update (tabs are restored)`}
      disabled={updates.restarting}
      onclick={() => void updates.restart()}
    >
      <Icon name="arrowDownToLine" />
      <span class="dot"></span>
    </button>
  {/if}
  <div class="spaces" class:many class:reordering={reorder?.moved} role="tablist" aria-label="Spaces">
    {#each browser.spaces as s (s.id)}
      <button
        class="space-btn"
        class:active={s.id === browser.activeSpaceId}
        class:dragged={reorder?.moved && reorder.id === s.id}
        class:drop={dnd.drag?.started && dnd.drag.target?.zone === 'space' && dnd.drag.target.spaceId === s.id}
        role="tab"
        aria-selected={s.id === browser.activeSpaceId}
        style:--c={s.color}
        data-drop-space={s.id}
        data-space-btn={s.id}
        title={s.name}
        onpointerdown={(e) => startReorder(e, s.id)}
        onclick={() => browser.switchSpace(s.id)}
        oncontextmenu={(e) => {
          e.preventDefault();
          ui.openMenu(e.clientX, e.clientY, spaceMenu(s));
        }}
      >
        {#if many && s.id !== browser.activeSpaceId}
          <span class="mini-dot"></span>
        {:else}
          <span class="space-icon">{s.icon}</span>
        {/if}
      </button>
    {/each}
  </div>
  <button
    class="icon-btn"
    title="Create new…"
    data-action="create-new"
    onclick={(e) => {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      // In a tab strip at the top of the window the menu opens below the button.
      ui.openMenu(r.left, inline && r.top < window.innerHeight / 2 ? r.bottom + 4 : r.top - 8, createNewMenu());
    }}
  >
    <Icon name="plus" />
  </button>
</footer>

<style>
  .foot {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    padding: var(--zen-pad) var(--zen-pad) 0;
    min-height: 40px;
  }
  .foot.collapsed {
    flex-direction: column;
    padding: var(--zen-sep) 0 0;
  }
  .foot.inline {
    flex: 0 1 auto;
    min-width: 0;
    min-height: 0;
    padding: 0;
  }
  .foot.inline .spaces {
    flex: 0 1 auto;
  }
  .spaces {
    flex: 1;
    min-width: 0;
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 3px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .foot.collapsed .spaces {
    flex-direction: column;
    gap: 2px;
  }
  .spaces.many {
    gap: 0;
  }
  .space-btn {
    flex: none;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    filter: grayscale(1);
    opacity: 0.7;
    transition:
      filter 0.2s,
      opacity 0.2s,
      width 0.1s,
      transform 0.2s,
      background-color 0.1s;
  }
  .foot.collapsed .space-btn {
    width: 36px;
  }
  .space-icon {
    font-size: 12px;
    line-height: 1;
  }
  .space-btn.active,
  .space-btn:hover,
  .space-btn.dragged {
    filter: none;
    opacity: 1;
  }
  .space-btn:hover,
  .space-btn.dragged {
    background: var(--element-bg);
  }
  .spaces.reordering .space-btn:not(.dragged) {
    opacity: 0.2;
    filter: grayscale(1);
  }
  .space-btn.drop {
    transform: scale(1.18);
    filter: none;
    opacity: 1;
    background: color-mix(in srgb, var(--c) 35%, transparent);
  }
  .spaces.many .space-btn:not(.active) {
    width: 14px;
  }
  .mini-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: color-mix(in srgb, currentColor 50%, transparent);
  }
  .dl,
  .update {
    position: relative;
  }
  .icon-btn.on {
    background: var(--element-bg);
  }
  .update {
    color: var(--accent-fg);
  }
  .ring {
    position: absolute;
    inset: 1px;
    transform: rotate(-90deg);
    pointer-events: none;
  }
  .ring circle {
    fill: none;
    stroke: var(--accent-button);
    stroke-width: 2.4;
    stroke-linecap: round;
    transition: stroke-dasharray 0.3s;
  }
  .dot {
    position: absolute;
    right: 4px;
    top: 4px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent-button);
  }
</style>
