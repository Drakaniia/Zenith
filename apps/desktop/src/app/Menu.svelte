<script lang="ts">
  import { scale } from 'svelte/transition';
  import { tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { type MenuItem, type MenuMods } from '$shared/ui/menu';
  import { overlayLease } from '$platform/layout.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { zoomActive } from '$features/page/actions';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  const zoomPct = $derived(browser.activeTab?.live ? Math.round((browser.activeTab.zoom || 1) * 100) : 100);

  // A menu and the submenus opened from it, to any depth. Panel 0 is the menu itself; `subs[i]`
  // is panel i + 1 and hangs from item `parentIndex` of the panel before it.
  interface Sub {
    items: MenuItem[];
    x: number;
    y: number;
    /** Left edge of the parent item: where the submenu flips to when it doesn't fit on the right. */
    flip: number;
    parentIndex: number;
  }
  let subs = $state<Sub[]>([]);
  let focus = $state({ level: 0, index: -1 });
  let rootEl: HTMLDivElement | undefined = $state();

  const menu = $derived(ui.menu);
  const panels = $derived(menu ? [menu.items, ...subs.map((s) => s.items)] : []);

  const selectable = (it: MenuItem) => !it.separator && !it.disabled && !it.zoom && !it.heading;
  const childrenOf = (it: MenuItem) => (typeof it.children === 'function' ? it.children() : it.children);

  $effect(() => {
    if (!menu) {
      subs = [];
      return;
    }
    void menu.key;
    subs = [];
    focus = { level: 0, index: -1 };
    void tick().then(() => rootEl?.focus());
  });

  // Keeps the keyboard-focused item visible in a long menu.
  $effect(() => {
    void focus.level;
    void focus.index;
    void tick().then(() => rootEl?.querySelector('.item.focus')?.scrollIntoView({ block: 'nearest' }));
  });

  /** Places a panel at (x, y), moving it back inside the window if it would run off an edge. */
  function fit(node: HTMLElement, p: { x: number; y: number; flip?: number }) {
    const apply = (q: { x: number; y: number; flip?: number }) => {
      const w = node.offsetWidth;
      const h = node.offsetHeight;
      let x = q.x;
      let y = q.y;
      if (x + w > window.innerWidth - 6) x = q.flip !== undefined ? q.flip - w - 2 : window.innerWidth - w - 6;
      if (y + h > window.innerHeight - 6) y = window.innerHeight - h - 6;
      node.style.left = `${Math.max(6, x)}px`;
      node.style.top = `${Math.max(6, y)}px`;
    };
    apply(p);
    return { update: apply };
  }

  const modsOf = (e: MouseEvent): MenuMods => ({
    ctrl: e.ctrlKey || e.metaKey,
    shift: e.shiftKey,
    alt: e.altKey,
    middle: e.button === 1,
  });

  function run(item: MenuItem, mods?: MenuMods) {
    if (item.disabled || item.separator || item.heading || item.children) return;
    ui.closeMenu();
    item.run?.(mods);
  }

  /** Opens the submenu of `item` (item `index` of panel `level`) beside `el`; closes deeper ones. */
  function openSub(level: number, item: MenuItem, index: number, el: HTMLElement) {
    subs = subs.slice(0, level);
    const items = childrenOf(item);
    if (!items || item.disabled) return;
    const r = el.getBoundingClientRect();
    subs = [...subs, { items, x: r.right + 2, y: r.top - 5, flip: r.left, parentIndex: index }];
  }

  function firstSelectable(items: MenuItem[]) {
    return items.findIndex(selectable);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!menu) return;
    const items = panels[focus.level] ?? [];
    const sel = items.map((it, i) => (selectable(it) ? i : -1)).filter((i) => i >= 0);
    const item = items[focus.index];
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!sel.length) return;
      const cur = sel.indexOf(focus.index);
      const next = e.key === 'ArrowDown' ? (cur + 1) % sel.length : (cur - 1 + sel.length) % sel.length;
      focus = { ...focus, index: sel[next] };
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      if (sel.length) focus = { ...focus, index: e.key === 'Home' ? sel[0] : sel[sel.length - 1] };
    } else if (e.key === 'ArrowRight' || ((e.key === 'Enter' || e.key === ' ') && item && item.children)) {
      if (!item || !item.children || item.disabled) return;
      e.preventDefault();
      const el = rootEl?.querySelector<HTMLElement>(`.item[data-level="${focus.level}"][data-index="${focus.index}"]`);
      if (!el) return;
      openSub(focus.level, item, focus.index, el);
      const opened = subs[focus.level];
      if (opened) focus = { level: focus.level + 1, index: firstSelectable(opened.items) };
    } else if (e.key === 'ArrowLeft' || (e.key === 'Escape' && focus.level > 0)) {
      if (focus.level === 0) return;
      // Closes one submenu and returns to the item it hangs from; the menu itself stays.
      e.preventDefault();
      e.stopPropagation();
      const closing = subs[focus.level - 1];
      subs = subs.slice(0, focus.level - 1);
      focus = { level: focus.level - 1, index: closing?.parentIndex ?? -1 };
    } else if ((e.key === 'Enter' || e.key === ' ') && item) {
      e.preventDefault();
      run(item, { ctrl: e.ctrlKey, shift: e.shiftKey, alt: e.altKey, middle: false });
    } else if (e.key === 'Escape') {
      e.preventDefault();
      ui.closeMenu();
    }
  }
