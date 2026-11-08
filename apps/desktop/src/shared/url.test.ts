import { test } from "node:test";
import assert from "node:assert/strict";
import {
  NEW_TAB_URL,
  displayHost,
  internalPage,
  isNewTab,
  normalizeInput,
  prettyUrl,
  searchUrl,
  siteOf,
  titleFor,
} from "./url.ts";

test("addresses typed with a scheme are kept", () => {
  assert.equal(
    normalizeInput("https://example.com/a?b=1", "google"),
    "https://example.com/a?b=1",
  );
  assert.equal(
    normalizeInput("http://localhost:3000", "google"),
    "http://localhost:3000/",
  );
  assert.equal(normalizeInput("about:blank", "google"), "about:blank");
});

test("bare hosts get https; localhost and IP addresses get http", () => {
  assert.equal(normalizeInput("example.com", "google"), "https://example.com/");
  assert.equal(
    normalizeInput("docs.rs/serde", "google"),
    "https://docs.rs/serde",
  );
  assert.equal(
    normalizeInput("localhost:5173", "google"),
    "http://localhost:5173/",
  );
  assert.equal(normalizeInput("192.168.1.1", "google"), "http://192.168.1.1/");
});

test("everything else becomes a search on the chosen engine", () => {
  assert.equal(
    normalizeInput("how to tie a tie", "google"),
    "https://www.google.com/search?q=how%20to%20tie%20a%20tie",
  );
  assert.equal(
    normalizeInput("rust", "yandex"),
    "https://yandex.com/search/?text=rust",
  );
  assert.equal(
    normalizeInput("notes.txt", "google"),
    searchUrl("notes.txt", "google"),
  );
  assert.equal(
    normalizeInput("?example.com", "google"),
    searchUrl("example.com", "google"),
  );
});

test("script URLs are never navigated to", () => {
  assert.equal(
    normalizeInput("javascript:alert(1)", "google"),
    searchUrl("javascript:alert(1)", "google"),
  );
  assert.equal(
    normalizeInput("vbscript:msgbox", "google"),
    searchUrl("vbscript:msgbox", "google"),
  );
});

test("internal pages and local paths", () => {
  assert.equal(
    normalizeInput("zenith://settings", "google"),
    "zenith://settings",
  );
  assert.equal(normalizeInput("zenith://nope", "google"), NEW_TAB_URL);
  assert.equal(
    normalizeInput("C:\\Users\\me\\page.html", "google"),
    "file:///C:/Users/me/page.html",
  );
  assert.equal(internalPage("zenith://settings/privacy"), "settings");
  assert.equal(internalPage("https://example.com/"), null);
  assert.ok(isNewTab("about:blank"));
  assert.ok(isNewTab(NEW_TAB_URL));
});

test("display helpers", () => {
  assert.equal(displayHost("https://www.example.com/a"), "example.com");
  assert.equal(prettyUrl("https://www.example.com/"), "example.com");
  assert.equal(titleFor("zenith://history", ""), "History");
  assert.equal(titleFor("https://example.com/", "Example"), "Example");
  assert.equal(siteOf("a.b.example.co.uk"), "example.co.uk");
  assert.equal(siteOf("www.google.com"), "google.com");
  assert.equal(siteOf("localhost"), "localhost");
});
