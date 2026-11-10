import { test } from "node:test";
import assert from "node:assert/strict";
import { applyClick, emptySelection, prune, selectAll } from "./selection.ts";

const ids = [1, 2, 3, 4, 5];
const none = { ctrl: false, shift: false };

test("ctrl-click toggles rows and moves the anchor", () => {
  let s = applyClick(ids, emptySelection(), 2, { ctrl: true, shift: false });
  s = applyClick(ids, s, 4, { ctrl: true, shift: false });
  assert.deepEqual([...s.selected], [2, 4]);
  assert.equal(s.anchor, 4);
  s = applyClick(ids, s, 2, { ctrl: true, shift: false });
  assert.deepEqual([...s.selected], [4]);
});

test("shift-click selects the range from the anchor, either direction", () => {
  let s = applyClick(ids, emptySelection(), 2, { ctrl: true, shift: false });
  s = applyClick(ids, s, 4, { ctrl: false, shift: true });
  assert.deepEqual([...s.selected].sort(), [2, 3, 4]);
  s = applyClick(ids, s, 1, { ctrl: false, shift: true });
  assert.deepEqual([...s.selected].sort(), [1, 2]);
});

test("ctrl+shift keeps the existing selection and adds the range", () => {
  let s = applyClick(ids, emptySelection(), 1, { ctrl: true, shift: false });
  s = applyClick(ids, s, 3, { ctrl: true, shift: false });
  s = applyClick(ids, s, 5, { ctrl: true, shift: true });
  assert.deepEqual([...s.selected].sort(), [1, 3, 4, 5]);
});

test("shift-click with no anchor selects just that row", () => {
  const s = applyClick(ids, emptySelection(), 3, { ctrl: false, shift: true });
  assert.deepEqual([...s.selected], [3]);
  assert.equal(s.anchor, 3);
});

test("a click without modifiers toggles like a checkbox", () => {
  const s = applyClick(ids, emptySelection(), 3, none);
  assert.deepEqual([...s.selected], [3]);
});

test("selectAll and prune", () => {
  const all = selectAll(ids);
  assert.equal(all.selected.size, 5);
  const p = prune(all, [2, 3]);
  assert.deepEqual([...p.selected].sort(), [2, 3]);
  assert.equal(p.anchor, null);
});
