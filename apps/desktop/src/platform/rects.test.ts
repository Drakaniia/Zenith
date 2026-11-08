import { test } from "node:test";
import assert from "node:assert/strict";
import { intersectRects, sameRects } from "./rects.ts";

const page = { x: 260, y: 8, w: 1000, h: 700 };

test("a page that stays put keeps its whole cutout", () => {
  assert.deepEqual(intersectRects([page], [page]), [page]);
  assert.ok(sameRects(intersectRects([page], [page]), [page]));
});

test("a page that shrinks for a sidebar keeps only where it is before and after", () => {
  const narrower = { x: 620, y: 8, w: 640, h: 700 };
  assert.deepEqual(intersectRects([page], [narrower]), [narrower]);
  // Growing back: the part it covers already is all that stays cut out until it has grown.
  assert.deepEqual(intersectRects([narrower], [page]), [narrower]);
});

test("pages that go away or do not overlap leave no cutout", () => {
  assert.deepEqual(intersectRects([page], []), []);
  assert.deepEqual(intersectRects([], [page]), []);
  assert.deepEqual(intersectRects([page], [{ x: 0, y: 0, w: 260, h: 8 }]), []);
});

test("split panes intersect pairwise", () => {
  const left = { x: 0, y: 0, w: 100, h: 100 };
  const right = { x: 110, y: 0, w: 100, h: 100 };
  const moved = [
    { x: 50, y: 0, w: 70, h: 100 },
    { x: 130, y: 0, w: 80, h: 100 },
  ];
  assert.deepEqual(intersectRects([left, right], moved), [
    { x: 50, y: 0, w: 50, h: 100 },
    { x: 110, y: 0, w: 10, h: 100 },
    { x: 130, y: 0, w: 80, h: 100 },
  ]);
});

test("sameRects compares values and order", () => {
  assert.ok(sameRects([{ ...page }], [page]));
  assert.ok(!sameRects([page], [{ ...page, w: 999 }]));
  assert.ok(!sameRects([page], []));
});
