import { test } from "node:test";
import assert from "node:assert/strict";
import {
  ACTIONS,
  chordFromEvent,
  defaultShortcuts,
  displayChord,
  isReserved,
  mergeShortcuts,
} from "../src/lib/shortcuts.ts";

test("no chord is bound to two actions by default", () => {
  const owner = new Map<string, string>();
  for (const a of ACTIONS) {
    for (const k of a.keys) {
      assert.ok(
        !owner.has(k),
        `${k} is bound to both ${owner.get(k)} and ${a.id}`,
      );
      owner.set(k, a.id);
    }
  }
});

test("default bindings never use keys reserved for pages", () => {
  for (const [action, keys] of Object.entries(defaultShortcuts())) {
    for (const k of keys) assert.equal(isReserved(k), null, `${action}: ${k}`);
  }
});

test("editing keys are reserved", () => {
  for (const k of [
    "Ctrl+C",
    "Ctrl+V",
    "Ctrl+X",
    "Ctrl+A",
    "Ctrl+Z",
    "Ctrl+Y",
    "Ctrl+Shift+Z",
    "Ctrl+Shift+V",
    "Ctrl+Backspace",
    "Ctrl+Left",
  ]) {
    assert.ok(isReserved(k), k);
  }
  assert.ok(isReserved("K"), "a bare letter needs a modifier");
  assert.equal(isReserved("F11"), null);
  assert.equal(isReserved("Ctrl+T"), null);
  assert.equal(isReserved("Ctrl+Alt+Left"), null);
});

test("chords are recorded from keyboard events by physical key", () => {
  const ev = (code: string, mods: Partial<KeyboardEvent> = {}) =>
    ({
      code,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      ...mods,
    }) as KeyboardEvent;
  assert.equal(
    chordFromEvent(ev("KeyT", { ctrlKey: true, shiftKey: true })),
    "Ctrl+Shift+T",
  );
  assert.equal(chordFromEvent(ev("Equal", { ctrlKey: true })), "Ctrl+Plus");
  assert.equal(chordFromEvent(ev("Digit3", { ctrlKey: true })), "Ctrl+3");
  assert.equal(chordFromEvent(ev("F5")), "F5");
  assert.equal(chordFromEvent(ev("ShiftLeft", { shiftKey: true })), null);
});

test("chords display with symbols", () => {
  assert.deepEqual(displayChord("Ctrl+Alt+Left"), ["Ctrl", "Alt", "←"]);
  assert.deepEqual(displayChord("Ctrl+Plus"), ["Ctrl", "+"]);
});

test("bookmark shortcuts follow Zen", () => {
  const keys = defaultShortcuts();
  assert.deepEqual(keys.bookmark, ["Ctrl+D"]);
  assert.deepEqual(keys.bookmarkAllTabs, ["Ctrl+Shift+D"]);
  assert.deepEqual(keys.toggleBookmarksSidebar, ["Ctrl+B"]);
  assert.deepEqual(keys.toggleBookmarksBar, ["Ctrl+Shift+B"]);
  assert.deepEqual(keys.bookmarks, ["Ctrl+Shift+O"]);
  // Compact mode and the sidebar width moved to the keys Zen gives them.
  assert.deepEqual(keys.toggleCompact, ["Ctrl+Alt+C"]);
  assert.deepEqual(keys.toggleSidebar, ["Alt+B"]);
});

test("an existing profile follows the moved defaults without two actions on one key", () => {
  const defaults = defaultShortcuts();
  // What the previous version saved: every action, with the old Ctrl+B and Ctrl+Shift+B owners.
  const saved: Record<string, string[]> = { ...defaults, toggleCompact: ["Ctrl+B"], toggleSidebar: ["Ctrl+Shift+B"] };
  delete saved.toggleBookmarksSidebar;
  delete saved.toggleBookmarksBar;
  delete saved.bookmarkAllTabs;
  const merged = mergeShortcuts(saved);
  assert.deepEqual(merged.toggleCompact, ["Ctrl+Alt+C"], "an untouched old default moves");
  assert.deepEqual(merged.toggleSidebar, ["Alt+B"]);
  assert.deepEqual(merged.toggleBookmarksSidebar, ["Ctrl+B"]);
  assert.deepEqual(merged.toggleBookmarksBar, ["Ctrl+Shift+B"]);
  assert.deepEqual(merged.bookmarkAllTabs, ["Ctrl+Shift+D"]);
  const owner = new Map<string, string>();
  for (const [action, ks] of Object.entries(merged)) {
    for (const k of ks) {
      assert.ok(!owner.has(k), `${k} is bound to both ${owner.get(k)} and ${action}`);
      owner.set(k, action);
    }
  }
});

test("the user's own bindings win over defaults", () => {
  const defaults = defaultShortcuts();
  // Compact mode was rebound by hand; the new bookmarks sidebar keeps its default.
  let merged = mergeShortcuts({ toggleCompact: ["Ctrl+Alt+K"] });
  assert.deepEqual(merged.toggleCompact, ["Ctrl+Alt+K"]);
  assert.deepEqual(merged.toggleBookmarksSidebar, ["Ctrl+B"]);
  // A cleared binding stays cleared.
  merged = mergeShortcuts({ bookmark: [] });
  assert.deepEqual(merged.bookmark, []);
  // A new action's default never takes a key the user already uses for something else.
  merged = mergeShortcuts({ pinTab: ["Ctrl+B"], toggleCompact: ["Ctrl+Alt+K"] });
  assert.deepEqual(merged.pinTab, ["Ctrl+B"]);
  assert.deepEqual(merged.toggleBookmarksSidebar, [], "Ctrl+B is taken, so the new action starts unbound");
  // Nothing saved: the defaults.
  assert.deepEqual(mergeShortcuts(undefined), defaults);
  // Bindings for actions this version doesn't know are kept.
  assert.deepEqual(mergeShortcuts({ legacyAction: ["Ctrl+Alt+9"] }).legacyAction, ["Ctrl+Alt+9"]);
});
