<script lang="ts">
  import { scale } from 'svelte/transition';
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import { ui, type MenuItem } from '../lib/ui.svelte';
  import { overlayLease } from '../lib/layout.svelte';
  import { browser } from '../lib/browser.svelte';
  import { zoomActive } from '../lib/actions';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  const zoomPct = $derived(browser.activeTab?.live ? Math.round((browser.activeTab.zoom || 1) * 100) : 100);

  let menuEl: HTMLDivElement | undefined = $state();
  let subEl: HTMLDivElement | undefined = $state();
  let pos = $state({ x: 0, y: 0 });
  let sub = $state<{ items: MenuItem[]; x: number; y: number; index: number } | null>(null);
  let focusIndex = $state(-1);

  const menu = $derived(ui.menu);

  $effect(() => {
    if (!menu) {
      sub = null;
      return;
    }
    void menu.key;
    pos = { x: menu.x, y: menu.y };
    focusIndex = -1;
    sub = null;
    void tick().then(() => {
      if (!menuEl) return;
      const r = menuEl.getBoundingClientRect();
      pos = {
        x: Math.max(6, Math.min(menu.x, window.innerWidth - r.width - 6)),
        y: Math.max(6, Math.min(menu.y, window.innerHeight - r.height - 6)),
      };
      menuEl.focus();
    });
  });

  function run(item: MenuItem) {
    if (item.disabled || item.separator || item.children) return;
    ui.closeMenu();
    item.run?.();
  }

  function openSub(item: MenuItem, index: number, el: HTMLElement) {
    if (!item.children) {
      sub = null;
      return;
    }
    const r = el.getBoundingClientRect();
    sub = { items: item.children, x: r.right + 2, y: r.top - 5, index };
    void tick().then(() => {
      if (!subEl || !sub) return;
      const s = subEl.getBoundingClientRect();
      if (s.right > window.innerWidth - 6) sub.x = r.left - s.width - 2;
      if (s.bottom > window.innerHeight - 6) sub.y = Math.max(6, window.innerHeight - s.height - 6);
    });
  }

  function onkeydown(e: KeyboardEvent) {
    if (!menu) return;
    const items = menu.items;
    const selectable = items.map((it, i) => (!it.separator && !it.disabled && !it.zoom ? i : -1)).filter((i) => i >= 0);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const cur = selectable.indexOf(focusIndex);
      const next = e.key === 'ArrowDown' ? (cur + 1) % selectable.length : (cur - 1 + selectable.length) % selectable.length;
      focusIndex = selectable[next];
    } else if (e.key === 'Enter' && focusIndex >= 0) {
      e.preventDefault();
      run(items[focusIndex]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      ui.closeMenu();
    }
  }
</script>

