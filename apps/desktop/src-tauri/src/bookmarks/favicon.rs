//! Favicons for bookmarks: where a site's icon is fetched from, and turning the bytes into a data URL.

// ---------------------------------------------------------------------------------------------
// Favicons

/// Largest favicon file read.
pub const FAVICON_MAX_BYTES: usize = 128 * 1024;

/// The address of a site's default icon, `/favicon.ico` on the page's own origin. None for
/// anything but a plain http(s) address.
pub fn favicon_url(page: &str) -> Option<String> {
    let rest = page.strip_prefix("https://").map(|r| ("https", r)).or_else(|| page.strip_prefix("http://").map(|r| ("http", r)))?;
    let authority = rest.1.split(['/', '?', '#']).next()?;
    // Credentials in the address are never sent along.
    if authority.is_empty() || authority.contains('@') || authority.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    // Judge the host the way the request will see it: parsed, so that forms like 127.1, 0x7f.1 or
    // 2130706433 are the loopback address they are. Only real public host names and public IPv4
    // addresses are asked.
    let url = tauri::Url::parse(&format!("{}://{authority}/favicon.ico", rest.0)).ok()?;
    let host = url.host_str()?;
    let public = if url.domain().is_some() {
        host.rsplit('.').next().is_some_and(|tld| tld.chars().any(|c| c.is_ascii_alphabetic())) && !crate::media::detect::is_private_host(host)
    } else {
        // An address: IPv4 only (IPv6 forms are too many to judge), and not a private one.
        !host.contains(':') && !crate::media::detect::is_private_host(host)
    };
    public.then(|| url.to_string())
}

/// A `data:` URL for an icon file, when its first bytes say it is a raster image. SVG is not
/// accepted: the UI shows icons in `<img>` where it would be harmless, but nothing needs it.
pub fn favicon_data_url(bytes: &[u8]) -> Option<String> {
    use base64::Engine;
    if bytes.is_empty() || bytes.len() > FAVICON_MAX_BYTES {
        return None;
    }
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) && bytes.len() > 6 {
        "image/x-icon"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return None;
    };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favicons_come_from_the_site_itself_and_only_as_images() {
        assert_eq!(favicon_url("https://example.com/a/b?c#d").as_deref(), Some("https://example.com/favicon.ico"));
        assert_eq!(favicon_url("http://93.184.216.34/x").as_deref(), Some("http://93.184.216.34/favicon.ico"));
        for bad in [
            "ftp://example.com/", "javascript:alert(1)", "https://", "https://user:pw@example.com/", "https://exa mple.com/", "example.com",
            "http://localhost:3000/x", "http://127.0.0.1/", "http://127.1/", "http://0x7f.0.0.1/", "http://2130706433/", "http://192.168.1.1/",
            "http://169.254.169.254/", "http://[::1]/", "http://10.0.0.5:8080/", "http://router.local/", "http://intranet/", "http://0177.0.0.1/",
        ] {
            assert!(favicon_url(bad).is_none(), "{bad}");
        }
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
        assert!(favicon_data_url(&png).unwrap().starts_with("data:image/png;base64,"));
        assert!(favicon_data_url(&[0, 0, 1, 0, 1, 0, 16, 16]).unwrap().starts_with("data:image/x-icon;base64,"));
        assert!(favicon_data_url(b"<svg onload=alert(1)></svg>").is_none());
        assert!(favicon_data_url(b"<!DOCTYPE html><html>").is_none());
        assert!(favicon_data_url(&[]).is_none());
        assert!(favicon_data_url(&vec![0xFF; FAVICON_MAX_BYTES + 1]).is_none());
    }
}
