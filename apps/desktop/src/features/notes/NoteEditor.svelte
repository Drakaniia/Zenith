<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { notes } from './notes.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { displayChord } from '$features/shortcuts/shortcuts';
  import { formatEdited, wordCount } from './notes';

  // The open note on zenith://notes (title, word count, text), or the empty state.

  let {
    wide,
    now,
    create,
  }: {
    wide: boolean;
    now: number;
    create: () => void;
  } = $props();

  const active = $derived(notes.active);
  const shortcut = $derived(settings.value.shortcuts.openNotes?.[0]);

  let titleEl: HTMLInputElement | undefined = $state();
  let bodyEl: HTMLTextAreaElement | undefined = $state();
  /** The field undo/redo act on: the one last typed in. */
  let lastField: HTMLInputElement | HTMLTextAreaElement | null = null;

  export function focusBody(atEnd = true) {
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

  export function history(cmd: 'undo' | 'redo') {
    const el = lastField?.isConnected ? lastField : bodyEl;
    if (!el) return;
    el.focus();
    // The fields' own undo stacks, as Ctrl+Z / Ctrl+Y use them.
    document.execCommand(cmd);
  }
</script>

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

<style>
  /* ---- Editor ---- */
  .editor {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    cursor: text;
    padding: 28px 40px 80px;
  }
  /* `.narrow` is set on the page root (NotesPage) below its breakpoint. */
  :global(.notes.narrow) .editor {
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
