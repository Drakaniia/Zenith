// The notes editor's DOM: a note's blocks (rich-text.ts) drawn into the contenteditable body and
// read back from whatever editing left there, plus the formatting commands. Formatting goes
// through the browser's editing commands, so it lands on the field's own undo stack.

import { formatCommand, type Block, type BlockKind, type FormatCommand } from "./rich-text.ts";

/** Marks the note body, so formatting shortcuts know where they apply. */
export const BODY_ATTR = "data-note-body";

// ---------------------------------------------------------------------------------------------
// Blocks to DOM

export function renderBlocks(root: HTMLElement, blocks: Block[]) {
  root.replaceChildren();
  let list: HTMLElement | null = null;
  for (const b of blocks) {
    if (b.kind === "p") {
      list = null;
      const div = document.createElement("div");
      fill(div, b);
      root.append(div);
      continue;
    }
    const tag = b.kind === "ul" ? "UL" : "OL";
    if (!list || list.tagName !== tag) {
      list = document.createElement(tag);
      if (b.kind === "ol" && b.n && b.n !== 1) (list as HTMLOListElement).start = b.n;
      root.append(list);
    }
    const li = document.createElement("li");
    fill(li, b);
    list.append(li);
  }
}

function fill(el: HTMLElement, b: Block) {
  if (!b.runs.some((r) => r.text)) {
    el.append(document.createElement("br"));
    return;
  }
  for (const r of b.runs) {
    let node: Node = document.createTextNode(r.text);
    if (r.italic) {
      const i = document.createElement("i");
      i.append(node);
      node = i;
    }
    if (r.bold) {
      const strong = document.createElement("b");
      strong.append(node);
      node = strong;
    }
    el.append(node);
  }
}

// ---------------------------------------------------------------------------------------------
// DOM to blocks

const BLOCK_TAGS = new Set(["DIV", "P", "LI", "H1", "H2", "H3", "H4", "H5", "H6", "BLOCKQUOTE", "PRE", "SECTION", "ARTICLE"]);

interface ListCtx {
  kind: BlockKind;
  /** The list's first number, until its first item takes it. */
  start?: number;
}

interface Fmt {
  bold: boolean;
  italic: boolean;
}

/** Reads the editor's content as blocks: one per line, lists flattened to one level, and only
 *  bold and italic kept of whatever formatting the editing left. */
export function readBlocks(root: HTMLElement): Block[] {
  const blocks: Block[] = [];
  let cur: Block | null = null;

  const start = (list: ListCtx | null): Block => {
    const b: Block = { kind: list?.kind ?? "p", runs: [] };
    if (list?.kind === "ol" && list.start !== undefined) {
      b.n = list.start;
      list.start = undefined;
    }
    blocks.push(b);
    cur = b;
    return b;
  };

  const text = (t: string, fmt: Fmt, list: ListCtx | null) => {
    const lines = t.replace(/\u00a0/g, " ").split("\n");
    lines.forEach((line, i) => {
      if (i > 0) cur = null;
      if (!line) return;
      const b = cur ?? start(list);
      b.runs.push({ text: line, ...(fmt.bold ? { bold: true } : {}), ...(fmt.italic ? { italic: true } : {}) });
    });
  };

  const walk = (node: Node, fmt: Fmt, list: ListCtx | null) => {
    if (node.nodeType === Node.TEXT_NODE) {
      const parent = node.parentElement?.tagName;
      // Whitespace between list items isn't text.
      if ((parent === "UL" || parent === "OL") && !node.textContent?.trim()) return;
      text(node.textContent ?? "", fmt, list);
      return;
    }
    if (!(node instanceof HTMLElement)) return;
    const tag = node.tagName;
    if (tag === "BR") {
      if (!cur) start(list);
      cur = null;
      return;
    }
    if (tag === "UL" || tag === "OL") {
      cur = null;
      const inner: ListCtx = { kind: tag === "UL" ? "ul" : "ol" };
      if (tag === "OL") inner.start = (node as HTMLOListElement).start || 1;
      for (const child of node.childNodes) walk(child, fmt, inner);
      cur = null;
      return;
    }
    const next = formatOf(node, fmt);
    if (BLOCK_TAGS.has(tag)) {
      cur = null;
      const before = blocks.length;
      for (const child of node.childNodes) walk(child, next, list);
      // An empty line is an element with nothing (or only a <br>) in it.
      if (blocks.length === before) start(list);
      cur = null;
      return;
    }
    for (const child of node.childNodes) walk(child, next, list);
  };

  for (const child of root.childNodes) walk(child, { bold: false, italic: false }, null);
  return blocks;
}

