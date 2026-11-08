//! Pure helpers for Chrome extension packages and the Chrome Web Store: `.crx` (v2/v3) header
//! parsing, extension ids, install-prompt permission warnings and update-check responses.
//! Nothing here touches WebView2, so it is unit tested directly.

use base64::Engine as _;
use sha2::{Digest, Sha256};

/// A parsed `.crx` package.
pub struct Crx<'a> {
    /// The zip archive that follows the header.
    pub zip: &'a [u8],
    /// The 32-letter extension id Chrome assigns this package (from its signing key).
    pub id: Option<String>,
    /// The developer public key (DER, SubjectPublicKeyInfo) that the id derives from.
    pub public_key: Option<Vec<u8>>,
}

pub fn parse(bytes: &[u8]) -> Result<Crx<'_>, String> {
    if bytes.len() < 16 || &bytes[..4] != b"Cr24" {
        return Err("not a Chrome extension package (.crx)".into());
    }
    let u32_at = |i: usize| -> usize {
        u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize
    };
    match u32_at(4) {
        3 => {
            let len = u32_at(8);
            let header = bytes.get(12..12 + len).ok_or("truncated .crx package")?;
            let zip = bytes.get(12 + len..).ok_or("truncated .crx package")?;
            let (id, public_key) = crx3_identity(header);
            Ok(Crx { zip, id, public_key })
        }
        2 => {
            let (key_len, sig_len) = (u32_at(8), u32_at(12));
            let key = bytes.get(16..16 + key_len).ok_or("truncated .crx package")?;
            let zip = bytes.get(16 + key_len + sig_len..).ok_or("truncated .crx package")?;
            Ok(Crx { zip, id: Some(id_from_key(key)), public_key: Some(key.to_vec()) })
        }
        v => Err(format!("unsupported .crx version {v}")),
    }
}

/// The extension id for a public key: the first 16 bytes of its SHA-256, one letter a-p per
/// nibble (Chrome's scheme; also how unpacked extensions with a manifest `key` get their id).
pub fn id_from_key(key: &[u8]) -> String {
    id_from_bytes(&sha256(key)[..16])
}

fn id_from_bytes(b: &[u8]) -> String {
    b.iter()
        .flat_map(|byte| [byte >> 4, byte & 0xf])
        .map(|n| (b'a' + n) as char)
        .collect()
}

/// The manifest `key` value for a public key, so the unpacked copy keeps the store id.
pub fn manifest_key(key: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(key)
}

// -------------------------------------------------------------------------------------------
// CRX3 header (protobuf CrxFileHeader)

fn varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *buf.get(*pos)?;
        *pos += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// Calls `f(field, bytes)` for every length-delimited field of a protobuf message.
fn each_bytes_field<'a>(buf: &'a [u8], mut f: impl FnMut(u64, &'a [u8])) {
    let mut pos = 0;
    while pos < buf.len() {
        let Some(tag) = varint(buf, &mut pos) else { return };
        match tag & 7 {
            0 => {
                if varint(buf, &mut pos).is_none() {
                    return;
                }
            }
            1 => pos += 8,
            5 => pos += 4,
            2 => {
                let Some(len) = varint(buf, &mut pos) else { return };
                let end = pos.saturating_add(len as usize);
                let Some(data) = buf.get(pos..end) else { return };
                f(tag >> 3, data);
                pos = end;
            }
            _ => return,
        }
    }
}

fn crx3_identity(header: &[u8]) -> (Option<String>, Option<Vec<u8>>) {
    let mut keys: Vec<&[u8]> = Vec::new();
    let mut crx_id: Option<Vec<u8>> = None;
    each_bytes_field(header, |field, data| match field {
        // sha256_with_rsa / sha256_with_ecdsa: AsymmetricKeyProof { public_key = 1 }
        2 | 3 => each_bytes_field(data, |f, key| {
            if f == 1 {
                keys.push(key);
            }
        }),
        // signed_header_data: SignedData { crx_id = 1 }
        10000 => each_bytes_field(data, |f, id| {
            if f == 1 && id.len() == 16 {
                crx_id = Some(id.to_vec());
            }
        }),
        _ => {}
    });
    let Some(crx_id) = crx_id else { return (None, None) };
    // The package is also signed by the Web Store's own key; the developer key is the one whose
    // hash is the id.
    let key = keys.into_iter().find(|k| sha256(k)[..16] == crx_id[..]).map(<[u8]>::to_vec);
    (Some(id_from_bytes(&crx_id)), key)
}

// -------------------------------------------------------------------------------------------
// SHA-256 (FIPS 180-4), only used for extension ids.

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

// -------------------------------------------------------------------------------------------
// Chrome Web Store

/// Extracts the 32-letter extension id from a Chrome Web Store URL, a download file name
/// (`<id>_<n>.crx`) or a bare id.
pub fn store_id(input: &str) -> Option<String> {
    input
        .split(|c: char| !c.is_ascii_lowercase())
        .find(|part| part.len() == 32 && part.chars().all(|c| ('a'..='p').contains(&c)))
        .map(str::to_string)
}

