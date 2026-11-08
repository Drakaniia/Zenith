import { test } from "node:test";
import assert from "node:assert/strict";
import {
  HOVER_HIDE_MS,
  MAX_CUSTOM_IMAGES,
  MAX_SAVED_SITES,
  averageLuminance,
  backgroundChoices,
  composeTiles,
  moveItem,
  newSiteId,
  siteKey,
  siteUrl,
  validateSite,
  dailyChoice,
  dayNumber,
  defaultStartPage,
  fitWithin,
  localDay,
  newWallpaperId,
  normalizeStartPage,
  parseBackground,
  stepChoice,
  textTone,
  toneOf,
} from "./startpage.ts";
import { GALLERY, GALLERY_IDS, galleryUrl } from "./wallpapers.ts";

test("hover-to-hide activates after one second", () => {
  assert.equal(HOVER_HIDE_MS, 1000);
});

test("missing or broken saved preferences fall back to the defaults", () => {
  assert.deepEqual(normalizeStartPage(undefined, GALLERY_IDS), defaultStartPage());
  assert.deepEqual(normalizeStartPage("nope", GALLERY_IDS), defaultStartPage());
  const p = normalizeStartPage(
    { showSearch: "yes", tileSize: "huge", tileCount: 7, align: "left", dim: 500, blur: -3, dailyDate: "today" },
    GALLERY_IDS,
  );
  const d = defaultStartPage();
  assert.equal(p.showSearch, d.showSearch);
  assert.equal(p.tileSize, d.tileSize);
  assert.equal(p.tileCount, d.tileCount);
  assert.equal(p.align, d.align);
  assert.equal(p.dim, 70);
  assert.equal(p.blur, 0);
  assert.equal(p.dailyDate, "");
});

test("saved choices are kept", () => {
  const p = normalizeStartPage(
    {
      showSearch: false,
      showTiles: false,
      tileSize: "large",
      tileCount: 16,
      align: "top",
      searchWidth: "compact",
      background: "gallery:tide",
      dailyBackground: true,
      dailyDate: "2026-11-07",
      dim: 35,
      blur: 6,
    },
    GALLERY_IDS,
  );
  assert.equal(p.showSearch, false);
  assert.equal(p.showTiles, false);
  assert.equal(p.tileSize, "large");
  assert.equal(p.tileCount, 16);
  assert.equal(p.align, "top");
  assert.equal(p.searchWidth, "compact");
  assert.equal(p.background, "gallery:tide");
  assert.equal(p.dailyBackground, true);
  assert.equal(p.dailyDate, "2026-11-07");
  assert.equal(p.dim, 35);
  assert.equal(p.blur, 6);
});

test("a background that no longer exists is dropped", () => {
  assert.equal(normalizeStartPage({ background: "gallery:gone" }, GALLERY_IDS).background, "");
  assert.equal(normalizeStartPage({ background: "custom:w-1" }, GALLERY_IDS).background, "");
  const p = normalizeStartPage({ background: "custom:w-1", customImages: [{ id: "w-1", tone: "light" }] }, GALLERY_IDS);
  assert.equal(p.background, "custom:w-1");
  assert.deepEqual(p.customImages, [{ id: "w-1", tone: "light" }]);
});

test("uploaded image entries are validated, deduplicated and capped", () => {
  const many = Array.from({ length: 20 }, (_, i) => ({ id: `w-${i}`, tone: "dark" }));
  const p = normalizeStartPage(
    { customImages: [{ id: "../evil" }, { id: "w-a", tone: "odd" }, { id: "w-a" }, null, ...many] },
    GALLERY_IDS,
  );
  assert.equal(p.customImages.length, MAX_CUSTOM_IMAGES);
  assert.deepEqual(p.customImages[0], { id: "w-a", tone: "dark" });
  assert.ok(p.customImages.every((c) => /^[a-z0-9-]+$/.test(c.id)));
});

