// The notes editor's data and the pure helpers its page uses (no Svelte state, so the Node
// tests can import them). The store itself is notes.svelte.ts. A note's body is formatted text
// (rich-text.ts); titles, previews, search and copying use its plain text.

import { noteText } from "./rich-text.ts";

export interface Note {
  id: string;
  title: string;
  body: string;
  /** Unix ms. */
  created: number;
  /** Unix ms of the last edit. */
  updated: number;
}

export const UNTITLED = "Untitled note";

export function newNoteId(): string {
  return crypto.randomUUID();
}

export function isEmptyNote(n: Pick<Note, "title" | "body">): boolean {
  return !n.title.trim() && !n.body.trim();
}

function firstLine(text: string): string {
  return (
    text
      .split("\n")
      .map((l) => l.trim())
      .find(Boolean) ?? ""
  );
}

/** The title shown for a note: its own, else its first line of text. */
export function noteTitle(n: Pick<Note, "title" | "body">): string {
  const own = n.title.trim();
  if (own) return own;
  const line = firstLine(noteText(n.body));
  return line ? line.slice(0, 80) : UNTITLED;
}

/** A one-line excerpt of the text, skipping the line already used as the title. */
export function notePreview(n: Pick<Note, "title" | "body">, max = 120): string {
  let text = noteText(n.body).trim();
  if (!n.title.trim()) {
    const line = firstLine(text);
    text = text.slice(text.indexOf(line) + line.length);
  }
  return text.replace(/\s+/g, " ").trim().slice(0, max);
}

/** Every whitespace-separated term of `query` appears in the title or the text. */
export function matchesQuery(n: Pick<Note, "title" | "body">, query: string): boolean {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return true;
  const hay = `${n.title}\n${noteText(n.body)}`.toLowerCase();
  return terms.every((t) => hay.includes(t));
}

/** Notes matching `query`, most recently edited first. */
export function filterNotes<T extends Note>(notes: T[], query: string): T[] {
  return notes
    .filter((n) => matchesQuery(n, query))
    .sort((a, b) => b.updated - a.updated || b.created - a.created);
}

function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** Days between the calendar days of `ms` and `now` (0 = same day). */
function daysAgo(ms: number, now: number): number {
  return Math.round((startOfDay(now) - startOfDay(ms)) / 86_400_000);
}

export type NoteGroup = "Today" | "Yesterday" | "Previous 7 days" | "Previous 30 days" | "Older";

export function groupOf(ms: number, now = Date.now()): NoteGroup {
  const d = daysAgo(ms, now);
  if (d <= 0) return "Today";
  if (d === 1) return "Yesterday";
  if (d < 7) return "Previous 7 days";
  if (d < 30) return "Previous 30 days";
  return "Older";
}

/** Notes (already sorted) in runs of the same group, for the list's section headers. */
export function groupNotes<T extends Note>(notes: T[], now = Date.now()): { label: NoteGroup; items: T[] }[] {
  const out: { label: NoteGroup; items: T[] }[] = [];
  for (const n of notes) {
    const label = groupOf(n.updated, now);
    const last = out[out.length - 1];
    if (last?.label === label) last.items.push(n);
    else out.push({ label, items: [n] });
  }
  return out;
}

const time = (ms: number) =>
  new Date(ms).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

/** When a note was last edited: "Just now", "5 min ago", "14:02", "Yesterday, 14:02", "Mar 4". */
export function formatEdited(ms: number, now = Date.now()): string {
  const s = (now - ms) / 1000;
  if (s < 45) return "Just now";
  if (s < 3600 && s >= 0) return `${Math.max(1, Math.round(s / 60))} min ago`;
  const d = daysAgo(ms, now);
  if (d <= 0) return time(ms);
  if (d === 1) return `Yesterday, ${time(ms)}`;
  const date = new Date(ms);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

export function wordCount(text: string): number {
  const words = text.trim().match(/\S+/g);
  return words ? words.length : 0;
}

/** What "Copy" puts on the clipboard: the plain text (lists keep their bullets and numbers),
 *  or the title for a note without text. */
export function copyableText(n: Pick<Note, "title" | "body">): string {
  return n.body.trim() ? noteText(n.body, true) : n.title.trim();
}
