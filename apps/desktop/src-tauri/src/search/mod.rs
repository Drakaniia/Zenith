//! Search suggestions as the user types, from the search engine they chose.

pub mod commands;

use crate::platform::net;

/// Fetches query suggestions in the OpenSearch JSON format (`["q", ["s1", "s2"]]`) from the
/// chosen engine. An engine without a known endpoint gets none: typing is never sent to a
/// different provider than the one the user picked.
pub fn suggestions(engine: &str, query: &str) -> Result<Vec<String>, String> {
    let Some(endpoint) = suggest_endpoint(engine, query) else {
        return Ok(Vec::new());
    };
    let body = net::get(&endpoint, 256 * 1024, 4000)?;
    let value: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("bad suggestion payload: {e}"))?;
    Ok(value
        .get(1)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .take(8)
                .collect()
        })
        .unwrap_or_default())
}

/// Hosts the address bar's instant answers may query; any other address is refused.
const INSTANT_HOSTS: &[&str] = &["api.frankfurter.dev", "geocoding-api.open-meteo.com", "api.open-meteo.com"];

pub fn instant_fetch(url: &str) -> Result<String, String> {
    if !instant_allowed(url) {
        return Err("address not allowed".into());
    }
    // No redirects: only the address given was checked against the allowed hosts.
    let body = net::get_no_redirects(url, 64 * 1024, 4000)?;
    String::from_utf8(body).map_err(|e| e.to_string())
}

fn instant_allowed(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| u.scheme() == "https" && u.port().is_none() && u.host_str().is_some_and(|h| INSTANT_HOSTS.contains(&h)))
}

fn suggest_endpoint(engine: &str, query: &str) -> Option<String> {
    let q = url_encode(query);
    Some(match engine {
        "google" => format!("https://suggestqueries.google.com/complete/search?client=firefox&q={q}"),
        "bing" => format!("https://api.bing.com/osjson.aspx?query={q}"),
        "duckduckgo" => format!("https://duckduckgo.com/ac/?q={q}&type=list"),
        "brave" => format!("https://search.brave.com/api/suggest?q={q}"),
        "ecosia" => format!("https://ac.ecosia.org/autocomplete?q={q}&type=list"),
        "yandex" => format!("https://suggest.yandex.com/suggest-ff.cgi?part={q}"),
        _ => return None,
    })
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_queries() {
        assert_eq!(super::url_encode("a b&c/é"), "a+b%26c%2F%C3%A9");
    }

    #[test]
    fn suggestions_stay_with_the_chosen_engine() {
        let ddg = super::suggest_endpoint("duckduckgo", "a b").unwrap();
        assert!(ddg.starts_with("https://duckduckgo.com/") && ddg.contains("q=a+b"));
        assert!(super::suggest_endpoint("unknown", "x").is_none());
    }

    #[test]
    fn instant_fetches_stay_on_the_allowed_hosts() {
        assert!(super::instant_allowed("https://api.frankfurter.dev/v1/latest?from=USD"));
        assert!(super::instant_allowed("https://api.open-meteo.com/v1/forecast?latitude=1&longitude=2"));
        assert!(!super::instant_allowed("http://api.open-meteo.com/v1/forecast"));
        assert!(!super::instant_allowed("https://api.open-meteo.com:8443/v1/forecast"));
        assert!(!super::instant_allowed("https://evil.example/?api.frankfurter.dev"));
        assert!(!super::instant_allowed("https://api.frankfurter.dev.evil.example/"));
    }
}
