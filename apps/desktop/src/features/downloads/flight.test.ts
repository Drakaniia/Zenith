import { test } from "node:test";
import assert from "node:assert/strict";
import { arcControl, boxAt, centre, CHIP, fileBadge, flightFrames, flightMs, LIFT, quad } from "./flight.ts";

const page = boxAt({ x: 900, y: 300 });
const button = { x: 12, y: 760, w: 32, h: 32 };

test("the arc's control point is above both ends, nearer the start", () => {
  const a = { x: 900, y: 300 };
  const b = { x: 28, y: 776 };
  const c = arcControl(a, b);
  assert.ok(c.y < a.y && c.y < b.y);
  assert.ok(Math.abs(c.x - a.x) < Math.abs(c.x - b.x));
});

test("the arc stays inside the window near its top edge", () => {
  const c = arcControl({ x: 500, y: 20 }, { x: 20, y: 30 });
  assert.equal(c.y, 12);
});

test("the Bézier passes through its ends", () => {
  const a = { x: 1, y: 2 };
  const b = { x: 9, y: 4 };
  const c = { x: 5, y: -10 };
  assert.deepEqual(quad(a, c, b, 0), a);
  assert.deepEqual(quad(a, c, b, 1), b);
});

test("a flight lifts off where the file was and lands on the button", () => {
  const f = flightFrames(page, button);
  const first = f[0];
  const last = f[f.length - 1];
  assert.deepEqual({ x: first.x, y: first.y }, centre(page));
  assert.equal(first.scale, 1);
  assert.equal(first.opacity, 0);
  const end = centre(button);
  assert.ok(Math.abs(last.x - end.x) < 1e-9 && Math.abs(last.y - end.y) < 1e-9);
  assert.equal(last.opacity, 0);
  assert.ok(last.scale * CHIP.h <= button.h);
});

test("the file rises and grows while it lifts, then curves above the straight line", () => {
  const f = flightFrames(page, button);
  const lifted = f.find((k) => Math.abs(k.offset - LIFT) < 1e-9)!;
  assert.ok(lifted.y < centre(page).y);
  assert.ok(lifted.scale > 1);
  const mid = f.find((k) => k.offset >= 0.6)!;
  const a = centre(page);
  const b = centre(button);
  const lineY = a.y + ((mid.x - a.x) / (b.x - a.x)) * (b.y - a.y);
  assert.ok(mid.y < lineY);
});

test("frame offsets run from 0 to 1 in order", () => {
  const f = flightFrames(page, button);
  assert.equal(f[0].offset, 0);
  assert.equal(f[f.length - 1].offset, 1);
  for (let i = 1; i < f.length; i++) assert.ok(f[i].offset > f[i - 1].offset);
});

test("a file starting from a thumbnail starts at the thumbnail's size", () => {
  const thumb = { x: 100, y: 100, w: 34, h: 34 };
  const f = flightFrames(thumb, button);
  assert.ok(Math.abs(f[0].scale * CHIP.h - 34) < 1e-9);
});

test("flight time grows with distance within bounds", () => {
  const near = flightMs(boxAt({ x: 40, y: 740 }), button);
  const far = flightMs(boxAt({ x: 3000, y: 0 }), button);
  assert.ok(near >= 560 && near < far && far <= 860);
});

test("badges show the file's kind and extension", () => {
  assert.deepEqual(fileBadge("report.pdf"), { icon: "fileText", label: "PDF", color: "#e5484d" });
  assert.equal(fileBadge("clip", "video/mp4").icon, "film");
  assert.equal(fileBadge("clip", "video/mp4").label, "");
  assert.equal(fileBadge("data.jsonl").label, "JSON");
});
