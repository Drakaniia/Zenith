<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { notes } from './notes.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { type MenuItem } from '$shared/ui/menu';
  import { formatEdited, groupNotes, isEmptyNote, notePreview, noteTitle, UNTITLED, type Note } from './notes';

  // The list of notes on zenith://notes: search, the notes grouped by when they were edited,
  // renaming in place, the context menu and keyboard use.

  let {
    now,
    narrow,
    drawer = $bindable(),
    searchEl = $bindable(),
    focusBody,
    copy,
    remove,
  }: {
    now: number;
    narrow: boolean;
    drawer: boolean;
    searchEl?: HTMLInputElement;
    /** Puts the caret in the open note's text. */
    focusBody: (atEnd?: boolean) => void;
    copy: (n: Note | null) => Promise<unknown>;
    remove: (n: Note | null) => Promise<void>;
  } = $props();

  const active = $derived(notes.active);
  const groups = $derived(
    notes.query.trim() ? [{ label: 'Results', items: notes.visible }] : groupNotes(notes.visible, now),
  );

  let listEl: HTMLDivElement | undefined = $state();

  let renaming = $state<string | null>(null);
  let renameText = $state('');

  function open(id: string) {
    if (renaming && renaming !== id) commitRename();
    notes.select(id);
    if (narrow) drawer = false;
  }

  function startRename(n: Note) {
    renaming = n.id;
    const shown = noteTitle(n);
    renameText = n.title.trim() || shown === UNTITLED ? n.title : shown;
  }

  function commitRename() {
    if (renaming) notes.rename(renaming, renameText);
    renaming = null;
    listEl?.focus();
  }

  function focusSelect(el: HTMLInputElement) {
    el.focus();
    el.select();
  }

  function noteMenu(e: MouseEvent, n: Note) {
    e.preventDefault();
    const items: MenuItem[] = [
      { label: 'Rename', icon: 'edit', hint: 'F2', run: () => (open(n.id), startRename(n)) },
      { label: 'Copy text', icon: 'copy', disabled: isEmptyNote(n), run: () => void copy(n) },
      { separator: true },
      { label: 'Delete', icon: 'trash', hint: 'Del', danger: true, run: () => void remove(n) },
    ];
    ui.openMenu(e.clientX, e.clientY, items);
  }

  // Keyboard use of the list: arrows move, Enter opens the note for writing, F2 renames.
  function onListKey(e: KeyboardEvent) {
    if (renaming) return;
    const list = notes.visible;
    const i = list.findIndex((n) => n.id === notes.activeId);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = list[Math.max(0, Math.min(list.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))];
      if (next) {
        notes.select(next.id);
        void tick().then(() => listEl?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }));
      }
    } else if (e.key === 'Enter' && active) {
      e.preventDefault();
      if (narrow) drawer = false;
      void tick().then(() => focusBody());
    } else if (e.key === 'F2' && active) {
      e.preventDefault();
      startRename(active);
    } else if (e.key === 'Delete' && active) {
      e.preventDefault();
      void remove(active);
    }
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && notes.query) {
      e.preventDefault();
      e.stopPropagation();
      notes.query = '';
    } else if (e.key === 'Enter') {
      const first = notes.visible[0];
      if (first) {
        open(first.id);
        void tick().then(() => focusBody());
      }
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      listEl?.focus();
    }
  }
</script>

