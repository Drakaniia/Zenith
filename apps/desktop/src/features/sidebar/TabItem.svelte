<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import { browser, needsWebview, tabTitle } from '$features/browser/browser.svelte';
  import { tabMenu } from '../../app/menus';
  import { titleFor } from '$shared/url';
  import { ui } from '$shared/ui/ui.svelte';

  import type { Tab } from '$shared/types';

  // A sidebar tab, modelled on Zen's vertical tab (zen-tabs/vertical-tabs.css).
  let {
    tab,
    active = false,
    inSplit = false,
    drop = null,
    dragging = false,
    collapsed = false,
    grouped = false,
    onpointerdown,
  }: {
    tab: Tab;
    active?: boolean;
    inSplit?: boolean;
    drop?: 'before' | 'after' | null;
    dragging?: boolean;
    /** Icon-only sidebar (Zen's collapsed toolbar layout). */
    collapsed?: boolean;
    /** Member of a split-view group row. */
    grouped?: boolean;
    onpointerdown?: (e: PointerEvent) => void;
  } = $props();

  const title = $derived(tabTitle(tab));
  /** Picked together with other tabs (Ctrl+click, Shift+click), e.g. for "Bookmark Tabs…". */
  const picked = $derived(ui.selectedTabs.length > 1 && ui.selectedTabs.includes(tab.id));

  /** A click picks the tab (Ctrl: adds it to the picked ones, Shift: all between) or opens it. */
  function click(e: MouseEvent) {
    if (e.ctrlKey || e.metaKey) {
      ui.toggleTabSelected(tab.id, browser.activeTab?.id);
    } else if (e.shiftKey) {
      ui.selectTabRange(browser.order(), ui.selectedTabs[0] ?? browser.activeTab?.id, tab.id);
    } else {
      ui.clearTabSelection();
      browser.activate(tab.id);
    }
  }
  const sleeping = $derived(!tab.live && needsWebview(tab.url));
  const keep = $derived(tab.pinned || tab.essential);
  /** A pinned tab that navigated away from its pinned page (Zen: zen-pinned-changed). */
  const changed = $derived(tab.pinned && !!tab.pinnedUrl && tab.pinnedUrl !== tab.url && !collapsed);
  let showSublabel = $state(false);
  let renaming = $state(false);
  let input: HTMLInputElement | undefined = $state();

  function menu(e: MouseEvent) {
    e.preventDefault();
    ui.openMenu(e.clientX, e.clientY, tabMenu(tab, { rename: startRename }));
  }

  export async function startRename() {
    if (collapsed) return;
    renaming = true;
    ui.renaming = true;
    await tick();
    input?.focus();
    input?.select();
  }

  function finishRename(save: boolean) {
    if (!renaming) return;
    const value = input?.value.trim() ?? '';
    renaming = false;
    ui.renaming = false;
    if (save) tab.customTitle = value && value !== titleFor(tab.url, tab.title) ? value : undefined;
  }

  /** Pinned tabs unload instead of closing; an unloaded pinned tab's X removes it, as in Zen. */
  function closeOrReset(e: MouseEvent) {
    e.stopPropagation();
    if (tab.pinned && sleeping) browser.removeTab(tab.id, true);
    else browser.closeTab(tab.id);
  }
</script>

<div
  class="tab"
  class:active
  class:picked
  class:sleeping
  class:split={inSplit && !grouped}
  class:dragging
  class:collapsed
  class:grouped
  class:sublabel={showSublabel}
  role="tab"
  tabindex="-1"
  aria-selected={active}
  data-tab-id={tab.id}
  data-audible={tab.audible ? 'true' : undefined}
  title={renaming ? undefined : title}
  onclick={(e) => !renaming && click(e)}
  onauxclick={(e) => {
    if (e.button === 1) browser.closeTab(tab.id);
  }}
  onkeydown={(e) => {
    if (e.key === 'Enter' && !renaming) browser.activate(tab.id);
  }}
  oncontextmenu={menu}
  {onpointerdown}
