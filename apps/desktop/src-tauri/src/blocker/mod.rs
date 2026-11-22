//! Network and cosmetic content blocking powered by `adblock` (Brave's Rust ABP engine).
//!
//! Supports full Adblock Plus / EasyList / EasyPrivacy network syntax (URL patterns, wildcards,
//! `$domain=`, `$third-party`, `$important`, resource types, `@@` exceptions), hosts files,
//! plain domain lists, and CSS-only cosmetic element hiding (`##`, `#@#`). Scriptlet injection
//! rules (`##+js`) are stripped at parse time so filter lists never execute arbitrary scripts.

pub mod commands;

use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;

use crate::platform::urls::{host_of, site_of};

pub const BUILTIN_LIST: &str = include_str!("../../resources/blocklist.txt");

/// Defensive bounds against hostile or malformed filter lists (Section 10.1).
const MAX_LINES_PER_LIST: usize = 300_000;
const MAX_RULE_LEN: usize = 2048;
const MAX_COSMETIC_SELECTORS: usize = 512;

/// Incrementally collects filter lists and builds an immutable, thread-safe `Engine`.
pub struct Builder {
    set: FilterSet,
    rules: usize,
}

impl Default for Builder {
    fn default() -> Self {
        Self { set: FilterSet::new(false), rules: 0 }
    }
}

impl Builder {
    pub fn with_builtin() -> Self {
        let mut b = Self::default();
        b.add_list(BUILTIN_LIST);
        b
    }

    /// Parses a hosts file, plain domain list, or Adblock Plus / EasyList text into the set.
    /// Returns the number of accepted rules (bounded against pathological inputs).
    pub fn add_list(&mut self, text: &str) -> usize {
        let mut buf = String::with_capacity(text.len().min(1024 * 1024));
        let mut added = 0usize;
        for line in text.lines().take(MAX_LINES_PER_LIST) {
            if let Some(rule) = normalize_rule(line) {
                buf.push_str(&rule);
                buf.push('\n');
                added += 1;
            }
        }
        if added > 0 {
            self.set.add_filter_list(buf, ParseOptions::default());
            self.rules += added;
        }
        added
    }

    pub fn build(self) -> Engine {
        Engine { inner: adblock::Engine::new_with_filter_set(self.set), rules: self.rules }
    }
}

pub struct Engine {
    inner: adblock::Engine,
    rules: usize,
}

impl Default for Engine {
    fn default() -> Self {
        Builder::default().build()
    }
}

impl Engine {
    pub fn with_builtin() -> Self {
        Builder::with_builtin().build()
    }

    pub fn builder() -> Builder {
        Builder::default()
    }

    pub fn rule_count(&self) -> usize {
        self.rules
    }

    /// Full ABP network match for a CEF request (supports URL paths, `$domain=`, `$third-party`,
    /// `$important`, resource types, exceptions, and worker requests).
    pub fn should_block_request(&self, url: &str, source_url: &str, resource_type: &str) -> bool {
        if let (Some(req_host), Some(src_host)) = (host_of(url), host_of(source_url)) {
            if resource_type == "document" && site_of(req_host).eq_ignore_ascii_case(site_of(src_host)) {
                return false;
            }
        }
        let Ok(req) = Request::new(url, source_url, resource_type, "GET") else {
            return false;
        };
        self.inner.check_network_request(&req).should_block()
    }

    /// Convenience host-level subresource check (preserved for tests and simple callers).
    pub fn should_block(&self, request_host: &str, page_host: Option<&str>) -> bool {
        let host = request_host.trim_end_matches('.').to_ascii_lowercase();
        let url = format!("https://{host}/");
        let source = page_host
            .map(|h| format!("https://{}/", h.trim_end_matches('.').to_ascii_lowercase()))
            .unwrap_or_default();
        if let Some(page) = page_host {
            if site_of(page).eq_ignore_ascii_case(site_of(&host)) {
                return false;
            }
        }
        self.should_block_request(&url, &source, "script")
    }

