<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { type MenuItem } from '$shared/ui/menu';
  import type { Scope } from './library';

  // The Library's bar: back and forward, the Organize, Views and Sort menus, and the search box.

  let {
    scope,
    query = $bindable(),
    canBack,
    canForward,
    back,
    forward,
    organizeMenu,
    viewsMenu,
    sortMenu,
  }: {
    scope: Scope;
    query: string;
    canBack: boolean;
    canForward: boolean;
    back: () => void;
    forward: () => void;
    organizeMenu: () => MenuItem[];
    viewsMenu: () => MenuItem[];
    sortMenu: () => MenuItem[];
  } = $props();

  function drop(e: MouseEvent, items: () => MenuItem[]) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    ui.openMenu(r.left, r.bottom + 4, items());
  }
</script>

<header class="bar">
  <div class="nav">
    <button class="icon-btn" title="Back" aria-label="Back" disabled={!canBack} onclick={back}><Icon name="chevronLeft" size={16} /></button>
    <button class="icon-btn" title="Forward" aria-label="Forward" disabled={!canForward} onclick={forward}><Icon name="chevronRight" size={16} /></button>
  </div>
  <button class="drop" title="Organize" aria-haspopup="menu" onclick={(e) => drop(e, organizeMenu)}>
    <Icon name="gear" size={17} /><Icon name="chevronDown" size={12} />
  </button>
  <button class="drop" title="Views" aria-haspopup="menu" onclick={(e) => drop(e, viewsMenu)}>
    <Icon name="listView" size={17} /><Icon name="chevronDown" size={12} />
  </button>
  <button class="drop" title="Sort" aria-haspopup="menu" onclick={(e) => drop(e, sortMenu)}>
    <Icon name="sort" size={17} /><Icon name="chevronDown" size={12} />
  </button>
  <span class="grow"></span>
  {#if scope.kind !== 'history' && scope.kind !== 'downloads'}
    <label class="search">
      <Icon name="search" size={15} />
      <input
        bind:value={query}
        placeholder={scope.kind === 'folder' ? 'Search in this folder' : 'Search Bookmarks'}
        spellcheck="false"
        autocomplete="off"
        onkeydown={(e) => {
          if (e.key === 'Escape' && query) {
            e.stopPropagation();
            query = '';
          }
        }}
      />
    </label>
  {/if}
</header>

<style>
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 46px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
  }
  .nav {
    display: flex;
    margin-right: 4px;
    border-radius: var(--r-btn);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .nav .icon-btn {
    width: 30px;
    height: 28px;
  }
  .drop {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 30px;
    padding: 0 8px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-2);
  }
  .drop:hover {
    background: var(--field-focus);
    color: var(--text);
  }
  .grow {
    flex: 1;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    width: min(320px, 40%);
    height: 30px;
    padding: 0 10px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-3);
  }
  .search:focus-within {
    background: var(--field-focus);
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 13px;
  }
</style>
