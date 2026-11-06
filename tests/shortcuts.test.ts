import { test } from "node:test";
import assert from "node:assert/strict";
import {
  ACTIONS,
  chordFromEvent,
  defaultShortcuts,
  displayChord,
  isReserved,
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
