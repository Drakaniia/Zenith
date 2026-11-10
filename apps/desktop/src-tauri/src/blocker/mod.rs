//! Network ad and tracker blocking engine.
//!
//! Domain-based matcher that understands hosts files, plain domain lists and the
//! domain-anchored subset of Adblock Plus syntax (`||example.com^`, `@@||example.com^`),
//! which covers the bulk of EasyList / EasyPrivacy network rules. Lookups walk the host's
//! parent domains against two hash sets, so the cost per request is a few hash probes.

pub mod commands;

use std::collections::HashSet;

use crate::platform::urls::site_of;

pub const BUILTIN_LIST: &str = include_str!("../../resources/blocklist.txt");

#[derive(Default)]
pub struct Engine {
    block: HashSet<Box<str>>,
    /// Hosts whose rules name only subresource types (`$script`, `$image`, ...) or only
    /// `$third-party`: they never apply to pages, frames or the files a link downloads.
    subresource: HashSet<Box<str>>,
    allow: HashSet<Box<str>>,
}

impl Engine {
    pub fn with_builtin() -> Self {
        let mut engine = Self::default();
        engine.add_list(BUILTIN_LIST);
        engine
    }

    pub fn rule_count(&self) -> usize {
        self.block.len() + self.subresource.len() + self.allow.len()
    }

    /// Adds rules from a hosts file, plain domain list or Adblock Plus list.
    /// Returns the number of new rules.
    pub fn add_list(&mut self, text: &str) -> usize {
        let mut added = 0;
        for line in text.lines() {
            if let Some((domain, exception, documents)) = parse_line(line) {
                let set = match (exception, documents) {
                    (true, _) => &mut self.allow,
                    (false, true) => &mut self.block,
                    (false, false) => &mut self.subresource,
                };
                if set.insert(domain.into_boxed_str()) {
                    added += 1;
                }
            }
        }
        added
    }

    /// Decides whether a subresource request to `request_host` should be blocked when the
    /// top-level page is on `page_host`. Requests to the page's own site are never blocked.
    pub fn should_block(&self, request_host: &str, page_host: Option<&str>) -> bool {
        self.lookup(request_host, page_host, false)
    }

    /// Like `should_block`, for a document request (a page, a frame, or a link that turns out to
    /// be a download): only rules meant for whole hosts apply, not ones for scripts or images.
    pub fn should_block_document(&self, request_host: &str, page_host: Option<&str>) -> bool {
        self.lookup(request_host, page_host, true)
    }

    fn lookup(&self, request_host: &str, page_host: Option<&str>, document: bool) -> bool {
        if let Some(page) = page_host {
            if site_of(page).eq_ignore_ascii_case(site_of(request_host)) {
                return false;
            }
        }
        let lowered;
        let mut host = if request_host.bytes().any(|b| b.is_ascii_uppercase()) {
            lowered = request_host.to_ascii_lowercase();
            lowered.as_str()
        } else {
            request_host
        };
        host = host.trim_end_matches('.');
        loop {
            if self.allow.contains(host) {
                return false;
            }
            if self.block.contains(host) || (!document && self.subresource.contains(host)) {
                return true;
            }
            match host.find('.') {
                Some(i) => host = &host[i + 1..],
                None => return false,
            }
        }
    }
}

/// A rule as (domain, exception, applies to documents).
fn parse_line(line: &str) -> Option<(String, bool, bool)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('!') || line.starts_with('#') || line.starts_with('[') {
        return None;
    }
    // Cosmetic (element hiding / scriptlet) rules are not network rules.
    if line.contains("##") || line.contains("#@#") || line.contains("#?#") || line.contains("#$#") {
        return None;
    }
    let (body, exception) = match line.strip_prefix("@@") {
        Some(rest) => (rest, true),
        None => (line, false),
    };

    if !exception {
        let mut parts = body.split_whitespace();
        if let (Some(ip), Some(host)) = (parts.next(), parts.next()) {
            if matches!(ip, "0.0.0.0" | "127.0.0.1" | "::" | "::1" | "::0") {
                return normalize_domain(host)
                    .filter(|h| !matches!(h.as_str(), "localhost.localdomain" | "broadcasthost"))
                    .map(|h| (h, false, true));
            }
        }
    }

    if let Some(rest) = body.strip_prefix("||") {
        let (pattern, options) = match rest.split_once('$') {
            Some((p, o)) => (p, Some(o)),
            None => (rest, None),
        };
        let domain = pattern
            .strip_suffix("^|")
            .or_else(|| pattern.strip_suffix('^'))
            .unwrap_or(pattern);
        if domain.contains(['/', '*', '^', '|', '?', '=']) {
            return None; // path or wildcard rule: outside the domain-level subset
        }
        if let Some(options) = options {
            if !broad_options(options) {
                return None;
            }
        }
        let documents = options.is_none_or(applies_to_documents);
        return normalize_domain(domain).map(|d| (d, exception, documents));
    }

    if !exception && !body.contains(['/', '*', '^', '$', '|', ' ', '=', '@', '#']) {
        return normalize_domain(body).map(|d| (d, false, true));
    }
    None
}

