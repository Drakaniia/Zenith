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
  assert.deepEqual(keys.bookmark, ["Ctrl+D", "Ctrl+Shift+D"]);
  assert.deepEqual(keys.bookmarkAllTabs, []);
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
  assert.deepEqual(merged.bookmarkAllTabs, []);
  const owner = new Map<string, string>();
  for (const [action, ks] of Object.entries(merged)) {
    for (const k of ks) {
      assert.ok(!owner.has(k), `${k} is bound to both ${owner.get(k)} and ${action}`);
      owner.set(k, action);
    }
  }
});

test("every shortcut in Zen's list has its Zen key by default", () => {
  const keys = defaultShortcuts();
  // Zen's documented defaults (docs.zen-browser.app/user-manual/shortcuts), by section.
  const zen: [string, string][] = [
    ["toggleFloatingToolbar", "Ctrl+Alt+T"],
    ["toggleFloatingSidebar", "Ctrl+Alt+S"],
    ["toggleCompact", "Ctrl+Alt+C"],
    ["prevSpace", "Ctrl+Alt+Q"],
    ["nextSpace", "Ctrl+Alt+E"],
    ["unsplit", "Ctrl+Alt+U"],
    ["splitHorizontal", "Ctrl+Alt+H"],
    ["splitVertical", "Ctrl+Alt+V"],
    ["splitGrid", "Ctrl+Alt+G"],
    ["newWindow", "Ctrl+N"],
    ["newTab", "Ctrl+T"],
    ["closeTab", "Ctrl+W"],
    ["closeWindow", "Ctrl+Shift+W"],
    ["quit", "Ctrl+Q"],
    ["reopenClosed", "Ctrl+Shift+T"],
    ["undoCloseWindow", "Ctrl+Shift+N"],
    ["selectTab1", "Alt+1"],
    ["selectTab8", "Alt+8"],
    ["lastTab", "Alt+9"],
    ["nextTab", "Ctrl+Tab"],
    ["prevTab", "Ctrl+Shift+Tab"],
    ["back", "Alt+Left"],
    ["forward", "Alt+Right"],
    ["back", "Ctrl+BracketLeft"],
    ["forward", "Ctrl+BracketRight"],
    ["goHome", "Alt+Home"],
    ["reload", "Ctrl+R"],
    ["hardReload", "Ctrl+Shift+R"],
    ["historySidebar", "Ctrl+H"],
    ["newIncognitoWindow", "Ctrl+Shift+P"],
    ["focusSearch", "Ctrl+K"],
    ["focusSearch", "Ctrl+J"],
    ["find", "Ctrl+F"],
    ["findNext", "Ctrl+G"],
    ["findPrev", "Ctrl+Shift+G"],
    ["openPalette", "Ctrl+L"],
    ["openPalette", "Alt+D"],
    ["savePage", "Ctrl+S"],
    ["print", "Ctrl+P"],
    ["readerMode", "Ctrl+Alt+R"],
    ["pictureInPicture", "Ctrl+Shift+BracketRight"],
    ["viewSource", "Ctrl+U"],
    ["history", "Ctrl+Shift+H"],
    ["bookmark", "Ctrl+D"],
    ["bookmark", "Ctrl+Shift+D"],
    ["bookmarks", "Ctrl+Shift+O"],
    ["muteTab", "Ctrl+M"],
    ["zoomOut", "Ctrl+Minus"],
    ["zoomIn", "Ctrl+Plus"],
    ["zoomReset", "Ctrl+0"],
    ["switchTextDirection", "Ctrl+Shift+X"],
    ["screenshot", "Ctrl+Shift+S"],
    ["devtools", "Ctrl+Shift+I"],
    ["browserToolbox", "Ctrl+Alt+Shift+I"],
    ["browserConsole", "Ctrl+Shift+J"],
    ["responsiveDesign", "Ctrl+Shift+M"],
    ["inspector", "Ctrl+Shift+L"],
    ["webConsole", "Ctrl+Shift+K"],
    ["networkMonitor", "Ctrl+Shift+E"],
    ["styleEditor", "Shift+F7"],
    ["performance", "Shift+F5"],
    ["storage", "Shift+F9"],
    ["accessibility", "Shift+F12"],
    ["copyUrlMarkdown", "Ctrl+Alt+Shift+C"],
    ["copyUrl", "Ctrl+Shift+C"],
    ["toggleSidebar", "Alt+B"],
    ["downloads", "Ctrl+Shift+Y"],
    ["extensions", "Ctrl+Shift+A"],
    ["openFile", "Ctrl+O"],
    ["fullscreen", "F11"],
    ["taskManager", "Shift+Escape"],
    ["toggleBookmarksSidebar", "Ctrl+B"],
    ["toggleBookmarksBar", "Ctrl+Shift+B"],
    ["toggleChatbot", "Ctrl+Alt+X"],
    ["toggleSidebarPanel", "Ctrl+Alt+Z"],
    ["clearData", "Ctrl+Shift+Delete"],
  ];
  for (const [action, key] of zen) {
    assert.ok(keys[action]?.includes(key), `${action} should have ${key}, has ${keys[action]}`);
  }
  // Not set by default in Zen.
  for (let i = 1; i <= 10; i++) assert.deepEqual(keys[`switchSpace${i}`], []);
  assert.deepEqual(keys.resetPinned, []);
});

