//! What can start a torrent: magnet links, .torrent files and the command-line arguments Windows
//! passes for them, and the names torrents get on disk.

use std::path::Path;

/// A `magnet:` link (scheme matched case-insensitively, as browsers do).
pub fn is_magnet(s: &str) -> bool {
    s.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("magnet:"))
}

/// The BitTorrent v1 info hash of a magnet link, lowercase hex.
pub fn magnet_hash(uri: &str) -> Option<String> {
    if !is_magnet(uri) {
        return None;
    }
    librqbit::Magnet::parse(uri).ok()?.as_id20().map(|h| h.as_string())
}

/// The display name (`dn`) of a magnet link.
pub fn magnet_name(uri: &str) -> Option<String> {
    librqbit::Magnet::parse(uri).ok()?.name.filter(|n| !n.trim().is_empty())
}

/// A magnet link for a torrent known by its hash (shown as the download's address).
pub fn magnet_for(hash: &str, name: &str) -> String {
    let mut uri = format!("magnet:?xt=urn:btih:{hash}");
    if !name.is_empty() {
        uri.push_str("&dn=");
        for b in name.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => uri.push(b as char),
                _ => uri.push_str(&format!("%{b:02X}")),
            }
        }
    }
    uri
}

/// A .torrent download: by type, saved file name or address.
pub fn is_torrent_file(path: &str, mime: &str) -> bool {
    mime.eq_ignore_ascii_case("application/x-bittorrent") || path.to_ascii_lowercase().ends_with(".torrent")
}

/// The info hash and name inside a .torrent file.
pub fn torrent_file_info(bytes: &[u8]) -> Result<(String, String), String> {
    let meta = librqbit::torrent_from_bytes(bytes).map_err(|_| "This isn't a valid .torrent file.".to_string())?;
    let name = meta
        .info
        .data
        .name
        .as_ref()
        .map(|n| String::from_utf8_lossy(n.as_ref()).into_owned())
        .unwrap_or_default();
    Ok((meta.info_hash.as_string(), name))
}

/// Command-line arguments that are torrents: magnet links and existing .torrent files (what
/// Windows passes when Zenith opens them).
pub fn torrent_args(argv: &[String]) -> Vec<String> {
    argv.iter()
        .skip(1)
        .map(|a| a.trim().trim_matches('"'))
        .filter(|a| is_magnet(a) || (a.to_ascii_lowercase().ends_with(".torrent") && Path::new(a).is_absolute() && Path::new(a).is_file()))
        .map(str::to_string)
        .collect()
}

/// A torrent's name as a Windows folder name: path separators, reserved characters and names,
/// and trailing dots or spaces removed.
pub fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit());
    match cleaned.as_str() {
        "" | "." | ".." => "Torrent".into(),
        _ if reserved => format!("_{cleaned}"),
        _ => cleaned.chars().take(150).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "cab507494d02ebb1178b38f2e9d7be299c86b862";

    #[test]
    fn magnets() {
        let uri = format!("MAGNET:?xt=urn:btih:{HASH}&dn=Some%20Thing&tr=udp%3A%2F%2Ftracker.example%3A80");
        assert!(is_magnet(&uri));
        assert!(!is_magnet("magnet"));
        assert!(!is_magnet("https://example.com/a.torrent"));
        assert_eq!(magnet_hash(&format!("magnet:?xt=urn:btih:{HASH}")).as_deref(), Some(HASH));
        assert_eq!(magnet_hash("magnet:?dn=nothing"), None);
        assert_eq!(magnet_hash("https://x"), None);
        assert_eq!(magnet_name(&format!("magnet:?xt=urn:btih:{HASH}&dn=Some%20Thing")).as_deref(), Some("Some Thing"));
        let built = magnet_for(HASH, "A b/c");
        assert_eq!(built, format!("magnet:?xt=urn:btih:{HASH}&dn=A%20b%2Fc"));
        assert_eq!(magnet_hash(&built).as_deref(), Some(HASH));
    }

    #[test]
    fn torrent_files() {
        assert!(is_torrent_file("C:\\x\\Movie.TORRENT", ""));
        assert!(is_torrent_file("C:\\x\\download", "application/x-bittorrent"));
        assert!(!is_torrent_file("C:\\x\\movie.mp4", "video/mp4"));
        assert!(torrent_file_info(b"not bencode").is_err());
        // d4:infod6:lengthi5e4:name5:a.txt12:piece lengthi16384e6:pieces20:<20 bytes>ee
        let mut t = b"d4:infod6:lengthi5e4:name5:a.txt12:piece lengthi16384e6:pieces20:".to_vec();
        t.extend_from_slice(&[7u8; 20]);
        t.extend_from_slice(b"ee");
        let (hash, name) = torrent_file_info(&t).unwrap();
        assert_eq!((hash.len(), name.as_str()), (40, "a.txt"));
    }

    #[test]
    fn args() {
        let argv: Vec<String> = ["zenith.exe", &format!("magnet:?xt=urn:btih:{HASH}"), "https://example.com", "C:\\missing\\x.torrent", "--flag"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(torrent_args(&argv), vec![format!("magnet:?xt=urn:btih:{HASH}")]);
    }

    #[test]
    fn folder_names() {
        assert_eq!(folder_name("My: Show / S01"), "My_ Show _ S01");
        assert_eq!(folder_name("  name.  "), "name");
        assert_eq!(folder_name(".."), "Torrent");
        assert_eq!(folder_name(""), "Torrent");
        assert_eq!(folder_name("con"), "_con");
        assert_eq!(folder_name("COM1.txt"), "_COM1.txt");
        assert_eq!(folder_name("Console"), "Console");
        assert_eq!(folder_name(&"x".repeat(300)).len(), 150);
    }
}
