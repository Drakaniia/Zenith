import { test } from "node:test";
import assert from "node:assert/strict";
import {
  centerSquare,
  cleanProfileName,
  deleteBlocker,
  emojiName,
  nextProfileColor,
  profileInitial,
  textOn,
} from "./profiles.ts";
import type { ProfileEntry } from "./types.ts";

const entry = (id: string, extra: Partial<ProfileEntry> = {}): ProfileEntry => ({
  id,
  name: id[0].toUpperCase() + id.slice(1),
  color: "#3b82f6",
  isDefault: id === "default",
  running: false,
  shortcut: false,
  ...extra,
});

test("avatar initials", () => {
  assert.equal(profileInitial("work"), "W");
  assert.equal(profileInitial("  école"), "É");
  assert.equal(profileInitial("😀 Fun"), "😀");
  assert.equal(profileInitial(""), "?");
});

test("names are cleaned like the backend does", () => {
  assert.equal(cleanProfileName("  Work \n stuff\t"), "Work stuff");
  assert.equal(cleanProfileName("   "), "");
  assert.equal(Array.from(cleanProfileName("x".repeat(60))).length, 40);
});

test("readable avatar text", () => {
  assert.equal(textOn("#f59e0b"), "#1a1a1a");
  assert.equal(textOn("#1e3a8a"), "#fff");
  assert.equal(textOn("nope"), "#fff");
});

test("new profiles get an unused colour", () => {
  const colors = ["#8b7cf6", "#3b82f6", "#14b8a6"];
  assert.equal(nextProfileColor(colors, [{ color: "#8B7CF6" }]), "#3b82f6");
  assert.equal(nextProfileColor(colors, colors.map((color) => ({ color }))), "#8b7cf6");
  assert.equal(nextProfileColor([], []), "#8b7cf6");
});

test("emoji avatars name their emoji", () => {
  assert.equal(emojiName("emoji:cat-face"), "cat-face");
  assert.equal(emojiName("emoji:"), null);
  assert.equal(emojiName("image"), null);
  assert.equal(emojiName(null), null);
  assert.equal(emojiName(undefined), null);
});

test("chosen pictures are cropped to their centred square", () => {
  assert.deepEqual(centerSquare(400, 300), { x: 50, y: 0, size: 300 });
  assert.deepEqual(centerSquare(300, 401), { x: 0, y: 51, size: 300 });
  assert.deepEqual(centerSquare(256, 256), { x: 0, y: 0, size: 256 });
});

test("which profiles can be deleted", () => {
  assert.match(deleteBlocker(entry("default"), "work")!, /Default/);
  assert.match(deleteBlocker(entry("work"), "work")!, /Switch/);
  assert.match(deleteBlocker(entry("work", { running: true }), "default")!, /Close the windows of “Work”/);
  assert.equal(deleteBlocker(entry("work"), "default"), null);
});