test("Zen's keys that aren't text edits are allowed", () => {
  assert.equal(isReserved("Shift+Escape"), null);
  assert.equal(isReserved("Ctrl+Shift+Delete"), null);
  assert.ok(isReserved("Ctrl+Delete"));
  assert.ok(isReserved("Escape"));
  assert.ok(isReserved("Shift+K"));
});

test("old defaults that clash with Zen's keys move with them", () => {
  const defaults = defaultShortcuts();
  // A profile saved by the previous version, still on its defaults.
  const saved: Record<string, string[]> = {
    ...defaults,
    newIncognitoWindow: ["Ctrl+Shift+N"],
    commandPalette: ["Ctrl+K", "Ctrl+Shift+P"],
    downloads: ["Ctrl+J"],
    history: ["Ctrl+H"],
    toggleSplit: ["Ctrl+Alt+S"],
    bookmark: ["Ctrl+D"],
    bookmarkAllTabs: ["Ctrl+Shift+D"],
    nextSpace: ["Ctrl+Alt+Right"],
    selectTab1: ["Ctrl+1"],
    extensions: [],
  };
  for (const id of ["undoCloseWindow", "focusSearch", "historySidebar", "toggleFloatingSidebar", "newWindow"])
    delete saved[id];
  const merged = mergeShortcuts(saved);
  assert.deepEqual(merged.newIncognitoWindow, ["Ctrl+Shift+P"]);
  assert.deepEqual(merged.undoCloseWindow, ["Ctrl+Shift+N"]);
  assert.deepEqual(merged.focusSearch, ["Ctrl+K", "Ctrl+J"]);
  assert.deepEqual(merged.commandPalette, ["Ctrl+Alt+P"]);
  assert.deepEqual(merged.downloads, ["Ctrl+Shift+Y"]);
  assert.deepEqual(merged.historySidebar, ["Ctrl+H"]);
  assert.deepEqual(merged.history, ["Ctrl+Shift+H"]);
  assert.deepEqual(merged.toggleFloatingSidebar, ["Ctrl+Alt+S"]);
  assert.deepEqual(merged.toggleSplit, []);
  assert.deepEqual(merged.bookmark, ["Ctrl+D", "Ctrl+Shift+D"]);
  assert.deepEqual(merged.bookmarkAllTabs, []);
  assert.deepEqual(merged.nextSpace, ["Ctrl+Alt+E", "Ctrl+Alt+Right"]);
  assert.deepEqual(merged.selectTab1, ["Ctrl+1", "Alt+1"]);
  assert.deepEqual(merged.extensions, ["Ctrl+Shift+A"]);
  const owner = new Map<string, string>();
  for (const [action, ks] of Object.entries(merged)) {
    for (const k of ks) {
      assert.ok(!owner.has(k), `${k} is bound to both ${owner.get(k)} and ${action}`);
      owner.set(k, action);
    }
  }
  // A binding the user changed by hand stays, and the new action doesn't take its key.
  const mine = mergeShortcuts({ ...saved, commandPalette: ["Ctrl+K"] });
  assert.deepEqual(mine.commandPalette, ["Ctrl+K"]);
  assert.deepEqual(mine.focusSearch, ["Ctrl+J"]);
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
