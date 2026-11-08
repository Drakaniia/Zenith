import { test } from "node:test";
import assert from "node:assert/strict";
import {
  UNTITLED,
  copyableText,
  filterNotes,
  formatEdited,
  groupNotes,
  groupOf,
  isEmptyNote,
  matchesQuery,
  notePreview,
  noteTitle,
  wordCount,
  type Note,
} from "./notes.ts";

const note = (id: string, title: string, body: string, updated = 0): Note => ({
  id,
  title,
  body,
  created: 0,
  updated,
});

test("a note's title is its own, else its first line of text", () => {
  assert.equal(noteTitle(note("a", "  Groceries ", "milk")), "Groceries");
  assert.equal(noteTitle(note("a", "", "\n\n  Call Anna  \nabout Friday")), "Call Anna");
  assert.equal(noteTitle(note("a", " ", "  \n ")), UNTITLED);
});

test("the preview skips the line used as the title", () => {
  assert.equal(notePreview(note("a", "", "Call Anna\nabout   Friday\nand Sunday")), "about Friday and Sunday");
  assert.equal(notePreview(note("a", "Plans", "Call Anna\nabout Friday")), "Call Anna about Friday");
  assert.equal(notePreview(note("a", "", "only line")), "");
  assert.equal(notePreview(note("a", "T", "x".repeat(300)), 10).length, 10);
});

test("search matches every term in the title or the text, ignoring case", () => {
  const n = note("a", "Trip to Lisbon", "Book the HOTEL and trains");
  assert.ok(matchesQuery(n, ""));
  assert.ok(matchesQuery(n, "lisbon hotel"));
  assert.ok(matchesQuery(n, "  TRAINS "));
  assert.ok(!matchesQuery(n, "lisbon flights"));
});

test("notes are listed most recently edited first", () => {
  const list = [note("old", "a", "", 10), note("new", "b", "", 30), note("mid", "c", "", 20)];
  assert.deepEqual(filterNotes(list, "").map((n) => n.id), ["new", "mid", "old"]);
  assert.deepEqual(filterNotes(list, "c").map((n) => n.id), ["mid"]);
  // The input order is left alone.
  assert.equal(list[0].id, "old");
});

test("notes are grouped by when they were edited", () => {
  const now = new Date(2026, 10, 7, 15, 0).getTime();
  const at = (days: number, hour = 10) => new Date(2026, 10, 7 - days, hour).getTime();
  assert.equal(groupOf(at(0, 1), now), "Today");
  assert.equal(groupOf(at(1, 23), now), "Yesterday");
  assert.equal(groupOf(at(6), now), "Previous 7 days");
  assert.equal(groupOf(at(20), now), "Previous 30 days");
  assert.equal(groupOf(at(400), now), "Older");
  const groups = groupNotes([note("a", "", "", at(0)), note("b", "", "", at(0)), note("c", "", "", at(3))], now);
  assert.deepEqual(groups.map((g) => [g.label, g.items.length]), [["Today", 2], ["Previous 7 days", 1]]);
});

test("the edit time reads naturally", () => {
  const now = new Date(2026, 10, 7, 15, 0).getTime();
  assert.equal(formatEdited(now - 10_000, now), "Just now");
  assert.equal(formatEdited(now - 5 * 60_000, now), "5 min ago");
  assert.match(formatEdited(new Date(2026, 10, 7, 9, 5).getTime(), now), /9.*05/);
  assert.match(formatEdited(new Date(2026, 10, 6, 9, 5).getTime(), now), /^Yesterday, /);
  assert.doesNotMatch(formatEdited(new Date(2026, 2, 4).getTime(), now), /2026/);
  assert.match(formatEdited(new Date(2024, 2, 4).getTime(), now), /2024/);
});

test("empty notes, word counts and copied text", () => {
  assert.ok(isEmptyNote(note("a", " ", "\n\t")));
  assert.ok(!isEmptyNote(note("a", "", "x")));
  assert.equal(wordCount("  one two\nthree  "), 3);
  assert.equal(wordCount("   "), 0);
  assert.equal(copyableText(note("a", "Title", "Body")), "Body");
  assert.equal(copyableText(note("a", "Title", "  ")), "Title");
});
