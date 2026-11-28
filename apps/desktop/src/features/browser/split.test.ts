import test from "node:test";
import assert from "node:assert/strict";
import {
  MIN_PANE_SHARE,
  defaultRatios,
  detachFromSplits,
  effectiveRatios,
  resizeAdjacent,
  resizeGridAxis,
  spaceSplits,
} from "./split-math.ts";
import type { Space } from "./types.ts";

test("defaultRatios and effectiveRatios normalize linear and grid splits", () => {
  assert.deepEqual(defaultRatios(3, "vsep"), [1 / 3, 1 / 3, 1 / 3]);
  assert.deepEqual(defaultRatios(4, "grid"), [0.5, 0.5]);
  assert.deepEqual(effectiveRatios({ ids: ["a", "b"], ratio: 0.3 }), [0.3, 0.7]);
  assert.deepEqual(effectiveRatios({ ids: ["a", "b", "c"], ratio: 0.5, ratios: [2, 1, 1] }), [0.5, 0.25, 0.25]);
  assert.deepEqual(effectiveRatios({ ids: ["a", "b", "c"], ratio: 0.5, layout: "grid" }), [0.5, 0.5]);
});

test("resizeAdjacent adjusts only the neighboring pair and enforces floor", () => {
  const start = [0.3, 0.4, 0.3];
  const moved = resizeAdjacent(start, 0, 0.5);
  assert.ok(Math.abs(moved[0] - 0.5) < 1e-9);
  assert.ok(Math.abs(moved[1] - 0.2) < 1e-9);
  assert.equal(moved[2], 0.3, "non-adjacent pane stays untouched");

  const clamped = resizeAdjacent(start, 0, 0.99);
  assert.ok(Math.abs(clamped[1] - MIN_PANE_SHARE) < 1e-9);
  assert.ok(Math.abs(clamped[0] - (0.7 - MIN_PANE_SHARE)) < 1e-9);

  const grid = resizeGridAxis([0.5, 0.5], 0, 0.01);
  assert.equal(grid[0], MIN_PANE_SHARE);
  assert.equal(grid[1], 0.5);
});

test("detachFromSplits shrinks 3-pane split to 2 panes and drops 2-pane split", () => {
  const space: Space = {
    id: "s1",
    name: "Home",
    icon: "🏠",
    color: "#8b7cf6",
    tabIds: ["a", "b", "c", "d", "e"],
    pinnedIds: [],
    splits: [
      { ids: ["a", "b", "c"], ratio: 1 / 3, layout: "vsep" },
      { ids: ["d", "e"], ratio: 0.5, layout: "hsep" },
    ],
  };
  detachFromSplits(space, "b");
  assert.deepEqual(spaceSplits(space).map((g) => g.ids), [["a", "c"], ["d", "e"]]);
  detachFromSplits(space, "d");
  assert.deepEqual(spaceSplits(space).map((g) => g.ids), [["a", "c"]]);
});
