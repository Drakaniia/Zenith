import { test } from "node:test";
import assert from "node:assert/strict";
import {
  clampDevToolsSize,
  DEFAULT_DEVTOOLS_LAYOUT,
  MIN_CONTENT_SIZE,
  MIN_DEVTOOLS_SIZE,
  parseDevToolsLayout,
} from "./devtools.ts";

test("clampDevToolsSize keeps both DevTools and page area usable", () => {
  assert.equal(clampDevToolsSize(420, 1200), 420);
  assert.equal(clampDevToolsSize(100, 1200), MIN_DEVTOOLS_SIZE);
  assert.equal(clampDevToolsSize(1100, 1200), 1200 - MIN_CONTENT_SIZE);
  // Very small window falls back to half container rather than overflowing.
  assert.equal(clampDevToolsSize(50, 320), 160);
  assert.equal(clampDevToolsSize(Number.NaN, 1200), MIN_DEVTOOLS_SIZE);
});

test("parseDevToolsLayout handles missing, corrupt, and valid values", () => {
  assert.deepEqual(parseDevToolsLayout(null), DEFAULT_DEVTOOLS_LAYOUT);
  assert.deepEqual(parseDevToolsLayout("{broken"), DEFAULT_DEVTOOLS_LAYOUT);
  assert.deepEqual(parseDevToolsLayout(JSON.stringify({ dock: "left", width: 10, height: "x" })), {
    dock: "right",
    width: MIN_DEVTOOLS_SIZE,
    height: DEFAULT_DEVTOOLS_LAYOUT.height,
  });
  assert.deepEqual(parseDevToolsLayout(JSON.stringify({ dock: "bottom", width: 500.4, height: 340.6 })), {
    dock: "bottom",
    width: 500,
    height: 341,
  });
});
