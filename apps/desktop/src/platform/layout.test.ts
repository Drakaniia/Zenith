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
  class Element {
    private attr: string;
    constructor(attr: string) { this.attr = attr; }
    hasAttribute(name: string) { return name === this.attr; }
  }
  class Transition {
    currentTime: number | null = 20;
    playState = "running";
    pauses = 0;
    plays = 0;
    effect: { target: Element; getComputedTiming(): { endTime: number } };
    transitionProperty: string;
    constructor(transitionProperty: string, attr: string) {
      this.transitionProperty = transitionProperty;
      this.effect = { target: new Element(attr), getComputedTiming: () => ({ endTime: 100 }) };
    }
    pause() { this.pauses++; this.playState = "paused"; }
    play() { this.plays++; this.playState = "running"; }
  }
  const animations: Transition[] = [];
  let measurements = 0;
  const el = {
    isConnected: true,
    getBoundingClientRect: () => {
      measurements++;
      return { left: bounds.x, top: 8, right: bounds.right, bottom: 708, width: bounds.right - bounds.x, height: 700 };
    },
  };
  const frames: FrameRequestCallback[] = [];
  type Spec = { panes: { id: string; x: number; w: number; cw?: number; ch?: number }[]; overlay: boolean };
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
    "webviewApi", "intersectRects", "sameRects", "performance", "CSSTransition", "HTMLElement", js,
  )(
    bridge.exports, (v: unknown) => v,
    class { constructor(cb: () => void) { resized = cb; } observe() {} unobserve() {} },
    { innerWidth: 1280, innerHeight: 720, addEventListener() {} },
    { hidden: false, documentElement: root, getAnimations: () => animations },
    (cb: FrameRequestCallback) => frames.push(cb),
    { setLayout: async (spec: Spec) => { sent.push(spec); await send(spec); }, focusView: async () => {} },
    intersectRects, sameRects, { now: () => time }, Transition, Element,
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
    api, bounds, sent, frames, frame, settle, animations, Transition,
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

test("layout clocks wait for native acknowledgement without pausing unrelated motion", async () => {
  const h = harness();
  await h.frame();
  const edge = new h.Transition("--lay-left", "data-layout-root");
  const width = new h.Transition("width", "data-layout-sidebar");
  const hover = new h.Transition("opacity", "data-layout-root");
  const paused = new h.Transition("--lay-top", "data-layout-root");
  paused.playState = "paused";
  h.animations.push(edge, width, hover, paused);
  let release!: () => void;
  h.setSend(() => new Promise<void>((resolve) => { release = resolve; }));
  h.bounds.x = 210;
  h.resize();
  await h.settle();
  assert.equal(edge.playState, "paused");
  assert.equal(width.playState, "paused");
  assert.equal(hover.pauses, 0);
  release();
  await h.settle();
  assert.equal(edge.playState, "running");
  assert.equal(width.playState, "running");
  assert.equal(paused.plays, 0, "Do not resume clocks paused by another owner");
});

test("paint fences remeasure geometry before native placement and cutout publication", async () => {
  const h = harness();
  await h.frame();
  h.api.raiseChrome();
  await h.frame();
  h.bounds.x = 180;
  await h.frame();
  await new Promise((resolve) => setTimeout(resolve, 5));
  await h.settle();
  assert.equal(h.sent.at(-1)?.panes[0].x, 180);
  assert.equal(h.api.overlay.panes[0].x, 180);
});

test("retargets keep the high-water content allocation until gated motion settles", async () => {
  const h = harness();
  h.bounds.x = 60;
  await h.frame();
  const edge = new h.Transition("--lay-left", "data-layout-root");
  const unrelated = new h.Transition("opacity", "data-layout-root");
  h.animations.push(edge, unrelated);
  let finished = false;
  void h.api.followLayout(50).then(() => { finished = true; });
  h.bounds.x = 160;
  await h.frame();
  assert.equal(h.sent.at(-1)?.panes[0].cw, 1200);
  h.api.followLayout(50);
  h.bounds.x = 260;
  await h.frame();
  assert.equal(h.sent.at(-1)?.panes[0].cw, 1200);
  for (let i = 0; i < 8; i++) await h.frame();
  assert.equal(finished, false, "Nominal timeout must not cancel a still-running gated transition");
  assert.equal(unrelated.currentTime, 20, "Endpoint measurement must not seek unrelated transitions");
  edge.playState = "finished";
  await h.frame();
  assert.equal(finished, true);
  assert.equal(h.sent.at(-1)?.panes[0].cw, undefined);
  assert.equal(h.sent.at(-1)?.panes[0].w, 1000);
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
