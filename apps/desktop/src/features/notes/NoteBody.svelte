<!-- The text of a note, with bold, italic, bulleted and numbered lists. Right-click shows the
     formatting (and the clipboard) with their keys; Shift+right-click keeps the spelling menu.
     The text is saved as a small Markdown subset (rich-text.ts). -->
<script lang="ts">
  import { ui } from '$shared/ui/ui.svelte';
  import { shell } from '$platform/shell';
  import { copyText } from '$shared/clipboard';
  import type { MenuItem } from '$shared/ui/menu';
  import { notes } from './notes.svelte';
  import type { Note } from './notes';
  import { formatCommand, parseNote, serializeNote, type FormatCommand } from './rich-text';
  import { applyFormat, autoList, BODY_ATTR, formatState, readBlocks, renderBlocks, selectIn, selectionIn } from './rich-dom';

  let { note, onfocus }: { note: Note; onfocus?: (el: HTMLElement) => void } = $props();

  let el: HTMLDivElement | undefined = $state();
  /** The text the DOM shows, so an edit's own save doesn't redraw (and move the caret). */
  let shown: string | null = null;
  let blank = $state(true);
  /** A drag that started in the note moves its own (formatted) text; others drop plain text. */
  let draggingOwn = false;

  $effect(() => {
    const body = note.body;
    if (!el || body === shown) return;
    renderBlocks(el, parseNote(body));
    shown = body;
    blank = !body;
  });

  function save() {
    if (!el) return;
    const md = serializeNote(readBlocks(el));
    shown = md;
    blank = !md;
    notes.edit(note.id, { body: md });
  }

  export function focus(atEnd = true) {
    if (el) selectIn(el, null, atEnd);
  }

  function format(cmd: FormatCommand) {
    applyFormat(cmd);
  }

  function onkeydown(e: KeyboardEvent) {
    const cmd = formatCommand({ ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, vk: e.keyCode });
    if (cmd) {
      e.preventDefault();
      format(cmd);
    } else if (e.ctrlKey && !e.altKey && !e.shiftKey && e.code === 'KeyU') {
      // Underline can't be saved, so it isn't offered.
      e.preventDefault();
    }
  }

  function oninput(e: Event) {
    if ((e as InputEvent).inputType === 'insertText' && (e as InputEvent).data === ' ' && el && autoList(el)) return;
    save();
  }

  /** Pasted text comes in plain (formatting from web pages would not survive a save). */
  function onpaste(e: ClipboardEvent) {
    e.preventDefault();
    const text = e.clipboardData?.getData('text/plain') ?? '';
    if (text) document.execCommand('insertText', false, text);
  }

  function ondrop(e: DragEvent) {
    if (draggingOwn || !e.dataTransfer?.types.includes('text/plain')) return;
    e.preventDefault();
    const at = document.caretRangeFromPoint(e.clientX, e.clientY);
    if (el && at) selectIn(el, at);
    document.execCommand('insertText', false, e.dataTransfer.getData('text/plain'));
  }

  async function paste(range: Range | null) {
    let text = '';
    try {
      text = await shell.clipboardRead();
    } catch (err) {
      ui.toast(`Couldn't read the clipboard: ${err}`, { icon: 'alert' });
    }
    if (!el) return;
    selectIn(el, range);
    if (text) document.execCommand('insertText', false, text);
  }

  function oncontextmenu(e: MouseEvent) {
    // Shift+right-click: the built-in menu, with spelling suggestions.
    if (e.shiftKey || !el) return;
    e.preventDefault();
    const body = el;
    const range = selectionIn(body);
    const on = formatState();
    const picked = !!range && !range.collapsed;
    const act = (fn: () => void) => () => {
      selectIn(body, range);
      fn();
    };
    const item = (cmd: FormatCommand, label: string, icon: MenuItem['icon'], hint: string): MenuItem => ({
      label,
      icon,
      hint,
      checked: on[cmd],
      run: act(() => format(cmd)),
    });
    ui.openMenu(e.clientX, e.clientY, [
      item('bold', 'Bold', 'bold', 'Ctrl+B'),
      item('italic', 'Italic', 'italic', 'Ctrl+I'),
      { separator: true },
      item('bullets', 'Bulleted list', 'listBullets', 'Ctrl+Shift+8'),
      item('numbers', 'Numbered list', 'listNumbers', 'Ctrl+Shift+7'),
      { separator: true },
      { label: 'Cut', icon: 'scissors', hint: 'Ctrl+X', disabled: !picked, run: act(() => document.execCommand('cut')) },
      {
        label: 'Copy',
        icon: 'copy',
        hint: 'Ctrl+C',
        disabled: !picked,
        run: act(() => {
          if (!document.execCommand('copy') && range) void copyText(range.toString());
        }),
      },
      { label: 'Paste', icon: 'clipboard', hint: 'Ctrl+V', run: () => void paste(range) },
      {
        label: 'Select all',
        hint: 'Ctrl+A',
        run: () => {
          const all = document.createRange();
          all.selectNodeContents(body);
          selectIn(body, all);
        },
      },
    ]);
  }
</script>

<div
  class="text"
  class:blank
  bind:this={el}
  contenteditable="true"
  role="textbox"
  tabindex="0"
  aria-multiline="true"
  aria-label="Note text"
  data-placeholder="Start writing…"
  spellcheck="true"
  {...{ [BODY_ATTR]: '' }}
  onfocus={(e) => onfocus?.(e.currentTarget)}
  {onkeydown}
  {oninput}
  {onpaste}
  {ondrop}
  ondragstart={() => (draggingOwn = true)}
  ondragend={() => (draggingOwn = false)}
  {oncontextmenu}
></div>

<style>
  .text {
    position: relative;
    outline: none;
    color: var(--text);
    font: 15px/1.7 var(--font);
    min-height: 55vh;
    tab-size: 4;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    cursor: text;
  }
  .text.blank::before {
    content: attr(data-placeholder);
    position: absolute;
    color: var(--text-3);
    pointer-events: none;
  }
  .text :global(b) {
    font-weight: 650;
  }
  .text :global(ul),
  .text :global(ol) {
    margin: 2px 0;
    padding-left: 26px;
  }
  .text :global(li) {
    padding-left: 2px;
  }
  .text :global(li::marker) {
    color: var(--text-2);
  }
</style>
