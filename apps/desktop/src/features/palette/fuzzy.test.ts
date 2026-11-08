import { test } from "node:test";
import assert from "node:assert/strict";
import { score, scoreAny } from "./fuzzy.ts";

test("an empty query matches everything", () => {
  assert.equal(score("", "anything"), 1);
});

test("no match scores zero", () => {
  assert.equal(score("xyz", "GitHub"), 0);
});

test("substring matches beat subsequence matches", () => {
  assert.ok(score("git", "GitHub") > score("gtb", "GitHub"));
  assert.ok(score("gtb", "GitHub") > 0);
});

test("matches at word starts rank higher", () => {
  assert.ok(score("hub", "git hub") > score("hub", "githubx"));
});

test("scoreAny takes the best field", () => {
  assert.equal(
    scoreAny("example", "Title", "https://example.com/"),
    score("example", "https://example.com/"),
  );
  assert.equal(scoreAny("zzz", "a", undefined, "b"), 0);
});