    /// Convenience host-level document check (pages, frames, downloads).
    pub fn should_block_document(&self, request_host: &str, page_host: Option<&str>) -> bool {
        let host = request_host.trim_end_matches('.').to_ascii_lowercase();
        let url = format!("https://{host}/");
        let source = page_host
            .map(|h| format!("https://{}/", h.trim_end_matches('.').to_ascii_lowercase()))
            .unwrap_or_default();
        if let Some(page) = page_host {
            if site_of(page).eq_ignore_ascii_case(site_of(&host)) {
                return false;
            }
        }
        self.should_block_request(&url, &source, "document")
    }

    /// Returns a CSS-only cosmetic hiding stylesheet for `page_url`, or `None` if no rules match.
    /// Never returns scriptlets or executable JavaScript.
    pub fn cosmetic_css(&self, page_url: &str) -> Option<String> {
        let res = self.inner.url_cosmetic_resources(page_url);
        if res.hide_selectors.is_empty() {
            return None;
        }
        let mut selectors: Vec<&str> = res
            .hide_selectors
            .iter()
            .map(String::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 512 && !s.contains(['{', '}']))
            .take(MAX_COSMETIC_SELECTORS)
            .collect();
        if selectors.is_empty() {
            return None;
        }
        selectors.sort_unstable();
        let mut css = selectors.join(",\n");
        css.push_str(" { display: none !important; }");
        Some(css)
    }
}

/// Normalizes hosts-file lines and plain-domain lines into ABP rules while passing valid ABP
/// network and CSS cosmetic rules through untouched. Drops comments, scriptlets, and huge lines.
fn normalize_rule(raw: &str) -> Option<String> {
    let line = raw.trim();
    if line.is_empty() || line.len() > MAX_RULE_LEN || line.starts_with('!') || line.starts_with('[') {
        return None;
    }
    // Single `#` without cosmetic operator is a hosts-file comment.
    if line.starts_with('#') && !line.starts_with("##") && !line.starts_with("#@#") {
        return None;
    }
    // Never allow scriptlet / procedural JS execution or HTML filtering from filter lists.
    if line.contains("##+js(")
        || line.contains("#@#+js(")
        || line.contains("#?#")
        || line.contains("#$#")
        || line.contains("##^")
    {
        return None;
    }
    // Cosmetic CSS element hiding (`domain##.ad`, `domain#@#.ok`, `##.banner`).
    if line.contains("##") || line.contains("#@#") {
        return Some(line.to_string());
    }
    // Hosts-file entry: `0.0.0.0 ads.example.com`
    let mut parts = line.split_whitespace();
    if let (Some(ip), Some(host)) = (parts.next(), parts.next()) {
        if matches!(ip, "0.0.0.0" | "127.0.0.1" | "::" | "::1" | "::0") {
            return normalize_domain(host)
                .filter(|h| !matches!(h.as_str(), "localhost.localdomain" | "broadcasthost"))
                .map(|h| format!("||{h}^"));
        }
    }
    // Plain domain exception (`@@example.com`) or plain domain (`example.com`) used by built-in list.
    if let Some(rest) = line.strip_prefix("@@") {
        if !rest.contains(['/', '*', '^', '$', '|', ' ', '=', '@', '#']) {
            if let Some(d) = normalize_domain(rest) {
                return Some(format!("@@||{d}^"));
            }
        }
        return Some(line.to_string());
    }
    if !line.contains(['/', '*', '^', '$', '|', ' ', '=', '@', '#']) {
        return normalize_domain(line).map(|d| format!("||{d}^"));
    }
    Some(line.to_string())
}

