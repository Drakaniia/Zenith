//! Addresses: the origin a credential belongs to, how it is shown, the "never save" key and
//! whether a saved origin matches a page.

use tauri::Url;

use super::*;

// ---------------------------------------------------------------------------------------------
// Addresses

/// The origin (`https://host[:port]`) a credential belongs to, from a URL or a bare domain.
pub fn normalize_origin(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() || s.len() > 2048 {
        return None;
    }
    let full = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
    let url = Url::parse(&full).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?;
    if host.is_empty() {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

fn host_of_origin(origin: &str) -> String {
    Url::parse(origin).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default()
}

/// Host for display.
pub fn display_site(origin: &str) -> String {
    let host = host_of_origin(origin);
    host.strip_prefix("www.").map(str::to_string).unwrap_or(host)
}

/// The site a "never save" choice applies to.
pub fn never_key(origin: &str) -> String {
    let host = host_of_origin(origin);
    crate::platform::urls::site_of(&host).to_ascii_lowercase()
}

/// Whether a credential saved for `saved` may be used on a page at `page` (both origins).
/// `Some(true)`: the same origin. `Some(false)`: another origin of the same site (only with
/// `same_site`). HTTPS credentials are never offered to plain-HTTP pages, IP addresses and
/// single-label hosts must match exactly, and shared hosting domains never share credentials.
pub fn origin_match(saved: &str, page: &str, same_site: bool) -> Option<bool> {
    if saved == page {
        return Some(true);
    }
    if !same_site {
        return None;
    }
    let (s, p) = (Url::parse(saved).ok()?, Url::parse(page).ok()?);
    if !matches!(p.scheme(), "http" | "https") || (s.scheme() == "https" && p.scheme() != "https") {
        return None;
    }
    let (sh, ph) = (s.host_str()?.to_ascii_lowercase(), p.host_str()?.to_ascii_lowercase());
    let simple = |h: &str| !h.contains('.') || h.parse::<std::net::IpAddr>().is_ok() || h.starts_with('[');
    if simple(&sh) || simple(&ph) {
        return None;
    }
    let (ss, ps) = (crate::platform::urls::site_of(&sh), crate::platform::urls::site_of(&ph));
    if ss != ps {
        return None;
    }
    if SHARED_SUFFIXES.iter().any(|x| *x == ss) {
        return None;
    }
    Some(false)
}
