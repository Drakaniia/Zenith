import { test } from "node:test";
import assert from "node:assert/strict";
import { RateMeter, formatTimeLeft, panelStatus } from "./rate.ts";
import type { DownloadItem } from "./types.ts";

const MB = 1024 * 1024;
const ago = () => "just now";

function dl(over: Partial<DownloadItem> = {}): DownloadItem {
  return { id: 1, url: "https://a.test/f.zip", path: "C:\\d\\f.zip", mime: "", total: 100 * MB, received: 0, state: "progress", started: 0, ...over };
}

test("the speed is measured from successive reports and smoothed", () => {
  const m = new RateMeter();
  assert.equal(m.sample(1, 0, 0), 0);
  assert.equal(m.sample(1, 2 * MB, 1000), 2 * MB);
  // Too soon to measure: the last speed stays.
  assert.equal(m.sample(1, 2.1 * MB, 1100), 2 * MB);
  const r = m.sample(1, 6 * MB, 2000);
  assert.ok(r > 2 * MB && r < 4.5 * MB, `smoothed, got ${r}`);
});

test("a download that starts over resets its speed", () => {
  const m = new RateMeter();
  m.sample(1, 0, 0);
  m.sample(1, 5 * MB, 1000);
  assert.equal(m.sample(1, 0, 2000), 0);
  m.forget(1);
  assert.equal(m.sample(1, 1 * MB, 3000), 0);
});

test("time left reads like other browsers", () => {
  assert.equal(formatTimeLeft(8.2), "8 s left");
  assert.equal(formatTimeLeft(0.3), "1 s left");
  assert.equal(formatTimeLeft(185), "3 min left");
  assert.equal(formatTimeLeft(3900), "1 h 5 min left");
  assert.equal(formatTimeLeft(7200), "2 h left");
  assert.equal(formatTimeLeft(0), "");
  assert.equal(formatTimeLeft(Infinity), "");
});

test("a running file shows its speed and time left", () => {
  assert.equal(panelStatus(dl({ received: 50 * MB }), 5 * MB, ago), "50.0 MB of 100 MB · 5.0 MB/s · 10 s left");
  assert.equal(panelStatus(dl({ received: 0 }), 0, ago), "Starting…");
  assert.equal(panelStatus(dl({ total: 0, received: 3 * MB }), 1 * MB, ago), "3.0 MB · 1.0 MB/s");
  assert.equal(panelStatus(dl({ received: 10 * MB, paused: true }), 5 * MB, ago), "Paused · 10.0 MB of 100 MB");
});

test("finished and failed files say so", () => {
  assert.equal(panelStatus(dl({ state: "completed", received: 100 * MB, ended: 1 }), 0, ago), "Completed · 100 MB · just now");
  assert.equal(panelStatus(dl({ state: "interrupted", reason: "Network error" }), 0, ago), "Network error");
  assert.equal(panelStatus(dl({ state: "interrupted" }), 0, ago), "Failed");
  assert.equal(panelStatus(dl({ state: "canceled" }), 0, ago), "Canceled");
});