test("saved sites and hidden sites are validated, deduplicated and capped", () => {
  const many = Array.from({ length: 40 }, (_, i) => ({ id: `s-${i}`, url: `https://site${i}.com/`, title: "" }));
  const p = normalizeStartPage(
    {
      sites: [
        { id: "s-a", url: "javascript:alert(1)", title: "x" },
        { id: "../x", url: "https://a.com" },
        { id: "s-b", url: "b.com", title: "  Bee   site " },
        { id: "s-b", url: "https://dup.com" },
        null,
        ...many,
      ],
      hiddenSites: ["x.com", "x.com", "<script>", 5, "y.org"],
      showSaved: false,
    },
    GALLERY_IDS,
  );
  assert.equal(p.sites.length, MAX_SAVED_SITES);
  assert.deepEqual(p.sites[0], { id: "s-b", url: "https://b.com/", title: "Bee site" });
  assert.deepEqual(p.hiddenSites, ["x.com", "y.org"]);
  assert.equal(p.showSaved, false);
  const d = normalizeStartPage({}, GALLERY_IDS);
  assert.deepEqual([d.sites, d.hiddenSites, d.showSaved], [[], [], true]);
});

test("typed site addresses become web URLs, anything else is refused", () => {
  assert.equal(siteUrl("example.com"), "https://example.com/");
  assert.equal(siteUrl(" http://Example.com/a?b=1 "), "http://example.com/a?b=1");
  assert.equal(siteUrl("localhost:3000"), "https://localhost:3000/");
  assert.equal(siteUrl("example.com:8080/x"), "https://example.com:8080/x");
  for (const bad of ["", "hello", "two words.com", "javascript:alert(1)", "file:///c:/x", "ftp://a.com", "a..com", "zenith://settings"])
    assert.equal(siteUrl(bad), null, bad);
  assert.equal(siteKey("https://www.Example.com/x"), "example.com");
  assert.equal(siteKey("nope"), "");
  assert.match(newSiteId(Date.now(), 0.5), /^[a-z0-9-]{1,40}$/);
});

test("the site form rejects bad addresses, duplicates and a full Tableau", () => {
  const sites = [{ id: "s-1", url: "https://a.com/", title: "" }];
  assert.deepEqual(validateSite({ url: "b.com", title: " B " }, sites), { url: "https://b.com/", title: "B" });
  assert.ok("error" in validateSite({ url: "not a url", title: "" }, sites));
  assert.ok("error" in validateSite({ url: "a.com", title: "" }, sites));
  assert.deepEqual(validateSite({ url: "a.com", title: "A" }, sites, "s-1"), { url: "https://a.com/", title: "A" }, "editing itself");
  const full = Array.from({ length: MAX_SAVED_SITES }, (_, i) => ({ id: `s-${i}`, url: `https://s${i}.com/`, title: "" }));
  assert.ok("error" in validateSite({ url: "new.com", title: "" }, full));
  assert.ok(!("error" in validateSite({ url: "new.com", title: "" }, full, "s-0")));
});

test("saved sites reorder", () => {
  assert.deepEqual(moveItem(["a", "b", "c"], 0, 2), ["b", "c", "a"]);
  assert.deepEqual(moveItem(["a", "b", "c"], 2, 0), ["c", "a", "b"]);
  assert.deepEqual(moveItem(["a", "b"], 5, 0), ["a", "b"]);
});

test("the Tableau shows saved sites first and fills the rest up to the limit", () => {
  const saved = [{ id: "s-1", url: "https://a.com/", title: "" }];
  const top = [{ url: "https://www.a.com/x" }, { url: "https://b.com/" }, { url: "https://c.com/" }, { url: "https://d.com/" }];
  const c = composeTiles(saved, ["e1"], top, ["c.com"], 4);
  assert.deepEqual(c.saved, saved);
  assert.deepEqual(c.essentials, ["e1"]);
  assert.deepEqual(c.top.map((t) => t.url), ["https://b.com/", "https://d.com/"], "saved and hidden sites are skipped");
  const many = Array.from({ length: 6 }, (_, i) => ({ id: `s-${i}`, url: `https://s${i}.com/`, title: "" }));
  const over = composeTiles(many, ["e1"], top, [], 4);
  assert.equal(over.saved.length, 6, "saved sites are never cut");
  assert.deepEqual([over.essentials, over.top], [[], []]);
});