/// Chromium version reported to the store (extensions may require a minimum version).
pub fn prodversion() -> String {
    wry::webview_version()
        .ok()
        .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or_else(|| "140.0.0.0".into())
}

pub fn download_url(id: &str, prodversion: &str) -> String {
    format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&prodversion={prodversion}&acceptformat=crx2,crx3&x=id%3D{id}%26uc"
    )
}

/// One update-check request for several extensions: `(store id, installed version)`.
pub fn update_check_url(items: &[(String, String)], prodversion: &str) -> String {
    let mut url = format!(
        "https://clients2.google.com/service/update2/crx?response=updatecheck&prodversion={prodversion}&acceptformat=crx2,crx3"
    );
    for (id, version) in items {
        let v: String = version.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        url.push_str(&format!("&x=id%3D{id}%26v%3D{v}%26uc"));
    }
    url
}

#[derive(Debug, PartialEq)]
pub struct UpdateCheck {
    pub id: String,
    /// "ok" (an update is offered), "noupdate", or an error status.
    pub status: String,
    pub version: Option<String>,
}

fn attr(tag: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!(" {name}={quote}");
        if let Some(start) = tag.find(&needle) {
            let rest = &tag[start + needle.len()..];
            return rest.find(quote).map(|end| rest[..end].to_string());
        }
    }
    None
}

/// Parses the store's gupdate XML response.
pub fn parse_update_response(xml: &str) -> Vec<UpdateCheck> {
    let mut out = Vec::new();
    for chunk in xml.split("<app ").skip(1) {
        let app_tag = &chunk[..chunk.find('>').unwrap_or(chunk.len())];
        let Some(id) = attr(&format!(" {app_tag}"), "appid") else { continue };
        let Some(start) = chunk.find("<updatecheck") else {
            out.push(UpdateCheck { id, status: attr(&format!(" {app_tag}"), "status").unwrap_or_default(), version: None });
            continue;
        };
        let rest = &chunk[start..];
        let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
        out.push(UpdateCheck {
            id,
            status: attr(tag, "status").unwrap_or_default(),
            version: attr(tag, "version"),
        });
    }
    out
}