>
  <span class="bg"></span>
  {#if changed}
    <button
      class="reset-pin"
      title="Back to pinned page"
      onpointerenter={() => (showSublabel = true)}
      onpointerleave={() => (showSublabel = false)}
      onclick={(e) => {
        e.stopPropagation();
        browser.resetPinned(tab.id);
      }}
    >
      <span class="icon"><Favicon url={tab.url} favicon={tab.favicon} /></span>
      <span class="reset-arrow"><Icon name="back" size={14} /></span>
    </button>
  {:else}
    <span class="icon">
      {#if tab.crashed}
        <span class="warn"><Icon name="alert" size={15} /></span>
      {:else}
        <Favicon url={tab.url} favicon={tab.favicon} />
      {/if}
      {#if collapsed && (tab.audible || tab.muted)}
        <span class="audio-badge"><Icon name={tab.muted ? 'volumeOff' : 'volume'} size={9} stroke={2.6} /></span>
      {/if}
    </span>
  {/if}
  {#if !collapsed}
    <span class="labels">
      {#if renaming}
        <input
          class="rename"
          bind:this={input}
          value={title}
          spellcheck="false"
          onclick={(e) => e.stopPropagation()}
          onpointerdown={(e) => e.stopPropagation()}
          onkeydown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') finishRename(true);
            else if (e.key === 'Escape') finishRename(false);
          }}
          onblur={() => finishRename(true)}
        />
      {:else}
        <span
          class="title ellipsis"
          role="presentation"
          ondblclick={(e) => {
            e.stopPropagation();
            void startRename();
          }}>{title}</span
        >
        {#if changed}<span class="sub ellipsis">Back to pinned page</span>{/if}
      {/if}
    </span>
    {#if tab.audible || tab.muted}
      <button
        class="audio"
        title={tab.muted ? 'Unmute tab' : 'Mute tab'}
        onclick={(e) => {
          e.stopPropagation();
          browser.setMuted(tab.id, !tab.muted);
        }}
      >
        <Icon name={tab.muted ? 'volumeOff' : 'volume'} size={14} />
      </button>
    {/if}
    {#if !tab.essential && !renaming}
      <button
        class="close"
        class:reset={keep && !sleeping}
        title={keep && !sleeping ? 'Unload tab' : 'Close tab'}
        aria-label={keep && !sleeping ? 'Unload tab' : 'Close tab'}
        onclick={closeOrReset}
      ></button>
    {/if}
  {/if}
  {#if drop}
    <span class="drop-mark" class:after={drop === 'after'}></span>
  {/if}
</div>

<style>
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    height: calc(var(--tab-h) + var(--tab-mb) * 2);
    padding: var(--tab-mb);
    color: var(--text);
    font-size: var(--tab-font);
    transition:
      scale 0.1s ease,
      opacity var(--med) ease;
  }
  .tab:active:not(:has(button:active, input)) {
    scale: 0.98;
  }
  .bg {
    position: absolute;
    inset: var(--tab-mb);
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    pointer-events: none;
    transition: background-color 0.1s ease;
  }
  .tab:hover .bg {
    background: var(--hover);
  }
  .tab.active .bg {
    background: var(--tab-selected);
    box-shadow: var(--tab-shadow);
  }
  /* Tabs picked together (Zen: multiselected). */
  .tab.picked .bg {
    background: color-mix(in srgb, var(--accent) 24%, var(--hover));
  }
  .tab.picked.active .bg {
    background: color-mix(in srgb, var(--accent) 30%, var(--tab-selected));
  }
  .tab.dragging {
    opacity: 0.4;
  }

  /* Icon: 16px with Zen's inner margins (10px after, a third of that before). */
  .icon {
    position: relative;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    flex: none;
    margin-inline: calc(var(--tab-pad) + 3px) 10px;
  }
  .tab.sleeping .icon {
    opacity: 0.5;
  }
  .warn {
    color: var(--warn);
  }
  .labels {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: center;
  }
  .title {
    display: block;
    width: 100%;
    line-height: 1.2;
    transition: transform 0.1s ease-in-out;
  }
  .sub {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 3px;
    font-size: 10px;
    line-height: 1.2;
    opacity: 0;
    pointer-events: none;
    transition:
      opacity 0.1s ease-in-out,
      transform 0.1s ease-in-out;
  }
  .tab.sublabel .title {
    transform: translateY(-6px);
  }
  .tab.sublabel .sub {
    opacity: 0.6;
    transform: translateY(-2px);
  }
  .rename {
    width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    font: inherit;
    color: inherit;
  }

  /* Back-to-pinned-page button: the favicon area, divided from the label by a slanted bar. */
  .reset-pin {
    position: relative;
    align-self: stretch;
    display: flex;
    align-items: center;
    margin-inline-end: 8px;
    padding-inline-end: 4px;
    border-radius: var(--r-tab) 0 0 var(--r-tab);
    corner-shape: var(--sq);
  }
  .reset-pin .icon {
    margin-inline-end: 0;
    transition: opacity 0.1s;
  }
  .reset-pin::after {
    content: '';
    position: absolute;
    right: 0;
    top: 50%;
    width: 2.5px;
    height: 16px;
    border-radius: 2px;
    background: color-mix(in srgb, currentColor 30%, transparent);
    transform: translateY(-50%) rotate(15deg);
  }
  .reset-arrow {
    position: absolute;
    left: calc(var(--tab-pad) + 4px);
    display: grid;
    opacity: 0;
    transition: opacity 0.1s;
  }
  .reset-pin:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }
  .reset-pin:hover::after {
    opacity: 0;
  }
  .reset-pin:hover .icon {
    opacity: 0;
  }
  .reset-pin:hover .reset-arrow {
    opacity: 1;
  }

  /* Close (X) and pinned "unload" (–) buttons are drawn from two 1.5px lines, which spin into
   * place when the button appears, as in Zen. */
  .close,
  .audio {
    position: relative;
    flex: none;
    display: none;
    place-items: center;
    width: 24px;
    height: 24px;
    margin-inline: 2px calc(var(--tab-pad) - 4px);
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    color: inherit;
  }
  .audio {
    display: grid;
    margin-inline-end: 2px;
    opacity: 0.8;
  }
  .tab:hover .close {
    display: grid;
  }
  .close:hover,
  .audio:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }
  .close:hover:active,
  .audio:hover:active {
    background: color-mix(in srgb, currentColor 20%, transparent);
    scale: 0.97;
  }
  .close::before,
  .close::after {
    --rot: 45deg;
    content: '';
    position: absolute;
    top: 50%;
    left: 50%;
    width: 15px;
    height: 1.5px;
    border-radius: 1px;
    background: currentColor;
    translate: -50% -50%;
    rotate: var(--rot);
    transition:
      rotate 0.12s ease,
      width 0.12s ease;
  }
  .close::after {
    rotate: calc(-1 * var(--rot));
  }
  .close.reset::before,
  .close.reset::after {
    --rot: 0deg;
    width: 12px;
  }
  @starting-style {
    .close::before,
    .close::after {
      rotate: 0deg;
      width: 12px;
    }
    .close.reset::before,
    .close.reset::after {
      rotate: 45deg;
      width: 15px;
    }
    .close.reset::after {
      rotate: -45deg;
    }
  }

  /* Split-view marker for tabs shown in split but listed apart from their partner. */
  .tab.split::before {
    content: '';
    position: absolute;
    left: 4px;
    top: 13px;
    bottom: 13px;
    width: 3px;
    border-radius: 3px;
    background: var(--split-outline);
  }

  /* Zen's drag indicator: a 2px line with a ring at its start. */
  .drop-mark {
    position: absolute;
    left: 14px;
    right: 6px;
    top: -1px;
    height: 2px;
    border-radius: 5px;
    background: var(--drag-indicator);
    pointer-events: none;
    z-index: 2;
  }
  .drop-mark.after {
    top: auto;
    bottom: -1px;
  }
  .drop-mark::before {
    content: '';
    position: absolute;
    left: -8px;
    top: 50%;
    width: 4px;
    height: 4px;
    border: 2px solid var(--drag-indicator);
    border-radius: 50%;
    translate: 0 -50%;
  }

  /* Icon-only tabs (collapsed sidebar): 48px wide squares, centred. */
  .tab.collapsed {
    width: calc(var(--collapsed-tab-w) + var(--tab-mb) * 2);
    margin-inline: auto;
    justify-content: center;
    padding-inline: 0;
  }
  .tab.collapsed .icon {
    margin: 0;
  }
  .tab.collapsed .drop-mark {
    left: 10px;
    right: 4px;
  }
  .audio-badge {
    position: absolute;
    right: -7px;
    top: -6px;
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--panel-bg);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--line);
  }

  /* Inside a split group: shorter, no own background/shadow, separators between members. */
  .tab.grouped {
    flex: 1 1 0;
    min-width: 0;
    height: 32px;
    padding: 2px;
  }
  .tab.grouped .bg {
    border-radius: 7px;
  }
  .tab.grouped:hover .bg {
    background: transparent;
  }
  .tab.grouped.active .bg {
    background: color-mix(in srgb, var(--element-bg), transparent 40%);
    box-shadow: none;
  }
  .tab.grouped:active {
    scale: 1;
  }
  .tab.grouped .icon {
    margin-inline: 6px 7px;
  }
  .tab.grouped .close {
    margin-inline-end: 0;
  }
  .tab.grouped:not(:last-child)::after {
    content: '';
    position: absolute;
    right: 0;
    top: 50%;
    width: 1px;
    height: 16px;
    translate: 0 -50%;
    background: var(--separator);
  }
</style>