{#if menu}
  {#key menu.key}
    <div class="root" use:overlayLease>
      <div
        class="catcher"
        role="presentation"
        onpointerdown={() => ui.closeMenu()}
        oncontextmenu={(e) => {
          e.preventDefault();
          ui.closeMenu();
        }}
      ></div>
      <div
        class="menu glass"
        role="menu"
        tabindex="-1"
        bind:this={menuEl}
        style:left="{pos.x}px"
        style:top="{pos.y}px"
        {onkeydown}
        transition:scale={{ start: 0.97, duration: 120, opacity: 0 }}
      >
        {#each menu.items as item, i (i)}
          {#if item.separator}
            <div class="sep"></div>
          {:else if item.zoom}
            <!-- Zen's app-menu zoom controls: label, −, percentage (resets), +, full screen. -->
            <div class="zoom-row" class:disabled={item.disabled}>
              <span class="label">Zoom</span>
              <button class="zoom-btn" title="Zoom out" disabled={item.disabled} onclick={() => zoomActive(-1)}>
                <Icon name="minimize" size={14} />
              </button>
              <button class="zoom-btn pct" title="Reset zoom" disabled={item.disabled} onclick={() => zoomActive(0)}>{zoomPct}%</button>
              <button class="zoom-btn" title="Zoom in" disabled={item.disabled} onclick={() => zoomActive(1)}>
                <Icon name="plus" size={14} />
              </button>
              <span class="zoom-sep"></span>
              <button
                class="zoom-btn"
                title="Full screen (F11)"
                onclick={async () => {
                  ui.closeMenu();
                  const w = getCurrentWindow();
                  await w.setFullscreen(!(await w.isFullscreen()));
                }}
              >
                <Icon name="expand" size={14} />
              </button>
            </div>
          {:else}
            <button
              class="item"
              class:danger={item.danger}
              class:focus={focusIndex === i}
              class:open={sub?.index === i}
              disabled={item.disabled}
              role="menuitem"
              onmouseenter={(e) => {
                focusIndex = i;
                openSub(item, i, e.currentTarget as HTMLElement);
              }}
              onclick={(e) => (item.children ? openSub(item, i, e.currentTarget as HTMLElement) : run(item))}
            >
              <span class="icon">
                {#if item.checked}<Icon name="check" size={15} />{:else if item.icon}<Icon name={item.icon} size={15} />{/if}
              </span>
              <span class="label ellipsis">{item.label}</span>
              {#if item.hint}<span class="hint">{item.hint}</span>{/if}
              {#if item.children}<Icon name="chevronRight" size={14} />{/if}
            </button>
          {/if}
        {/each}
      </div>
      {#if sub}
        <div class="menu glass" role="menu" tabindex="-1" bind:this={subEl} style:left="{sub.x}px" style:top="{sub.y}px">
          {#each sub.items as item, i (i)}
            {#if item.separator}
              <div class="sep"></div>
            {:else}
              <button class="item" disabled={item.disabled} role="menuitem" onclick={() => run(item)}>
                <span class="icon">{#if item.checked}<Icon name="check" size={15} />{:else if item.icon}<Icon name={item.icon} size={15} />{/if}</span>
                <span class="label ellipsis">{item.label}</span>
              </button>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  {/key}
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 90;
  }
  .catcher {
    position: absolute;
    inset: 0;
  }
  /* Zen's arrow panel / menupopup: panel colour, 2px x 4px item margins, 5px item radius. */
  .menu {
    position: absolute;
    min-width: 230px;
    max-width: 320px;
    padding: 4px 0;
    border-radius: var(--r-panel);
    corner-shape: var(--sq);
    outline: none;
    transform-origin: top left;
  }
  .item {
    width: calc(100% - 8px);
    margin: 0 4px;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 32px;
    padding: 6px 12px 6px 10px;
    border-radius: var(--r-item);
    corner-shape: var(--sq);
    text-align: left;
    font-size: 13px;
    color: var(--text);
  }
  .item:hover:not(:disabled),
  .item.focus:not(:disabled),
  .item.open {
    background: color-mix(in srgb, currentColor 9%, transparent);
  }
  .item:active:not(:disabled) {
    background: color-mix(in srgb, currentColor 14%, transparent);
  }
  .zoom-row {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 2px 4px;
    padding: 0 4px 0 40px;
    min-height: 34px;
    font-size: 13px;
  }
  .zoom-row.disabled .label {
    opacity: 0.4;
  }
  .zoom-row .label {
    flex: 1;
  }
  .zoom-btn {
    display: grid;
    place-items: center;
    min-width: 30px;
    height: 30px;
    padding: 0 6px;
    border-radius: var(--r-item);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .zoom-btn:hover:not(:disabled) {
    background: color-mix(in srgb, currentColor 9%, transparent);
  }
  .zoom-btn:disabled {
    opacity: 0.4;
  }
  .zoom-btn.pct {
    min-width: 50px;
  }
  .zoom-sep {
    width: 1px;
    height: 20px;
    margin: 0 4px;
    background: color-mix(in srgb, currentColor 15%, transparent);
  }
  .item:disabled {
    opacity: 0.4;
  }
  .item.danger {
    color: var(--danger);
  }
  .icon {
    width: 18px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    flex: none;
  }
  .item.danger .icon {
    color: var(--danger);
  }
  .label {
    flex: 1;
    min-width: 0;
  }
  .hint {
    font-size: 12px;
    opacity: 0.7;
    color: var(--text-2);
  }
  .sep {
    height: 1px;
    margin: 4px 1px;
    background: color-mix(in srgb, currentColor 15%, transparent);
  }
</style>
