import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import { intersectRects, sameRects } from "./rects.ts";

function harness() {
  // Exercise the actual bridge with deterministic frames, DOM and backend acknowledgements.
  const source = readFileSync(new URL("./layout.svelte.ts", import.meta.url), "utf8");
  const js = ts.transpileModule(source.replace(/^import .*;\r?\n/gm, ""), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const bounds = { x: 260, right: 1260 };
  let measurements = 0;
  const el = {
    isConnected: true,
    getBoundingClientRect: () => {
      measurements++;
      return { left: bounds.x, top: 8, right: bounds.right, bottom: 708, width: bounds.right - bounds.x, height: 700 };
    },
  };
  const frames: FrameRequestCallback[] = [];
  type Spec = { panes: { id: string; x: number; w: number }[]; overlay: boolean };
  const sent: Spec[] = [];
  let raised = false;
  let resized = () => {};
  let time = 0;
  let send = async (_spec: Spec) => {};
  const root = {
    hasAttribute: () => raised,
    setAttribute: () => { raised = true; },
    removeAttribute: () => { raised = false; },
  };
  const bridge = { exports: {} as Record<string, any> };
  new Function(
    "exports", "$state", "ResizeObserver", "window", "document", "requestAnimationFrame",
    "webviewApi", "intersectRects", "sameRects", "performance", js,
  )(
    bridge.exports, (v: unknown) => v,
    class { constructor(cb: () => void) { resized = cb; } observe() {} unobserve() {} },
    { innerWidth: 1280, innerHeight: 720, addEventListener() {} },
    { hidden: false, documentElement: root },
    (cb: FrameRequestCallback) => frames.push(cb),
    { setLayout: async (spec: Spec) => { sent.push(spec); await send(spec); }, focusView: async () => {} },
    intersectRects, sameRects, { now: () => time },
  );
  const api = bridge.exports;
  api.setLayoutRegistry({ isLive: () => true, isReady: () => true });
  const settle = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
  const frame = async () => {
    time += 16;
    const callbacks = frames.splice(0);
    for (const cb of callbacks) cb(time);
    await settle();
  };
  api.pane(el, "page");
  return {
    api, bounds, sent, frames, frame, settle,
    resize: () => resized(),
    measurements: () => measurements,
    raised: () => raised,
    setSend: (fn: typeof send) => { send = fn; },
  };
}

test("live pages follow each sidebar frame without a paint barrier below the pages", async () => {
  const h = harness();
  await h.frame();
  assert.equal(h.sent.length, 1);
  // Closing then reversing mid-flight must send intermediate geometry, not an endpoint hold.
  for (const x of [210, 140, 70, 8, 70, 140, 210, 260]) {
    h.bounds.x = x;
    h.api.scheduleLayout();
    await h.frame();
    assert.equal(h.sent.at(-1)?.panes[0].x, x);
    assert.equal(h.sent.at(-1)?.panes[0].w, h.bounds.right - x);
    assert.equal(h.frames.length, 0, "No extra paint frame while chrome is below the pages");
  }
  h.bounds.x = 8;
  for (const right of [1200, 1120, 1020, 1120, 1260]) {
    h.bounds.right = right;
    h.api.scheduleLayout();
    await h.frame();
    assert.equal(h.sent.at(-1)?.panes[0].w, right - h.bounds.x);
  }
  h.api.raiseChrome();
  await h.frame();
  assert.equal(h.raised(), true);
  assert.equal(h.sent.at(-1)?.overlay, false);
  await h.frame();
  await new Promise((resolve) => setTimeout(resolve, 5));
  await h.settle();
  assert.equal(h.sent.at(-1)?.overlay, true);
});

test("animation following sends bounds in its own frame, not the following frame", async () => {
  const h = harness();
  await h.frame();
  h.api.followLayout(100);
  h.bounds.x = 210;
  await h.frame();
  assert.equal(h.sent.at(-1)?.panes[0].x, 210);
});

test("ResizeObserver sends the new bounds without waiting for another rendering frame", async () => {
  const h = harness();
  await h.frame();
  h.bounds.x = 180;
  h.resize();
  await h.settle();
  assert.equal(h.sent.at(-1)?.panes[0].x, 180);
  assert.equal(h.frames.length, 0);
});

test("a slow backend coalesces resize requests and immediately applies the latest bounds", async () => {
  const h = harness();
  let release!: () => void;
  h.setSend(() => new Promise<void>((resolve) => { release = resolve; }));
  await h.frame();
  assert.equal(h.sent.length, 1);
  h.setSend(async () => {});
  for (const x of [230, 200, 170, 140, 110]) {
    h.bounds.x = x;
    h.api.scheduleLayout();
    h.resize();
    await h.frame();
  }
  assert.equal(h.sent.length, 1, "Only one IPC call may be in flight");
  release();
  await h.settle();
  assert.equal(h.sent.length, 2);
  assert.equal(h.sent.at(-1)?.panes[0].x, 110);
  assert.equal(h.measurements(), 2, "Do not replay a queued flush for every missed frame");
});
