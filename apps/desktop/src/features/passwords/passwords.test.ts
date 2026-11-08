import { test } from "node:test";
import assert from "node:assert/strict";
import {
  filterEntries,
  formatRecoveryKey,
  groupBySite,
  masterProblem,
  needsMaster,
  placeSuggestions,
  strength,
  type PasswordEntry,
} from "./passwords.ts";

const entry = (site: string, username: string, label = ""): PasswordEntry => ({
  id: `${site}/${username}`,
  origin: `https://${site}`,
  site,
  username,
  label,
  created: 0,
  updated: 0,
  lastUsed: 0,
  timesUsed: 0,
  hasNotes: false,
});

test("groups several accounts under one site, sorted", () => {
  const groups = groupBySite([entry("github.com", "zed"), entry("facebook.com", "ann"), entry("GitHub.com", "amy")]);
  assert.deepEqual(
    groups.map((g) => [g.site, g.entries.map((e) => e.username)]),
    [
      ["facebook.com", ["ann"]],
      ["github.com", ["amy", "zed"]],
    ],
  );
});

test("search matches every word across site, username and label", () => {
  const list = [entry("mail.google.com", "ann@x.com", "Work"), entry("google.com", "bob"), entry("bank.com", "ann@x.com")];
  assert.equal(filterEntries(list, "").length, 3);
  assert.deepEqual(
    filterEntries(list, "google ann").map((e) => e.site),
    ["mail.google.com"],
  );
  assert.deepEqual(
    filterEntries(list, "WORK").map((e) => e.site),
    ["mail.google.com"],
  );
});

test("strength rates length and variety, and penalises common passwords", () => {
  assert.equal(strength("").score, 0);
  assert.ok(strength("password123").score <= 1);
  assert.ok(strength("aaaaaaaaaaaaaaaa").score <= 1);
  assert.ok(strength("12345678").score <= 1);
  assert.ok(strength("correct horse battery staple").score >= 3);
  assert.ok(strength("Xq7$mP2!vR9#").score >= 3);
});

test("master password rules", () => {
  assert.match(masterProblem("short", "short"), /at least 8/);
  assert.match(masterProblem("aaaaaaaaaa", "aaaaaaaaaa"), /harder to guess/);
  assert.match(masterProblem("correct horse battery", "correct horse"), /don't match/);
  assert.equal(masterProblem("correct horse battery", "correct horse battery"), "");
});

test("recognises errors that ask for the master password", () => {
  assert.ok(needsMaster("Enter your master password to continue."));
  assert.ok(needsMaster("The password manager is locked."));
  assert.ok(!needsMaster("That master password isn't right."));
});

test("suggestions go under the field, scaled by page zoom, inside the page", () => {
  const pane = { x: 200, y: 50, w: 800, h: 600 };
  const p = placeSuggestions({ x: 100, y: 100, w: 200, h: 30 }, 1.5, 1, pane, { w: 300, h: 150 });
  // 150% zoom: the field is at 150,150 in the pane and 45px tall.
  assert.deepEqual(p, { x: 350, y: 50 + 150 + 45 + 6, w: 300, above: false });
  // Near the bottom: above the field.
  const low = placeSuggestions({ x: 0, y: 560, w: 100, h: 30 }, 1, 1, pane, { w: 300, h: 150 });
  assert.equal(low.above, true);
  assert.equal(low.y, 50 + 560 - 150 - 6);
  assert.equal(low.x, 208, "kept inside the page");
  // A high-DPI window at 100% page zoom is not scaled.
  const hidpi = placeSuggestions({ x: 10, y: 10, w: 400, h: 20 }, 2, 2, pane, { w: 300, h: 150 });
  assert.equal(hidpi.w, 400);
});

test("recovery keys are shown in groups of four", () => {
  assert.equal(formatRecoveryKey("abcd efgh-ijkl"), "ABCD-EFGH-IJKL");
  assert.equal(formatRecoveryKey("ab"), "AB");
  assert.equal(formatRecoveryKey(""), "");
});
