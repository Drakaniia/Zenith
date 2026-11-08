<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import NoteList from './NoteList.svelte';
  import NoteEditor from './NoteEditor.svelte';
  import { notes } from './notes.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { copyText } from '$shared/clipboard';
  import { copyableText, isEmptyNote, noteTitle, type Note } from './notes';

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

  let searchEl: HTMLInputElement | undefined = $state();
  let editor: NoteEditor | undefined = $state();

  function focusBody(atEnd = true) {
    editor?.focusBody(atEnd);
  }

  // The store asks for focus (the shortcut, "New note", Ctrl+F on this page).
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

  function history(cmd: 'undo' | 'redo') {
    editor?.history(cmd);
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
      <NoteList {now} {narrow} bind:drawer bind:searchEl {focusBody} {copy} {remove} />
    {/if}

    <NoteEditor bind:this={editor} {wide} {now} {create} />
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
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 2;
    background: var(--backdrop);
    border: 0;
  }
</style>
