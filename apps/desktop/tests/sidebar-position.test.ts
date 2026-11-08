import { test } from "node:test";
import assert from "node:assert/strict";
import {
  SIDEBAR_POSITIONS,
  effectiveMode,
  isStrip,
  normalizeSidebarPosition,
} from "../src/lib/sidebar-position.ts";

test("the sidebar position defaults to the left and keeps only known edges", () => {
  assert.equal(normalizeSidebarPosition(undefined), "left");
  assert.equal(normalizeSidebarPosition("middle"), "left");
  assert.equal(normalizeSidebarPosition(3), "left");
  for (const p of ["left", "right", "top", "bottom"]) assert.equal(normalizeSidebarPosition(p), p);
  assert.deepEqual(
    SIDEBAR_POSITIONS.map((p) => p.value),
    ["left", "right", "top", "bottom"],
  );
});

test("top and bottom are tab strips", () => {
  assert.equal(isStrip("left"), false);
  assert.equal(isStrip("right"), false);
  assert.equal(isStrip("top"), true);
  assert.equal(isStrip("bottom"), true);
});

test("a tab strip keeps the toolbar: single toolbar shows as multiple there", () => {
  assert.equal(effectiveMode("single", "left"), "single");
  assert.equal(effectiveMode("single", "right"), "single");
  assert.equal(effectiveMode("single", "top"), "multiple");
  assert.equal(effectiveMode("single", "bottom"), "multiple");
  assert.equal(effectiveMode("multiple", "top"), "multiple");
  assert.equal(effectiveMode("collapsed", "bottom"), "collapsed");
  assert.equal(effectiveMode("collapsed", "right"), "collapsed");
});