/// Rule options that still mean "this whole host is an ad/tracking endpoint".
fn broad_options(options: &str) -> bool {
    options.split(',').all(|o| {
        matches!(
            o.trim(),
            "third-party"
                | "3p"
                | "script"
                | "image"
                | "stylesheet"
                | "xmlhttprequest"
                | "xhr"
                | "subdocument"
                | "frame"
                | "media"
                | "font"
                | "object"
                | "ping"
                | "websocket"
                | "other"
                | "all"
                | "important"
                | "document"
                | "doc"
        )
    })
}

/// Options that still cover documents: pages and frames (the request type a download has).
/// `$third-party` alone does not: a page or download the user opened is never third-party.
fn applies_to_documents(options: &str) -> bool {
    let opts: Vec<&str> = options.split(',').map(str::trim).collect();
    let modifier = |o: &&str| matches!(*o, "third-party" | "3p" | "important");
    if opts.iter().all(modifier) {
        return !opts.iter().any(|o| matches!(*o, "third-party" | "3p"));
    }
    opts.iter().any(|o| matches!(*o, "all" | "document" | "doc" | "subdocument" | "frame"))
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
    fn parses_list_formats() {
        let mut engine = Engine::default();
        let added = engine.add_list(
            "! comment\n[Adblock Plus 2.0]\n||ads.example.net^\n||track.example.org^$third-party\n\
             ||cdn.example.org/ads/*\n0.0.0.0 hosts.example.io\n127.0.0.1 localhost\n\
             plain-domain.example\n@@||good.ads.example.net^\nexample.com##.banner\n\
             ||scoped.example.com^$domain=foo.com\n",
        );
        assert_eq!(added, 5);
        assert!(engine.should_block("ads.example.net", Some("site.test")));
        assert!(engine.should_block("x.ads.example.net", Some("site.test")));
        assert!(!engine.should_block("good.ads.example.net", Some("site.test")));
        assert!(engine.should_block("track.example.org", Some("site.test")));
        assert!(engine.should_block("hosts.example.io", Some("site.test")));
        assert!(engine.should_block("plain-domain.example", Some("site.test")));
        assert!(!engine.should_block("cdn.example.org", Some("site.test")));
        assert!(!engine.should_block("scoped.example.com", Some("site.test")));
        assert!(!engine.should_block("localhost", None));
    }

    #[test]
    fn subresource_rules_leave_documents_and_downloads_alone() {
        let mut engine = Engine::default();
        engine.add_list("||ads.example.net^\n||cdn.files.example^$third-party\n||img.example.org^$image,script\n||frames.example.io^$subdocument\n");
        assert!(engine.should_block_document("ads.example.net", Some("site.test")));
        assert!(engine.should_block_document("frames.example.io", Some("site.test")));
        assert!(!engine.should_block_document("cdn.files.example", Some("site.test")));
        assert!(!engine.should_block_document("img.example.org", Some("site.test")));
        assert!(engine.should_block("cdn.files.example", Some("site.test")), "still blocked as a subresource");
        assert!(engine.should_block("img.example.org", Some("site.test")));
    }

    #[test]
    fn first_party_is_never_blocked() {
        let mut engine = Engine::default();
        engine.add_list("||metrics.shop.example^");
        assert!(!engine.should_block("metrics.shop.example", Some("www.shop.example")));
        assert!(engine.should_block("metrics.shop.example", Some("other.example")));
        assert!(engine.should_block("METRICS.shop.example", None));
    }
}
