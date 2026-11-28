import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { matchesHostPattern, siteAccessStatus } from "./site-access.ts";

describe("matchesHostPattern", () => {
  it("matches <all_urls> and wildcard schemes for http/https only", () => {
    assert.equal(matchesHostPattern("<all_urls>", "https://example.com/a"), true);
    assert.equal(matchesHostPattern("*://*/*", "http://example.com/"), true);
    assert.equal(matchesHostPattern("<all_urls>", "zenith://extensions"), false);
    assert.equal(matchesHostPattern("<all_urls>", "chrome-extension://abc/popup.html"), false);
  });

  it("matches exact hosts, wildcard subdomains, and paths", () => {
    assert.equal(matchesHostPattern("https://*.github.com/*", "https://github.com/zen"), true);
    assert.equal(matchesHostPattern("https://*.github.com/*", "https://api.github.com/repos"), true);
    assert.equal(matchesHostPattern("https://*.github.com/*", "https://evilgithub.com/"), false);
    assert.equal(matchesHostPattern("https://example.com/docs/*", "https://example.com/docs/a"), true);
    assert.equal(matchesHostPattern("https://example.com/docs/*", "https://example.com/other"), false);
  });
});

describe("siteAccessStatus", () => {
  it("reports restricted on internal pages, allowed on matched sites, and on-click for activeTab", () => {
    const hostExt = { enabled: true, hostPermissions: ["<all_urls>"], hasActiveTab: false };
    assert.equal(siteAccessStatus(hostExt, "zenith://newtab").state, "restricted");
    assert.equal(siteAccessStatus(hostExt, "https://example.com/").state, "allowed");

    const clickExt = { enabled: true, hostPermissions: [], hasActiveTab: true };
    assert.equal(siteAccessStatus(clickExt, "https://example.com/").state, "on-click");

    const disabledExt = { enabled: false, hostPermissions: ["<all_urls>"], hasActiveTab: true };
    assert.equal(siteAccessStatus(disabledExt, "https://example.com/").label, "Disabled");
  });
});