<aside class="list" aria-label="Notes">
  <div class="search">
    <Icon name="search" size={15} />
    <input
      bind:this={searchEl}
      bind:value={notes.query}
      placeholder="Search notes"
      spellcheck="false"
      aria-label="Search notes"
      onkeydown={onSearchKey}
    />
    {#if notes.query}
      <button class="icon-btn sm" title="Clear search" onclick={() => ((notes.query = ''), searchEl?.focus())}>
        <Icon name="close" size={13} />
      </button>
    {/if}
  </div>

  <div class="items" role="listbox" tabindex="0" aria-label="Notes" bind:this={listEl} onkeydown={onListKey}>
    {#each groups as g (g.label)}
      <div class="group">{g.label}</div>
      {#each g.items as n (n.id)}
        {@const preview = notePreview(n)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="item"
          class:on={n.id === notes.activeId}
          role="option"
          aria-selected={n.id === notes.activeId}
          tabindex="-1"
          onclick={() => open(n.id)}
          ondblclick={() => startRename(n)}
          oncontextmenu={(e) => noteMenu(e, n)}
        >
          {#if renaming === n.id}
            <input
              class="rename"
              bind:value={renameText}
              use:focusSelect
              placeholder={UNTITLED}
              aria-label="Note title"
              onclick={(e) => e.stopPropagation()}
              onblur={commitRename}
              onkeydown={(e) => {
                e.stopPropagation();
                if (e.key === 'Enter') commitRename();
                else if (e.key === 'Escape') {
                  renaming = null;
                  listEl?.focus();
                }
              }}
            />
          {:else}
            <span class="item-title ellipsis" class:untitled={noteTitle(n) === UNTITLED}>{noteTitle(n)}</span>
          {/if}
          <span class="item-meta">
            <span class="date">{formatEdited(n.updated, now)}</span>
            {#if preview}<span class="preview ellipsis">{preview}</span>{/if}
          </span>
        </div>
      {/each}
    {:else}
      <div class="list-empty">
        {#if !notes.loaded}
          Loading…
        {:else if notes.query.trim()}
          No notes match “{notes.query.trim()}”
        {:else}
          No notes yet
        {/if}
      </div>
    {/each}
  </div>
  {#if notes.items.length}
    <footer class="count">{notes.items.length} {notes.items.length === 1 ? 'note' : 'notes'}</footer>
  {/if}
</aside>

<style>
  /* ---- List of notes ---- */
  .list {
    flex: none;
    width: 280px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--line);
    background: color-mix(in srgb, var(--text) 2%, var(--page-bg));
  }
  /* `.narrow` is set on the page root (NotesPage) below its breakpoint. */
  :global(.notes.narrow) .list {
    position: absolute;
    z-index: 3;
    inset: 0 auto 0 0;
    width: min(300px, 86%);
    box-shadow: var(--shadow);
    animation: drawer-in 0.18s var(--ease-out);
  }
  @keyframes drawer-in {
    from {
      transform: translateX(-12px);
      opacity: 0;
    }
  }
  .search {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    margin: 10px 10px 4px;
    padding: 0 6px 0 10px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-3);
    transition: border-color var(--fast);
  }
  .search:focus-within {
    border-color: color-mix(in srgb, var(--accent) 70%, transparent);
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
  .items {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 6px 10px;
    outline: none;
  }
  .group {
    padding: 12px 8px 4px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-3);
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border-radius: var(--r-item);
    corner-shape: var(--sq);
    cursor: default;
    user-select: none;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.on {
    background: var(--selected);
    box-shadow: var(--tab-shadow);
  }
  .items:focus-visible .item.on {
    outline: 2px solid color-mix(in srgb, var(--accent) 70%, transparent);
    outline-offset: -2px;
  }
  .item-title {
    font-size: 13.5px;
    font-weight: 600;
    line-height: 1.35;
  }
  .item-title.untitled {
    color: var(--text-3);
    font-weight: 500;
  }
  .item-meta {
    display: flex;
    gap: 8px;
    min-width: 0;
    font-size: 12px;
    line-height: 1.35;
    color: var(--text-3);
  }
  .date {
    flex: none;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .preview {
    min-width: 0;
  }
  .rename {
    height: 24px;
    margin: -2px -4px;
    padding: 0 4px;
    border-radius: 4px;
    border: 1px solid color-mix(in srgb, var(--accent) 70%, transparent);
    background: var(--field-focus);
    color: var(--text);
    font-size: 13.5px;
    font-weight: 600;
    outline: none;
  }
  .list-empty {
    padding: 28px 12px;
    text-align: center;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .count {
    flex: none;
    padding: 8px 16px 10px;
    font-size: 11.5px;
    color: var(--text-3);
    border-top: 1px solid var(--line);
  }
</style>
