import { test } from "node:test";
import assert from "node:assert/strict";
import { PDFDocument } from "pdf-lib";
import { applyPageOp, cornerFor, placeStamp } from "./pages.ts";

async function make(sizes: [number, number][]): Promise<Uint8Array> {
  const doc = await PDFDocument.create();
  for (const s of sizes) doc.addPage(s);
  return doc.save();
}

const sizes = async (bytes: Uint8Array) =>
  (await PDFDocument.load(bytes)).getPages().map((p) => [p.getWidth(), p.getHeight(), p.getRotation().angle]);

// 1×1 transparent PNG.
const PNG = Uint8Array.from(
  atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="),
  (c) => c.charCodeAt(0),
);

test("page operations", async () => {
  const base = await make([[100, 200], [300, 400]]);
  assert.deepEqual(await sizes(await applyPageOp(base, { kind: "rotate", index: 1, by: -90 })), [[100, 200, 0], [300, 400, 270]]);
  assert.deepEqual(await sizes(await applyPageOp(base, { kind: "delete", index: 0 })), [[300, 400, 0]]);
  assert.deepEqual(await sizes(await applyPageOp(base, { kind: "move", from: 1, to: 0 })), [[300, 400, 0], [100, 200, 0]]);
  assert.deepEqual(await sizes(await applyPageOp(base, { kind: "blank", after: 0 })), [[100, 200, 0], [100, 200, 0], [300, 400, 0]]);
  const merged = await applyPageOp(base, { kind: "merge", bytes: await make([[50, 60]]) });
  assert.deepEqual((await sizes(merged)).length, 3);
});

test("page operations refuse impossible edits", async () => {
  const one = await make([[100, 100]]);
  await assert.rejects(applyPageOp(one, { kind: "delete", index: 0 }));
  await assert.rejects(applyPageOp(one, { kind: "rotate", index: 3, by: 90 }));
});

test("signature corner keeps the image centred", () => {
  assert.deepEqual(cornerFor(100, 100, 40, 20, 0), { x: 80, y: 90 });
  const r = cornerFor(100, 100, 40, 20, 90);
  assert.ok(Math.abs(r.x - 110) < 1e-9 && Math.abs(r.y - 80) < 1e-9);
});

test("placing a signature keeps the page count", async () => {
  const out = await placeStamp(await make([[200, 200]]), { png: PNG, page: 0, x: 100, y: 100, width: 50, height: 20 });
  assert.equal((await sizes(out)).length, 1);
});
