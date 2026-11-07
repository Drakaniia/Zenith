import { test } from "node:test";
import assert from "node:assert/strict";
import { POPULAR_SITES, autofill, canAutofill, popularCandidates } from "../src/lib/autocomplete.ts";

const visit = (url: string, weight = 10) => ({ url, weight });

test("completes a typed prefix to a site from history", () => {
  const r = autofill("op", [visit("https://opencode.ai/docs/intro"), visit("https://example.com/")]);
  assert.deepEqual(r, { text: "opencode.ai", url: "https://opencode.ai/" });
});

test("keeps the typed characters' case and only adds the remainder", () => {
  const r = autofill("OpEn", [visit("https://open.spotify.com/playlist/1")]);
  assert.equal(r?.text, "OpEn.spotify.com");
  assert.equal(r?.url, "https://open.spotify.com/");
});

test("a site visited through many pages beats one visited once", () => {
  const r = autofill("op", [
    visit("https://openai.com/"),
    visit("https://opencode.ai/a"),
    visit("https://opencode.ai/b"),
  ]);
  assert.equal(r?.text, "opencode.ai");
});

test("own browsing beats bundled popular sites", () => {
  const r = autofill("op", [visit("https://open.spotify.com/", 1), ...popularCandidates()]);
  assert.equal(r?.text, "open.spotify.com");
});

test("popular sites complete on a fresh profile", () => {
  assert.equal(autofill("yout", popularCandidates())?.text, "youtube.com");
  assert.equal(autofill("opencod", popularCandidates())?.url, "https://opencode.ai/");
  assert.ok(POPULAR_SITES.includes("open.spotify.com"));
});

test("www. and a scheme can be typed", () => {
  const cands = [visit("https://www.github.com/"), visit("https://docs.rs/")];
  assert.equal(autofill("git", cands)?.text, "github.com");
  assert.equal(autofill("www.gi", cands)?.text, "www.github.com");
  assert.equal(autofill("www.do", cands)?.text, "www.docs.rs");
  assert.equal(autofill("https://do", cands)?.text, "https://docs.rs");
});

test("keeps the port and the original scheme", () => {
  const r = autofill("local", [visit("http://localhost:5173/app")]);
  assert.deepEqual(r, { text: "localhost:5173", url: "http://localhost:5173/" });
});

test("past the host, whole URLs are completed", () => {
  const r = autofill("github.com/sv", [visit("https://github.com/sveltejs/svelte"), visit("https://github.com/")]);
  assert.deepEqual(r, { text: "github.com/sveltejs/svelte", url: "https://github.com/sveltejs/svelte" });
});

test("no completion for searches, exact matches, non-web or unmatched input", () => {
  const cands = [visit("https://opencode.ai/"), visit("zenith://settings"), visit("file:///C:/x.txt")];
  assert.equal(autofill("op code", cands), null);
  assert.equal(autofill("?op", cands), null);
  assert.equal(autofill("opencode.ai", cands), null);
  assert.equal(autofill("sett", cands), null);
  assert.equal(autofill("zzz", cands), null);
  assert.equal(autofill("", cands), null);
  assert.equal(canAutofill("C:\\Users"), false);
  assert.equal(canAutofill(">tab"), false);
});