function formatOf(el: HTMLElement, fmt: Fmt): Fmt {
  const tag = el.tagName;
  let { bold, italic } = fmt;
  if (tag === "B" || tag === "STRONG") bold = true;
  if (tag === "I" || tag === "EM") italic = true;
  const weight = el.style.fontWeight;
  if (weight === "bold" || weight === "bolder" || Number(weight) >= 600) bold = true;
  else if (weight === "normal" || weight === "lighter" || (weight && Number(weight) < 600)) bold = false;
  if (el.style.fontStyle === "italic" || el.style.fontStyle === "oblique") italic = true;
  else if (el.style.fontStyle === "normal") italic = false;
  return { bold, italic };
}

// ---------------------------------------------------------------------------------------------
// Editing

const COMMANDS: Record<FormatCommand, string> = {
  bold: "bold",
  italic: "italic",
  bullets: "insertUnorderedList",
  numbers: "insertOrderedList",
};

/** Applies (or removes) a format at the selection in the focused note body. */
export function applyFormat(cmd: FormatCommand) {
  document.execCommand("styleWithCSS", false, "false");
  document.execCommand(COMMANDS[cmd]);
}

/** Which formats the selection has, for the menu's check marks. */
export function formatState(): Record<FormatCommand, boolean> {
  const q = (c: FormatCommand) => {
    try {
      return document.queryCommandState(COMMANDS[c]);
    } catch {
      return false;
    }
  };
  return { bold: q("bold"), italic: q("italic"), bullets: q("bullets"), numbers: q("numbers") };
}

/** A formatting shortcut that a browser action took (Ctrl+B, Ctrl+I are bound by default)
 *  formats the note being typed in instead. True when it was used. */
export function formatFromShortcut(keys: { ctrl: boolean; alt: boolean; shift: boolean; vk: number }): boolean {
  const cmd = formatCommand(keys);
  const el = document.activeElement;
  if (!cmd || !(el instanceof HTMLElement) || !el.hasAttribute(BODY_ATTR)) return false;
  applyFormat(cmd);
  return true;
}

/** The selection, when it is inside `root`. */
export function selectionIn(root: HTMLElement): Range | null {
  const sel = getSelection();
  if (!sel?.rangeCount) return null;
  const r = sel.getRangeAt(0);
  return root.contains(r.commonAncestorContainer) ? r.cloneRange() : null;
}

/** Focuses `root` with `range` selected (or the caret at its start or end). */
export function selectIn(root: HTMLElement, range: Range | null, atEnd = true) {
  root.focus({ preventScroll: true });
  const sel = getSelection();
  if (!sel) return;
  let r = range;
  if (!r) {
    r = document.createRange();
    r.selectNodeContents(root);
    r.collapse(!atEnd);
  }
  sel.removeAllRanges();
  sel.addRange(r);
}

/** Typing "- ", "* " or "1. " at the start of a plain line turns it into a list item, as in
 *  most editors. Call after a space was typed; true when it converted the line. */
export function autoList(root: HTMLElement): boolean {
  const sel = getSelection();
  if (!sel?.rangeCount || !sel.isCollapsed) return false;
  const caret = sel.getRangeAt(0);
  let line: Node | null = caret.startContainer;
  while (line && line.parentNode !== root) line = line.parentNode;
  if (!line || (line instanceof HTMLElement && (line.tagName === "UL" || line.tagName === "OL"))) return false;
  const before = document.createRange();
  before.setStart(line, 0);
  before.setEnd(caret.startContainer, caret.startOffset);
  const typed = before.toString().replace(/\u00a0/g, " ");
  const cmd: FormatCommand | null = typed === "- " || typed === "* " ? "bullets" : typed === "1. " ? "numbers" : null;
  if (!cmd) return false;
  sel.removeAllRanges();
  sel.addRange(before);
  document.execCommand("delete");
  applyFormat(cmd);
  return true;
}
