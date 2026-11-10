import { test } from "node:test";
import assert from "node:assert/strict";
import { SECRET_TAPS, tapCounter } from "./secret.ts";

test("the gesture completes on the seventh quick click, then starts over", () => {
  const c = tapCounter();
  const results = Array.from({ length: SECRET_TAPS }, (_, i) => c.tap(i * 200));
  assert.deepEqual(results, [false, false, false, false, false, false, true]);
  assert.equal(c.tap(1_500), false, "counting starts again");
});

test("slow clicks never complete it", () => {
  const c = tapCounter(3, 1000);
  assert.equal(c.tap(0), false);
  assert.equal(c.tap(600), false);
  assert.equal(c.tap(1_200), false, "the first click fell out of the window");
  assert.equal(c.tap(1_500), true);
});
