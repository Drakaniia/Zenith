import { test } from "node:test";
import assert from "node:assert/strict";
import {
  STEPS,
  STEP_FIELDS,
  altitude,
  arcPoint,
  clampStep,
  normalizeHomepage,
  plural,
  shouldOnboard,
  stepIndex,
} from "../src/lib/onboarding-flow.ts";

test("setup opens only for a normal window that has not finished it", () => {
  assert.equal(shouldOnboard({ onboarded: false }, false), true);
  assert.equal(shouldOnboard({}, false), true);
  assert.equal(shouldOnboard({ onboarded: true }, false), false);
  assert.equal(shouldOnboard({ onboarded: false }, true), false);
});

test("the flow starts with welcome and ends with ready; only middle steps are optional", () => {
  assert.equal(STEPS[0].id, "welcome");
  assert.equal(STEPS.at(-1)!.id, "ready");
  assert.equal(STEPS[0].optional, false);
  assert.equal(STEPS.at(-1)!.optional, false);
  assert.equal(new Set(STEPS.map((s) => s.id)).size, STEPS.length);
  for (const s of STEPS) assert.ok(STEP_FIELDS[s.id], `fields declared for ${s.id}`);
  assert.equal(stepIndex("search"), 2);
  assert.equal(stepIndex("nope"), -1);
  assert.equal(stepIndex(undefined), -1);
  assert.equal(clampStep(-3), 0);
  assert.equal(clampStep(99), STEPS.length - 1);
});

test("the sun rises from the horizon to the zenith", () => {
  assert.equal(altitude(0), 0);
  assert.equal(altitude(STEPS.length - 1), 90);
  assert.ok(altitude(2) > altitude(1));
  const rise = arcPoint(0);
  assert.ok(Math.abs(rise.x) < 1e-9 && Math.abs(rise.y - 1) < 1e-9);
  const top = arcPoint(90);
  assert.ok(Math.abs(top.x - 0.5) < 1e-9 && Math.abs(top.y) < 1e-9);
});

test("home pages are validated and get https", () => {
  assert.deepEqual(normalizeHomepage(""), { ok: true, url: "" });
  assert.deepEqual(normalizeHomepage("  example.com "), { ok: true, url: "https://example.com/" });
  assert.deepEqual(normalizeHomepage("http://intranet.corp/x"), { ok: true, url: "http://intranet.corp/x" });
  assert.deepEqual(normalizeHomepage("localhost:3000"), { ok: true, url: "https://localhost:3000/" });
  for (const bad of ["javascript:alert(1)", "file:///C:/x", "not a site", "ftp://a.example", "word"]) {
    assert.equal(normalizeHomepage(bad).ok, false, bad);
  }
});

test("counts read naturally", () => {
  assert.equal(plural(1, "bookmark"), "1 bookmark");
  assert.equal(plural(1204, "bookmark"), "1,204 bookmarks");
  assert.equal(plural(2, "entry", "entries"), "2 entries");
});