</script>

{#if menu}
  {#key menu.key}
    <div class="root menu-root" bind:this={rootEl} role="presentation" tabindex="-1" use:overlayLease {onkeydown}>
      <div
        class="catcher"
        role="presentation"
        onpointerdown={() => ui.closeMenu()}
        oncontextmenu={(e) => {
          e.preventDefault();
          ui.closeMenu();
        }}
      ></div>
      {#each panels as items, level (level)}
        <div
          class="menu glass"
          class:sub={level > 0}
          role="menu"
          tabindex="-1"
          use:fit={level === 0 ? { x: menu.x, y: menu.y } : { x: subs[level - 1].x, y: subs[level - 1].y, flip: subs[level - 1].flip }}
          transition:scale={{ start: 0.97, duration: 120, opacity: 0 }}
        >
          {#each items as item, i (i)}
            {#if item.separator}
              <div class="sep"></div>
            {:else if item.heading}
              <div class="heading ellipsis">{item.label}</div>
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
                class:focus={focus.level === level && focus.index === i}
                class:open={subs[level]?.parentIndex === i}
                disabled={item.disabled}
                role="menuitem"
                aria-haspopup={item.children ? 'menu' : undefined}
                data-level={level}
                data-index={i}
                onmouseenter={(e) => {
                  focus = { level, index: i };
                  openSub(level, item, i, e.currentTarget as HTMLElement);
                }}
                onclick={(e) => (item.children ? openSub(level, item, i, e.currentTarget as HTMLElement) : run(item, modsOf(e)))}
                onauxclick={(e) => {
                  if (e.button === 1) run(item, modsOf(e));
                }}
              >
                <span class="icon">
                  {#if item.swatch}
                    <span
                      class="swatch"
                      class:photo={item.swatch.photo}
                      style:background={item.swatch.photo ? 'transparent' : item.swatch.color}
                      style:color={item.swatch.fg}
                    >
                      {#if item.swatch.img}<img src={item.swatch.img} alt="" draggable="false" />{:else}{item.swatch.text}{/if}
                    </span>
                  {:else if item.checked}
                    <Icon name="check" size={15} />
                  {:else if item.site}
                    <Favicon url={item.site.url} favicon={item.site.favicon} size={16} />
                  {:else if item.icon}
                    <Icon name={item.icon} size={15} />
                  {/if}
                </span>
                <span class="label ellipsis">{item.label}</span>
                {#if item.hint}<span class="hint">{item.hint}</span>{/if}
                {#if item.children}<Icon name="chevronRight" size={14} />{/if}
              </button>
            {/if}
          {/each}
        </div>
      {/each}
    </div>
  {/key}
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 90;
    outline: none;
  }
  .catcher {
    position: absolute;
    inset: 0;
  }
  /* Zen's arrow panel / menupopup: panel colour, 2px x 4px item margins, 5px item radius. */
  .menu {
    position: absolute;
    min-width: 230px;
    max-width: 340px;
    max-height: calc(100vh - 12px);
    overflow-y: auto;
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
  .heading {
    margin: 0 4px;
    padding: 6px 12px 4px 38px;
    font-size: 12px;
    color: var(--text-3);
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
  .swatch {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 50%;
    font-size: 10.5px;
    font-weight: 700;
    line-height: 1;
  }
  .swatch img {
    width: 62%;
    height: 62%;
    display: block;
  }
  .swatch.photo img {
    width: 100%;
    height: 100%;
    object-fit: cover;
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
