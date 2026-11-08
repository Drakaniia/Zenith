<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { notes } from './notes.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { ui, type MenuItem } from '$shared/ui/ui.svelte';
  import { copyText } from '../../app/actions';
  import { displayChord } from '$features/shortcuts/shortcuts';
  import {
    copyableText,
    formatEdited,
    groupNotes,
    isEmptyNote,
    notePreview,
    noteTitle,
    UNTITLED,
    wordCount,
    type Note,
  } from './notes';

  // zenith://notes: the list of notes (search, most recently edited first) on the left, the
  // toolbar on top and the open note below it. State lives in the notes store, so switching tabs
  // or windows never loses the open note or an edit.

  /** Below this width the list becomes a drawer over the editor. */
  const NARROW = 720;
  let width = $state(1200);
  const narrow = $derived(width < NARROW);
  let drawer = $state(false);
  const listShown = $derived(narrow ? drawer : settings.value.notesSidebar);
  const wide = $derived(settings.value.notesWide);
  const active = $derived(notes.active);

  // Relative times ("5 min ago") stay current.
  let now = $state(Date.now());
  const groups = $derived(
    notes.query.trim() ? [{ label: 'Results', items: notes.visible }] : groupNotes(notes.visible, now),
  );
  const shortcut = $derived(settings.value.shortcuts.openNotes?.[0]);

  let titleEl: HTMLInputElement | undefined = $state();
  let bodyEl: HTMLTextAreaElement | undefined = $state();
  let searchEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();
  /** The field undo/redo act on: the one last typed in. */
  let lastField: HTMLInputElement | HTMLTextAreaElement | null = null;

  let renaming = $state<string | null>(null);
  let renameText = $state('');

  function focusBody(atEnd = true) {
    const el = bodyEl;
    if (!el) return;
    el.focus({ preventScroll: true });
    if (atEnd) el.setSelectionRange(el.value.length, el.value.length);
  }

  // The store asks for focus (the shortcut, "New note", Ctrl+F on this page).
  $effect(() => {
    if (!notes.focusEditor) return;
    void tick().then(() => (active && !active.title && !active.body ? titleEl?.focus() : focusBody()));
  });
  $effect(() => {
    if (!notes.focusSearch) return;
    if (narrow) drawer = true;
    else if (!settings.value.notesSidebar) settings.update({ notesSidebar: true });
    void tick().then(() => searchEl?.select());
  });

  function toggleList() {
    if (narrow) drawer = !drawer;
    else settings.update({ notesSidebar: !settings.value.notesSidebar });
  }

  function create() {
    notes.create();
    drawer = false;
  }

  function open(id: string) {
    if (renaming && renaming !== id) commitRename();
    notes.select(id);
    if (narrow) drawer = false;
  }

  function history(cmd: 'undo' | 'redo') {
    const el = lastField?.isConnected ? lastField : bodyEl;
    if (!el) return;
    el.focus();
    // The fields' own undo stacks, as Ctrl+Z / Ctrl+Y use them.
    document.execCommand(cmd);
  }

  async function copy(n: Note | null) {
    if (!n) return;
    const text = copyableText(n);
    if (!text) return ui.toast('This note is empty', { icon: 'note' });
    await copyText(text);
    ui.toast('Note copied to the clipboard', { icon: 'copy' });
  }

  async function remove(n: Note | null) {
    if (!n) return;
    const removed = await notes.remove(n.id);
    if (removed && !isEmptyNote(removed)) {
      ui.toast(`Deleted “${noteTitle(removed)}”`, {
        icon: 'trash',
        timeout: 6000,
        action: { label: 'Undo', run: () => notes.restore(removed) },
      });
    }
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

  function onPageKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && narrow && drawer) {
      drawer = false;
      e.stopPropagation();
    }
  }

  onMount(() => {
    void notes.load().then(() => {
      if (notes.focusEditor === 0 && active) void tick().then(() => focusBody());
    });
    const t = setInterval(() => (now = Date.now()), 30_000);
    return () => {
      clearInterval(t);
      void notes.flush();
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="notes" class:narrow bind:clientWidth={width} onkeydown={onPageKey}>
  <header class="toolbar">
    <button
      class="icon-btn"
      class:on={listShown}
      title={listShown ? 'Hide the notes list' : 'Show the notes list'}
      aria-pressed={listShown}
      onclick={toggleList}
    >
      <Icon name="sidebar" />
    </button>
    <button class="btn new" title="New note" onclick={create}>
      <Icon name="plus" size={15} />
      {#if width >= 520}<span>New note</span>{/if}
    </button>
    <span class="sep"></span>
    <!-- mousedown is cancelled so the text field keeps the focus (and its selection). -->
    <button class="icon-btn" title="Undo (Ctrl+Z)" disabled={!active} onmousedown={(e) => e.preventDefault()} onclick={() => history('undo')}>
      <Icon name="undo" />
    </button>
    <button class="icon-btn" title="Redo (Ctrl+Y)" disabled={!active} onmousedown={(e) => e.preventDefault()} onclick={() => history('redo')}>
      <Icon name="redo" />
    </button>
    <span class="sep"></span>
    <button class="icon-btn" title="Copy note text" disabled={!active || isEmptyNote(active)} onmousedown={(e) => e.preventDefault()} onclick={() => copy(active)}>
      <Icon name="copy" />
    </button>
    <button class="icon-btn" title="Delete note" disabled={!active} onclick={() => remove(active)}>
      <Icon name="trash" />
    </button>
    <span class="spacer"></span>
    {#if width >= 460}
      <span class="status" class:error={!!notes.saveError} title={notes.saveError || undefined} aria-live="polite">
        {#if notes.saveError}
          <Icon name="alert" size={13} />Not saved
        {:else if notes.unsaved}
          Saving…
        {:else if active && !isEmptyNote(active)}
          <Icon name="check" size={13} />Saved
        {/if}
      </span>
    {/if}
    <button
      class="icon-btn"
      class:on={wide}
      title={wide ? 'Narrow column' : 'Full width'}
      aria-pressed={wide}
      onclick={() => settings.update({ notesWide: !wide })}
    >
      <Icon name="textWidth" />
    </button>
  </header>

  <div class="main">
    {#if listShown}
      {#if narrow}
        <button class="scrim" aria-label="Close the notes list" onclick={() => (drawer = false)}></button>
      {/if}
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
    {/if}

    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <main class="editor" class:wide onclick={(e) => e.target === e.currentTarget && focusBody()}>
      {#if active}
        {@const note = active}
        {#key note.id}
          <article class="doc">
            <input
              class="title"
              bind:this={titleEl}
              bind:value={() => note.title, (v) => notes.edit(note.id, { title: v })}
              placeholder="Title"
              aria-label="Note title"
              spellcheck="true"
              onfocus={(e) => (lastField = e.currentTarget)}
              onkeydown={(e) => {
                if (e.key === 'Enter' || (e.key === 'ArrowDown' && !e.shiftKey)) {
                  e.preventDefault();
                  focusBody(false);
                  bodyEl?.setSelectionRange(0, 0);
                }
              }}
            />
            <div class="meta">
              Edited {formatEdited(note.updated, now)}
              <span aria-hidden="true">·</span>
              {wordCount(note.body)} {wordCount(note.body) === 1 ? 'word' : 'words'}
            </div>
            <textarea
              class="text"
              bind:this={bodyEl}
              bind:value={() => note.body, (v) => notes.edit(note.id, { body: v })}
              placeholder="Start writing…"
              aria-label="Note text"
              spellcheck="true"
              onfocus={(e) => (lastField = e.currentTarget)}
            ></textarea>
          </article>
        {/key}
      {:else if notes.loaded}
        <div class="empty">
          <div class="empty-icon"><Icon name="note" size={30} /></div>
          <h2>{notes.items.length ? 'Pick a note' : 'Write something down'}</h2>
          <p class="muted">
            Notes are saved as you type and stay on this PC.
            {#if shortcut}
              Press
              {#each displayChord(shortcut) as part, i (i)}<span class="kbd">{part}</span>{/each}
              to open them from anywhere.
            {/if}
          </p>
          <button class="btn primary" onclick={create}><Icon name="plus" size={15} />New note</button>
        </div>
      {/if}
    </main>
  </div>
</div>

<style>
  .notes {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--page-bg);
  }

  /* ---- Toolbar ---- */
  .toolbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    height: 46px;
    padding: 0 10px;
    border-bottom: 1px solid var(--line);
  }
  .toolbar .icon-btn {
    width: 32px;
    height: 32px;
    color: var(--text-2);
  }
  .toolbar .icon-btn:hover {
    color: var(--text);
  }
  .toolbar .icon-btn.on {
    color: var(--text);
    background: var(--hover);
  }
  .new {
    height: 30px;
    padding: 0 12px 0 9px;
    margin-left: 4px;
  }
  .sep {
    width: 1px;
    height: 18px;
    margin: 0 6px;
    background: var(--line-strong);
  }
  .spacer {
    flex: 1;
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-right: 6px;
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .status.error {
    color: var(--danger);
  }

  /* ---- Layout ---- */
  .main {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }

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
  .narrow .list {
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
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 2;
    background: var(--backdrop);
    border: 0;
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

  /* ---- Editor ---- */
  .editor {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    cursor: text;
    padding: 28px 40px 80px;
  }
  .narrow .editor {
    padding: 20px 20px 60px;
  }
  .doc {
    max-width: 760px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    cursor: auto;
  }
  .editor.wide .doc {
    max-width: none;
  }
  .title {
    border: 0;
    outline: none;
    background: transparent;
    padding: 0;
    color: var(--text);
    font: 600 26px/1.3 var(--font-display);
  }
  .title::placeholder,
  .text::placeholder {
    color: var(--text-3);
  }
  .meta {
    display: flex;
    gap: 6px;
    margin: 6px 0 18px;
    font-size: 12px;
    color: var(--text-3);
  }
  .text {
    border: 0;
    outline: none;
    resize: none;
    background: transparent;
    padding: 0;
    color: var(--text);
    font: 15px/1.7 var(--font);
    /* Grows with its text; the editor pane scrolls like a document. */
    field-sizing: content;
    min-height: 55vh;
    tab-size: 4;
    overflow-wrap: anywhere;
  }
  .empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
    cursor: default;
  }
  .empty h2 {
    margin: 6px 0 0;
    font: 600 18px/1.3 var(--font-display);
  }
  .empty p {
    margin: 0 0 8px;
    max-width: 360px;
    line-height: 1.6;
  }
  .empty .kbd {
    margin: 0 1px;
  }
  .empty-icon {
    width: 60px;
    height: 60px;
    border-radius: 18px;
    corner-shape: var(--sq);
    display: grid;
    place-items: center;
    color: var(--text-2);
    background: var(--field);
  }
</style>