fn normalize_domain(raw: &str) -> Option<String> {
    let d = raw
        .trim()
        .trim_start_matches("*.")
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let valid = d.len() > 3
        && d.contains('.')
        && !d.contains("..")
        && d.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'_');
    valid.then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_list_loads() {
        let engine = Engine::with_builtin();
        assert!(engine.rule_count() > 200, "only {} rules", engine.rule_count());
        assert!(engine.should_block("www.google-analytics.com", Some("news.example.com")));
        assert!(engine.should_block("securepubads.g.doubleclick.net", Some("example.com")));
        assert!(!engine.should_block("www.google.com", Some("example.com")));
        assert!(!engine.should_block("cdn.example.com", Some("www.example.com")));
    }

    #[test]
    fn parses_full_abp_network_and_cosmetic_rules() {
        let mut b = Engine::builder();
        let added = b.add_list(
            "! comment\n[Adblock Plus 2.0]\n\
             ||ads.example.net^\n\
             ||track.example.org^$third-party\n\
             ||cdn.example.org/ads/*\n\
             0.0.0.0 hosts.example.io\n\
             127.0.0.1 localhost\n\
             plain-domain.example\n\
             @@||good.ads.example.net^\n\
             ||scoped.example.com^$domain=foo.com\n\
             ||vip.example.com^$important\n\
             @@||vip.example.com^\n\
             news.example##.sponsored-banner\n\
             news.example#@#.keep-me\n\
             news.example##+js(abort-on-property-read, foo)\n",
        );
        assert!(added >= 9, "expected >= 9 parsed rules, got {added}");
        let engine = b.build();

        assert!(engine.should_block("ads.example.net", Some("site.test")));
        assert!(engine.should_block("x.ads.example.net", Some("site.test")));
        assert!(!engine.should_block("good.ads.example.net", Some("site.test")));
        assert!(engine.should_block("track.example.org", Some("site.test")));
        assert!(engine.should_block("hosts.example.io", Some("site.test")));
        assert!(engine.should_block("plain-domain.example", Some("site.test")));
        assert!(!engine.should_block("localhost", None));

        // Full ABP path, domain-scoped, and $important rules now work!
        assert!(engine.should_block_request(
            "https://cdn.example.org/ads/banner.js",
            "https://site.test/",
            "script",
        ));
        assert!(!engine.should_block_request(
            "https://cdn.example.org/app.js",
            "https://site.test/",
            "script",
        ));
        assert!(engine.should_block_request(
            "https://scoped.example.com/pixel.gif",
            "https://foo.com/page",
            "image",
        ));
        assert!(!engine.should_block_request(
            "https://scoped.example.com/pixel.gif",
            "https://bar.com/page",
            "image",
        ));
        assert!(engine.should_block_request(
            "https://vip.example.com/a.js",
            "https://site.test/",
            "script",
        ));

        // Cosmetic CSS hiding works and excludes scriptlet rules.
        let css = engine.cosmetic_css("https://news.example/article").expect("cosmetic css");
        assert!(css.contains(".sponsored-banner"));
        assert!(!css.contains("abort-on-property-read"));
    }

    #[test]
    fn subresource_rules_leave_documents_and_downloads_alone() {
        let mut b = Engine::builder();
        b.add_list(
            "||ads.example.net^\n\
             ||cdn.files.example^$third-party\n\
             ||img.example.org^$image,script\n\
             ||frames.example.io^$subdocument\n",
        );
        let engine = b.build();
        assert!(engine.should_block_document("ads.example.net", Some("site.test")));
        assert!(!engine.should_block_document("img.example.org", Some("site.test")));
        assert!(engine.should_block("cdn.files.example", Some("site.test")));
        assert!(engine.should_block("img.example.org", Some("site.test")));
        assert!(engine.should_block_request(
            "https://frames.example.io/embed",
            "https://site.test/",
            "subdocument",
        ));
    }

    #[test]
    fn first_party_is_never_blocked() {
        let mut b = Engine::builder();
        b.add_list("||metrics.shop.example^");
        let engine = b.build();
        assert!(!engine.should_block("metrics.shop.example", Some("www.shop.example")));
        assert!(engine.should_block("metrics.shop.example", Some("other.example")));
        assert!(engine.should_block("METRICS.shop.example", None));
    }

    #[test]
    fn hostile_long_rules_and_scriptlets_are_dropped() {
        let long_rule = format!("||{}example.com^", "a".repeat(MAX_RULE_LEN + 50));
        assert!(normalize_rule(&long_rule).is_none());
        assert!(normalize_rule("example.com##+js(set, x, 1)").is_none());
        assert!(normalize_rule("example.com##^script:has-text(ad)").is_none());
    }
}
