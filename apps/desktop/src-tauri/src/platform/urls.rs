//! URL helpers shared by external launches, palette fallbacks and the blocker.

use std::path::Path;
use tauri::Url;

/// Schemes a tab may be navigated to from the chrome UI.
pub fn is_navigable_scheme(scheme: &str) -> bool {
    matches!(
        scheme,
        "http" | "https" | "file" | "about" | "chrome-extension" | "view-source" | "data" | "blob"
    )
}

/// Validates a URL requested by the chrome UI for a tab navigation.
/// `javascript:` and unknown schemes are refused.
pub fn sanitize_navigation(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    if !is_navigable_scheme(url.scheme()) {
        return None;
    }
    // Top-level data: navigations are blocked by Chromium anyway; refuse them early.
    if url.scheme() == "data" || url.scheme() == "blob" {
        return None;
    }
    Some(url.to_string())
}

/// Converts a command-line argument passed by Windows (URL association, file association,
/// or another app) into a URL to open. Returns `None` for flags and unsafe inputs.
///
/// - `http`/`https` URLs are accepted.
/// - Existing local paths (file association, e.g. `C:\page.html`) become `file:///` URLs.
/// - Raw `file:`, `javascript:`, `data:` and other schemes from outside are refused.
pub fn external_arg_to_url(arg: &str) -> Option<String> {
    let arg = arg.trim().trim_matches('"');
    if arg.is_empty() || arg.starts_with('-') {
        return None;
    }
    if let Ok(url) = Url::parse(arg) {
        match url.scheme() {
            "http" | "https" => return Some(url.to_string()),
            // Drive letters parse as a one-letter scheme ("c:\\x"); fall through to path handling.
            s if s.len() == 1 => {}
            _ => return None,
        }
    }
    let path = Path::new(arg);
    if path.is_absolute() && path.exists() {
        return Url::from_file_path(path).ok().map(|u| u.to_string());
    }
    // Bare domains such as "example.com" from shell integrations.
    if looks_like_host(arg) {
        return Url::parse(&format!("https://{arg}")).ok().map(|u| u.to_string());
    }
    None
}

fn looks_like_host(s: &str) -> bool {
    let host = s.split(['/', '?', '#']).next().unwrap_or("");
    !host.is_empty()
        && host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
}

/// Extracts the lowercase host of an absolute URL without allocating when possible.
pub fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let authority = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// Approximates the registrable domain (eTLD+1) without a public suffix list.
/// Good enough to treat `cdn.example.co.uk` and `www.example.co.uk` as the same site.
pub fn site_of(host: &str) -> &str {
    const SECOND_LEVEL: &[&str] = &[
        "co", "com", "net", "org", "gov", "edu", "ac", "or", "ne", "go", "gob", "mil", "nic",
    ];
    let host = host.trim_end_matches('.');
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host;
    }
    let parts: Vec<&str> = host.split('.').collect();
    let n = parts.len();
    if n <= 2 {
        return host;
    }
    let take = if parts[n - 1].len() == 2 && SECOND_LEVEL.contains(&parts[n - 2]) {
        3
    } else {
        2
    };
    if n <= take {
        return host;
    }
    let skip: usize = parts[..n - take].iter().map(|p| p.len() + 1).sum();
    &host[skip..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_script_urls() {
        assert_eq!(sanitize_navigation("javascript:alert(1)"), None);
        assert_eq!(sanitize_navigation("vbscript:x"), None);
        assert_eq!(sanitize_navigation("data:text/html,hi"), None);
        assert_eq!(
            sanitize_navigation("https://example.com").as_deref(),
            Some("https://example.com/")
        );
        assert!(sanitize_navigation("about:blank").is_some());
    }

    #[test]
    fn external_args() {
        assert_eq!(
            external_arg_to_url("https://example.com/a?b").as_deref(),
            Some("https://example.com/a?b")
        );
        assert_eq!(external_arg_to_url("javascript:alert(1)"), None);
        assert_eq!(external_arg_to_url("file:///C:/Windows/win.ini"), None);
        assert_eq!(external_arg_to_url("--flag"), None);
        assert_eq!(
            external_arg_to_url("example.org").as_deref(),
            Some("https://example.org/")
        );
        let win_ini = "C:\\Windows\\win.ini";
        if Path::new(win_ini).exists() {
            assert_eq!(
                external_arg_to_url(win_ini).as_deref(),
                Some("file:///C:/Windows/win.ini")
            );
        }
    }

    #[test]
    fn hosts_and_sites() {
        assert_eq!(host_of("https://User@Sub.Example.com:8080/x"), Some("Sub.Example.com"));
        assert_eq!(host_of("http://[::1]:80/"), Some("::1"));
        assert_eq!(host_of("about:blank"), None);
        assert_eq!(site_of("a.b.example.com"), "example.com");
        assert_eq!(site_of("www.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(site_of("example.com"), "example.com");
        assert_eq!(site_of("localhost"), "localhost");
        assert_eq!(site_of("127.0.0.1"), "127.0.0.1");
    }
}