test("background refs parse", () => {
  assert.deepEqual(parseBackground(""), { kind: "none" });
  assert.deepEqual(parseBackground("gallery:tide"), { kind: "gallery", id: "tide" });
  assert.deepEqual(parseBackground("custom:w-1"), { kind: "custom", id: "w-1" });
  assert.deepEqual(parseBackground("http://x"), { kind: "none" });
  assert.deepEqual(parseBackground("gallery:"), { kind: "none" });
});

test("browsing order is none, own images, gallery; previous and next wrap", () => {
  const choices = backgroundChoices(["a", "b"], [{ id: "w-1", tone: "dark" }]);
  assert.deepEqual(choices, ["", "custom:w-1", "gallery:a", "gallery:b"]);
  assert.equal(stepChoice(choices, "", 1), "custom:w-1");
  assert.equal(stepChoice(choices, "", -1), "gallery:b");
  assert.equal(stepChoice(choices, "gallery:b", 1), "");
  assert.equal(stepChoice(choices, "unknown", 1), "custom:w-1");
});

test("the daily background changes every day and never picks none", () => {
  const ids = ["a", "b", "c"];
  const days = ["2026-11-07", "2026-11-08", "2026-11-09", "2026-11-10"];
  const picks = days.map((d) => dailyChoice(ids, [], d));
  for (let i = 1; i < picks.length; i++) assert.notEqual(picks[i], picks[i - 1]);
  assert.ok(picks.every((p) => p.startsWith("gallery:")));
  assert.equal(dailyChoice([], [], days[0]), "");
  assert.equal(dayNumber("1970-01-02"), 1);
  assert.equal(localDay(new Date(2026, 0, 5)), "2026-01-05");
});

test("text over a background stays readable", () => {
  assert.equal(textTone("dark", 0), "light");
  assert.equal(textTone("light", 0), "dark");
  assert.equal(textTone("light", 50), "light", "darkened enough for light text");
  assert.equal(toneOf(averageLuminance([255, 255, 255, 255, 240, 240, 240, 255])), "light");
  assert.equal(toneOf(averageLuminance([10, 20, 30, 255])), "dark");
});

test("uploads are downscaled to fit, never enlarged", () => {
  assert.deepEqual(fitWithin(5120, 2880, 2560), [2560, 1440]);
  assert.deepEqual(fitWithin(1000, 4000, 2560), [640, 2560]);
  assert.deepEqual(fitWithin(800, 600, 2560), [800, 600]);
});

test("generated ids are accepted by the backend's id rule", () => {
  for (const r of [0, 0.5, 0.999999]) assert.match(newWallpaperId(Date.now(), r), /^[a-z0-9-]{1,40}$/);
});

test("every gallery picture renders to an SVG data URL with a unique id", () => {
  assert.equal(new Set(GALLERY_IDS).size, GALLERY.length);
  assert.ok(GALLERY.length >= 10);
  for (const w of GALLERY) {
    const url = galleryUrl(w.id)!;
    assert.ok(url.startsWith("data:image/svg+xml;charset=utf-8,"), w.id);
    const svg = decodeURIComponent(url.slice(url.indexOf(",") + 1));
    assert.ok(svg.startsWith("<svg") && svg.endsWith("</svg>"), w.id);
    assert.ok(!/NaN|undefined/.test(svg), `${w.id} has no broken numbers`);
  }
  assert.equal(galleryUrl("missing"), null);
});
