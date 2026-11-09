// Formatted notes. A note's text is stored as a small subset of Markdown, so it stays readable
// plain text and every note written before formatting existed opens unchanged:
//   **bold**   *italic*   ***both***   "- " bulleted lines   "1. " numbered lines
// `\*` and `\\` are a literal asterisk and backslash. Each line is one block: a paragraph or a
// list item. Pure functions only; rich-dom.ts turns blocks into the editor's DOM and back.

export interface Run {
  text: string;
  bold?: boolean;
  italic?: boolean;
}

export type BlockKind = "p" | "ul" | "ol";

export interface Block {
  kind: BlockKind;
  runs: Run[];
  /** A numbered item's own number; following items of the same list count on from it. */
  n?: number;
}

const BULLET = /^[-*] /;
const NUMBER = /^(\d{1,9})\. /;
/** A paragraph that would read as a list item is stored with a leading backslash. */
const LIST_LIKE = /^([-*] |\d{1,9}\. )/;

const isSpace = (c: string | undefined) => c === undefined || /\s/.test(c);

// ---------------------------------------------------------------------------------------------
// Reading

export function parseNote(md: string): Block[] {
  if (!md) return [];
  return md.split(/\r?\n/).map(parseLine);
}

function parseLine(line: string): Block {
  if (line.startsWith("\\") && LIST_LIKE.test(line.slice(1))) return { kind: "p", runs: parseInline(line.slice(1)) };
  if (BULLET.test(line)) return { kind: "ul", runs: parseInline(line.slice(2)) };
  const m = NUMBER.exec(line);
  if (m) return { kind: "ol", n: Number(m[1]), runs: parseInline(line.slice(m[0].length)) };
  return { kind: "p", runs: parseInline(line) };
}

/** Is there a closing `marker` (right after a non-space) at or after `from`? */
function hasCloser(s: string, from: number, marker: string): boolean {
  for (let j = from; j < s.length; j++) {
    if (s[j] === "\\" && (s[j + 1] === "*" || s[j + 1] === "\\")) {
      j++;
      continue;
    }
    if (s.startsWith(marker, j) && !isSpace(s[j - 1])) return true;
  }
  return false;
}

/** One line's text as runs. A marker opens only before a non-space with a closer later on the
 *  line, and closes only after a non-space, so "5 * 3" or a lone "*" stay plain text. */
export function parseInline(s: string): Run[] {
  const runs: Run[] = [];
  let buf = "";
  let bold = false;
  let italic = false;
  const flush = () => {
    if (buf) runs.push(run(buf, bold, italic));
    buf = "";
  };
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === "\\" && (s[i + 1] === "*" || s[i + 1] === "\\")) {
      buf += s[i + 1];
      i += 2;
      continue;
    }
    if (c === "*") {
      const pair = s.startsWith("**", i);
      // Closing first: "*a***b**" closes the italic before it opens the bold.
      if (pair && bold && !isSpace(s[i - 1])) {
        flush();
        bold = false;
        i += 2;
        continue;
      }
      if (italic && !isSpace(s[i - 1])) {
        flush();
        italic = false;
        i += 1;
        continue;
      }
      if (pair && !bold && !isSpace(s[i + 2]) && hasCloser(s, i + 2, "**")) {
        flush();
        bold = true;
        i += 2;
        continue;
      }
      // An italic marker is never followed by another asterisk (bold opens first, and a literal
      // one is escaped), so "** x **" stays text instead of reading as two italic markers.
      if (!italic && !pair && !isSpace(s[i + 1]) && hasCloser(s, i + 1, "*")) {
        flush();
        italic = true;
        i += 1;
        continue;
      }
    }
    buf += c;
    i++;
  }
  flush();
  return runs;
}

function run(text: string, bold: boolean, italic: boolean): Run {
  const r: Run = { text };
  if (bold) r.bold = true;
  if (italic) r.italic = true;
  return r;
}

// ---------------------------------------------------------------------------------------------
// Writing

