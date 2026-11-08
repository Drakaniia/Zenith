import { test } from "node:test";
import assert from "node:assert/strict";
import { COMPACT_MS, LAYOUT_MS, STILL, TOOLBAR_FOLD_MS, followMs, layoutMotion, type LayoutState } from "./layout-motion.ts";

const base: LayoutState = { compact: false, mode: "multiple", barOn: false, hideToolbar: false, side: "left" };
const with_ = (p: Partial<LayoutState>): LayoutState => ({ ...base, ...p });

test("nothing changed: no motion", () => {
  assert.deepEqual(layoutMotion(base, with_({}), false), STILL);
});

test("compact mode toggles on Zen's 100 ms spring, both ways", () => {
  const on = layoutMotion(base, with_({ compact: true }), false);
  assert.deepEqual(on, { side: COMPACT_MS, bar: COMPACT_MS, fold: false });
  const off = layoutMotion(with_({ compact: true }), base, false);
  assert.deepEqual(off, { side: COMPACT_MS, bar: COMPACT_MS, fold: false });
});

test("a toolbar compact mode hides folds over 150 ms", () => {
  const on = layoutMotion(base, with_({ compact: true, hideToolbar: true }), false);
  assert.deepEqual(on, { side: COMPACT_MS, bar: TOOLBAR_FOLD_MS, fold: true });
  // Ctrl+Alt+T inside compact mode: only the toolbar moves.
  const t = layoutMotion(with_({ compact: true }), with_({ compact: true, hideToolbar: true }), false);
  assert.deepEqual(t, { side: 0, bar: TOOLBAR_FOLD_MS, fold: true });
});

test("leaving compact mode from the floating sidebar docks it without animating", () => {
  assert.deepEqual(layoutMotion(with_({ compact: true }), base, true), STILL);
  // Entering is unaffected (the floating sidebar only exists in compact mode anyway).
  assert.equal(layoutMotion(base, with_({ compact: true }), true).side, COMPACT_MS);
});

test("other layout changes keep the 300 ms motion", () => {
  const m = layoutMotion(base, with_({ mode: "collapsed" }), false);
  assert.deepEqual(m, { side: LAYOUT_MS, bar: LAYOUT_MS, fold: false });
  // Collapsing the sidebar out of compact mode (one step) is a layout change too.
  assert.equal(layoutMotion(with_({ compact: true }), with_({ mode: "collapsed" }), true).side, LAYOUT_MS);
});

test("followMs covers the longest move", () => {
  assert.equal(followMs(STILL), 0);
  assert.equal(followMs({ side: COMPACT_MS, bar: TOOLBAR_FOLD_MS, fold: true }), TOOLBAR_FOLD_MS + 80);
});