/// True when dotted version `a` is newer than `b` ("1.10" > "1.9").
pub fn is_newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.trim().parse().unwrap_or(0)).collect() };
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let (x, y) = (pa.get(i).copied().unwrap_or(0), pb.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

// -------------------------------------------------------------------------------------------
// Install prompt

fn host_pattern_host(pattern: &str) -> Option<String> {
    if pattern == "<all_urls>" {
        return Some("*".into());
    }
    let rest = pattern.split_once("://")?.1;
    let host = rest.split('/').next().unwrap_or("");
    let host = host.trim_start_matches("*.");
    if host.is_empty() {
        return None;
    }
    Some(host.to_string())
}

/// What an extension may do, worded like Chrome's install dialog.
pub fn permission_warnings(manifest: &serde_json::Value) -> Vec<String> {
    let strings = |v: Option<&serde_json::Value>| -> Vec<String> {
        v.and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    };
    let mut perms = strings(manifest.get("permissions"));
    perms.extend(strings(manifest.get("optional_permissions")).into_iter().filter(|p| p.contains("://")));
    let mut hosts: Vec<String> = Vec::new();
    let mut add_host = |p: &str| {
        if let Some(h) = host_pattern_host(p) {
            if !hosts.contains(&h) {
                hosts.push(h);
            }
        }
    };
    for p in perms.iter().chain(strings(manifest.get("host_permissions")).iter()) {
        if p.contains("://") || p == "<all_urls>" {
            add_host(p);
        }
    }
    if let Some(scripts) = manifest.get("content_scripts").and_then(|c| c.as_array()) {
        for s in scripts {
            for m in strings(s.get("matches")) {
                add_host(&m);
            }
        }
    }
    let mut out = Vec::new();
    if hosts.iter().any(|h| h == "*") {
        out.push("Read and change all your data on all websites".to_string());
    } else if hosts.len() > 3 {
        out.push(format!("Read and change your data on {} websites", hosts.len()));
    } else if !hosts.is_empty() {
        out.push(format!("Read and change your data on {}", hosts.join(", ")));
    }
    for p in &perms {
        let text = match p.as_str() {
            "tabs" | "webNavigation" => "Read your browsing history",
            "history" => "Read and change your browsing history",
            "bookmarks" => "Read and change your bookmarks",
            "downloads" => "Manage your downloads",
            "notifications" => "Display notifications",
            "clipboardRead" => "Read data you copy and paste",
            "clipboardWrite" => "Modify data you copy and paste",
            "declarativeNetRequest" | "declarativeNetRequestWithHostAccess" => "Block content on any page",
            "nativeMessaging" => "Communicate with cooperating native applications",
            "management" => "Manage your apps, extensions, and themes",
            "privacy" => "Change your privacy-related settings",
            "proxy" => "Change your proxy settings",
            "geolocation" => "Detect your physical location",
            "debugger" => "Access the page debugger backend",
            "topSites" => "Read a list of your most frequently visited websites",
            "pageCapture" => "Read and change all your data on all websites",
            _ => continue,
        };
        if !out.iter().any(|o| o == text) {
            out.push(text.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        let hex = |b: [u8; 32]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = vec![b'a'; 1000];
        assert_eq!(hex(sha256(&long)), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
    }

    fn field(num: u64, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut put = |mut v: u64| loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        };
        put((num << 3) | 2);
        put(data.len() as u64);
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn reads_crx3_identity() {
        let dev_key = b"developer-public-key".to_vec();
        let store_key = b"web-store-public-key".to_vec();
        let id_bytes = sha256(&dev_key)[..16].to_vec();
        let mut header = field(2, &field(1, &store_key));
        header.extend(field(2, &field(1, &dev_key)));
        header.extend(field(10000, &field(1, &id_bytes)));
        let mut crx = b"Cr24".to_vec();
        crx.extend(3u32.to_le_bytes());
        crx.extend((header.len() as u32).to_le_bytes());
        crx.extend(&header);
        crx.extend(b"PK\x03\x04zip");
        let parsed = parse(&crx).unwrap();
        assert_eq!(parsed.zip, b"PK\x03\x04zip");
        assert_eq!(parsed.public_key.as_deref(), Some(&dev_key[..]));
        assert_eq!(parsed.id.as_deref(), Some(id_from_key(&dev_key).as_str()));
        assert!(parse(b"PK\x03\x04 not a crx").is_err());
    }

    #[test]
    fn ids_use_letters_a_to_p() {
        let id = id_from_key(b"key");
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| ('a'..='p').contains(&c)));
    }

    #[test]
    fn finds_store_ids() {
        let id = "ddkjiahejlhfcafbddmgiahcphecmpfh";
        assert_eq!(store_id(&format!("https://chromewebstore.google.com/detail/ublock-origin-lite/{id}")).as_deref(), Some(id));
        assert_eq!(store_id(&format!("{id}_60977.crx")).as_deref(), Some(id));
        assert_eq!(store_id(id).as_deref(), Some(id));
        assert_eq!(store_id("https://example.com/"), None);
    }

    #[test]
    fn parses_update_responses() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><gupdate xmlns="http://www.google.com/update2/response" protocol="2.0" server="prod"><daystart elapsed_days="7000" elapsed_seconds="100"/><app appid="aaaa" cohort="1::" status="ok"><updatecheck codebase="https://x/a.crx" fp="1.x" hash_sha256="00" protected="0" size="10" status="ok" version="2025.11.2.1"/></app><app appid="bbbb" status="ok"><updatecheck status="noupdate"/></app></gupdate>"#;
        let r = parse_update_response(xml);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], UpdateCheck { id: "aaaa".into(), status: "ok".into(), version: Some("2025.11.2.1".into()) });
        assert_eq!(r[1].status, "noupdate");
        assert!(update_check_url(&[("aaaa".into(), "1.2".into())], "140.0").contains("x=id%3Daaaa%26v%3D1.2%26uc"));
    }

    /// Downloads uBlock Origin Lite from the Web Store: `cargo test real_store -- --ignored`.
    #[test]
    #[ignore]
    fn real_store_package_keeps_its_id_and_updates() {
        let id = "ddkjiahejlhfcafbddmgiahcphecmpfh";
        let bytes = crate::platform::net::get(&download_url(id, "140.0.0.0"), 64 << 20, 60_000).unwrap();
        let crx = parse(&bytes).unwrap();
        assert_eq!(crx.id.as_deref(), Some(id));
        assert_eq!(id_from_key(crx.public_key.as_deref().expect("developer key")), id);
        assert_eq!(&crx.zip[..2], b"PK");
        let xml = crate::platform::net::get(&update_check_url(&[(id.into(), "0.1".into())], "140.0.0.0"), 1 << 20, 20_000).unwrap();
        let r = parse_update_response(&String::from_utf8_lossy(&xml));
        assert_eq!(r.len(), 1, "{}", String::from_utf8_lossy(&xml));
        assert_eq!(r[0].status, "ok");
        assert!(is_newer(r[0].version.as_deref().unwrap(), "0.1"));
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("1.10", "1.9"));
        assert!(is_newer("2025.11.2.1", "2025.11.2"));
        assert!(!is_newer("1.0", "1.0.0"));
        assert!(!is_newer("1.2", "1.10"));
    }

    #[test]
    fn words_permission_warnings() {
        let m: serde_json::Value = serde_json::json!({
            "permissions": ["declarativeNetRequest", "storage", "tabs"],
            "host_permissions": ["<all_urls>"],
        });
        assert_eq!(
            permission_warnings(&m),
            vec!["Read and change all your data on all websites", "Block content on any page", "Read your browsing history"]
        );
        let m = serde_json::json!({ "content_scripts": [{ "matches": ["https://*.github.com/*"] }] });
        assert_eq!(permission_warnings(&m), vec!["Read and change your data on github.com"]);
    }
}