export function serializeNote(blocks: Block[]): string {
  let num = 0;
  return blocks
    .map((b, i) => {
      const text = serializeInline(b.runs);
      if (b.kind === "ul") return `- ${text}`;
      if (b.kind === "ol") {
        num = blocks[i - 1]?.kind === "ol" ? num + 1 : (b.n ?? 1);
        return `${num}. ${text}`;
      }
      return LIST_LIKE.test(text) ? `\\${text}` : text;
    })
    .join("\n");
}

/** Joins neighbours with the same formatting and drops empty runs. Spaces at the edges of a
 *  formatted run move outside it: a marker can't sit next to a space. */
export function normalizeRuns(runs: Run[]): Run[] {
  const out: Run[] = [];
  const push = (r: Run) => {
    if (!r.text) return;
    const last = out[out.length - 1];
    if (last && !!last.bold === !!r.bold && !!last.italic === !!r.italic) last.text += r.text;
    else out.push(run(r.text, !!r.bold, !!r.italic));
  };
  for (const r of runs) {
    if (!r.bold && !r.italic) {
      push(r);
      continue;
    }
    const m = /^(\s*)([\s\S]*?)(\s*)$/.exec(r.text)!;
    push({ text: m[1] });
    push({ text: m[2], bold: r.bold, italic: r.italic });
    push({ text: m[3] });
  }
  return out;
}

function serializeInline(runs: Run[]): string {
  let out = "";
  const list = normalizeRuns(runs);
  list.forEach((r, i) => {
    const marks = (r.bold ? "**" : "") + (r.italic ? "*" : "");
    const plain = !marks;
    // Formatting is reopened in full for every run (bold outside italic), which keeps
    // neighbouring markers unambiguous for parseInline.
    out += marks + escapeText(r.text, plain, plain && i === list.length - 1) + marks;
  });
  return out;
}

/** Escapes what parseInline would read as markup. In plain text an asterisk can only be markup
 *  when it could open (before a non-space), so one before a space or at the end of the line is
 *  left alone ("5 * 3", "3*"); inside formatting every asterisk is escaped. A backslash is
 *  escaped before an asterisk, another backslash or a marker. */
function escapeText(t: string, plain: boolean, lineEnd: boolean): string {
  let out = "";
  for (let i = 0; i < t.length; i++) {
    const c = t[i];
    if (c === "*") {
      const next = t[i + 1];
      const loose = plain && (next === undefined ? lineEnd : /\s/.test(next));
      out += loose ? "*" : "\\*";
    } else if (c === "\\") {
      const next = t[i + 1];
      out += (next === undefined && !lineEnd) || next === "*" || next === "\\" ? "\\\\" : "\\";
    } else out += c;
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Plain text

/** The note as plain text, for titles, previews, search and word counts; with `markers`, list
 *  items keep a "• " or "1. " in front (for copying). */
export function noteText(md: string, markers = false): string {
  const blocks = parseNote(md);
  let num = 0;
  return blocks
    .map((b, i) => {
      const text = b.runs.map((r) => r.text).join("");
      if (!markers || b.kind === "p") return text;
      if (b.kind === "ul") return `• ${text}`;
      num = blocks[i - 1]?.kind === "ol" ? num + 1 : (b.n ?? 1);
      return `${num}. ${text}`;
    })
    .join("\n");
}

// ---------------------------------------------------------------------------------------------
// Formatting keys

export type FormatCommand = "bold" | "italic" | "bullets" | "numbers";

/** The formatting a key chord applies in a note: Ctrl+B, Ctrl+I, and Google Docs' list keys
 *  Ctrl+Shift+8 (bullets) and Ctrl+Shift+7 (numbers). `vk` is the Windows virtual-key code. */
export function formatCommand(k: { ctrl: boolean; alt: boolean; shift: boolean; vk: number }): FormatCommand | null {
  if (!k.ctrl || k.alt) return null;
  if (!k.shift && k.vk === 0x42) return "bold";
  if (!k.shift && k.vk === 0x49) return "italic";
  if (k.shift && k.vk === 0x38) return "bullets";
  if (k.shift && k.vk === 0x37) return "numbers";
  return null;
}
