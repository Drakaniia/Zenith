//! Local privacy-preserving Safe Browsing, certificate error gating, external protocol
//! validation, dangerous download classification, and Windows Mark of the Web (Zone.Identifier).

pub mod commands;
pub mod protect;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, RwLock};

use cef::*;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::host::downloads::Started;
use crate::host::tab::TabShared;
use crate::platform::net;
use crate::platform::urls::host_of;
use crate::storage::{now_ms, write_atomic};

pub static NEXT_SECURITY_REQ: AtomicU32 = AtomicU32::new(1);

const DB_MAGIC: &[u8; 4] = b"ZSB1";
const MAX_FEED_BYTES: usize = 8 * 1024 * 1024;
const MAX_PREFIXES: usize = 500_000;

/// Built-in deterministic test threat indicators (never make network requests).
const BUILTIN_THREAT_HOSTS: &[&str] = &[
    "malware.testing.zenith.invalid",
    "phishing.testing.zenith.invalid",
];
const BUILTIN_THREAT_URLS: &[&str] = &[
    "https://testsafebrowsing.appspot.com/s/malware.html",
    "https://testsafebrowsing.appspot.com/s/phishing.html",
];

/// Open, hash-checked threat feeds fetched only on user/scheduled update; browsed URLs never leave
/// the device.
pub const THREAT_FEEDS: &[(&str, &str)] = &[
    ("urlhaus", "https://urlhaus.abuse.ch/downloads/hostfile/"),
    ("openphish", "https://openphish.com/feed.txt"),
];

pub static SAFE_BROWSING: LazyLock<RwLock<SafeBrowsing>> =
    LazyLock::new(|| RwLock::new(SafeBrowsing::with_builtin()));

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SafeBrowsingStatus {
    pub enabled: bool,
    pub entries: usize,
    pub updated: Option<i64>,
    pub feeds: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatHit {
    pub category: &'static str,
    pub host: String,
}

pub struct SafeBrowsing {
    /// Sorted 64-bit SHA-256 prefixes of blocked hostnames and canonical URLs.
    prefixes: Vec<u64>,
    /// Session-only explicit user overrides: `(tab_id, host_or_url)`.
    bypasses: HashSet<(String, String)>,
    pub updated: Option<i64>,
    pub enabled: bool,
}

impl SafeBrowsing {
    pub fn with_builtin() -> Self {
        let mut prefixes = Vec::with_capacity(16);
        for h in BUILTIN_THREAT_HOSTS {
            prefixes.push(hash_prefix(&format!("host:{h}")));
        }
        for u in BUILTIN_THREAT_URLS {
            prefixes.push(hash_prefix(&format!("url:{}", canonical_url(u))));
        }
        prefixes.sort_unstable();
        prefixes.dedup();
        Self { prefixes, bypasses: HashSet::new(), updated: None, enabled: true }
    }

    pub fn load_cache(&mut self, path: &Path) -> Result<usize, String> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(self.prefixes.len()),
            Err(e) => return Err(e.to_string()),
        };
        let parsed = decode_db(&bytes)?;
        let mut merged = Self::with_builtin().prefixes;
        merged.extend(parsed);
        merged.sort_unstable();
        merged.dedup();
        merged.truncate(MAX_PREFIXES);
        self.prefixes = merged;
        self.updated = fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);
        Ok(self.prefixes.len())
    }

    pub fn ingest_feeds(&mut self, texts: &[&str], cache_path: Option<&Path>) -> usize {
        let mut merged = Self::with_builtin().prefixes;
        for text in texts {
            for line in text.lines().take(MAX_PREFIXES) {
                if let Some(p) = parse_threat_line(line) {
                    merged.push(p);
                }
            }
        }
        merged.sort_unstable();
        merged.dedup();
        merged.truncate(MAX_PREFIXES);
        self.prefixes = merged;
        self.updated = Some(now_ms());
        if let Some(path) = cache_path {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = write_atomic(path, &encode_db(&self.prefixes));
        }
        self.prefixes.len()
    }

    pub fn allow_bypass(&mut self, tab_id: &str, url: &str) {
        let key = host_of(url).unwrap_or(url).to_ascii_lowercase();
        self.bypasses.insert((tab_id.to_string(), key));
    }

    pub fn drop_tab(&mut self, tab_id: &str) {
        self.bypasses.retain(|(t, _)| t != tab_id);
    }

    /// Checks a URL offline against the local hash-prefix database. Zero network traffic.
    pub fn check(&self, tab_id: Option<&str>, url: &str) -> Option<ThreatHit> {
        if !self.enabled {
            return None;
        }
        let host = host_of(url)?.to_ascii_lowercase();
        if let Some(tid) = tab_id {
            if self.bypasses.contains(&(tid.to_string(), host.clone())) {
                return None;
            }
        }
        let url_key = hash_prefix(&format!("url:{}", canonical_url(url)));
        if self.prefixes.binary_search(&url_key).is_ok() {
            return Some(ThreatHit { category: "Deceptive or malicious page", host });
        }
        let mut h = host.as_str();
        loop {
            let key = hash_prefix(&format!("host:{h}"));
            if self.prefixes.binary_search(&key).is_ok() {
                return Some(ThreatHit { category: "Known malware or phishing host", host });
            }
            match h.find('.') {
                Some(i) => h = &h[i + 1..],
                None => return None,
            }
        }
    }

    pub fn len(&self) -> usize {
        self.prefixes.len()
    }
}

pub fn cache_path() -> PathBuf {
    crate::host::persistent_paths().local.join("security").join("threats.bin")
}

pub fn init_safebrowsing() {
    let path = cache_path();
    if let Ok(mut sb) = SAFE_BROWSING.write() {
        if let Err(e) = sb.load_cache(&path) {
            // Corrupt DB fails open cleanly (Section 17.3).
            eprintln!("[zenith] safe browsing cache unreadable ({e}); using built-in set");
        }
    }
}

pub fn update_feeds() -> Result<SafeBrowsingStatus, String> {
    let mut bodies = Vec::new();
    let mut errors = Vec::new();
    for (name, url) in THREAT_FEEDS {
        match net::get(url, MAX_FEED_BYTES, 15_000) {
            Ok(bytes) => {
                if let Ok(text) = String::from_utf8(bytes) {
                    bodies.push(text);
                }
            }
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    if bodies.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let path = cache_path();
    let (entries, updated, enabled) = SAFE_BROWSING
        .write()
        .map(|mut sb| {
            let n = sb.ingest_feeds(&refs, Some(&path));
            (n, sb.updated, sb.enabled)
        })
        .map_err(|_| "safe browsing lock poisoned".to_string())?;
    Ok(SafeBrowsingStatus {
        enabled,
        entries,
        updated,
        feeds: THREAT_FEEDS.iter().map(|(n, _)| (*n).to_string()).collect(),
    })
}

fn hash_prefix(input: &str) -> u64 {
    let digest = Sha256::digest(input.as_bytes());
    u64::from_be_bytes(digest[..8].try_into().unwrap_or([0; 8]))
}

fn canonical_url(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_frag = trimmed.split('#').next().unwrap_or(trimmed);
    without_frag.to_ascii_lowercase()
}

fn parse_threat_line(raw: &str) -> Option<u64> {
    let line = raw.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('!') || line.len() > 1024 {
        return None;
    }
    if line.starts_with("http://") || line.starts_with("https://") {
        return Some(hash_prefix(&format!("url:{}", canonical_url(line))));
    }
    let mut parts = line.split_whitespace();
    if let (Some(ip), Some(host)) = (parts.next(), parts.next()) {
        if matches!(ip, "0.0.0.0" | "127.0.0.1" | "::" | "::1") {
            let h = host.trim_end_matches('.').to_ascii_lowercase();
            if h.contains('.') && !matches!(h.as_str(), "localhost" | "localhost.localdomain") {
                return Some(hash_prefix(&format!("host:{h}")));
            }
        }
    }
    None
}

fn encode_db(prefixes: &[u64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + prefixes.len() * 8);
    out.extend_from_slice(DB_MAGIC);
    out.extend_from_slice(&(prefixes.len() as u32).to_le_bytes());
    for &p in prefixes {
        out.extend_from_slice(&p.to_le_bytes());
    }
    out
}

fn decode_db(bytes: &[u8]) -> Result<Vec<u64>, String> {
    if bytes.len() < 8 || &bytes[..4] != DB_MAGIC {
        return Err("invalid safe browsing database header".into());
    }
    let count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if count > MAX_PREFIXES || bytes.len() != 8 + count * 8 {
        return Err("corrupt or truncated safe browsing database".into());
    }
    let mut out = Vec::with_capacity(count);
    for chunk in bytes[8..].chunks_exact(8) {
        out.push(u64::from_le_bytes(chunk.try_into().unwrap()));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Dangerous download classification & Mark of the Web (Zone.Identifier ADS)

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadRisk {
    pub reason: String,
    pub dangerous: bool,
}

const DANGEROUS_EXTS: &[&str] = &[
    "exe", "msi", "msp", "bat", "cmd", "ps1", "psm1", "vbs", "vbe", "js", "jse", "wsf", "wsh",
    "scr", "lnk", "hta", "reg", "dll", "com", "pif", "jar", "msc", "cpl", "inf", "iso", "img",
];

pub fn classify_download(file_name: &str, url: &str) -> Option<DownloadRisk> {
    if let Ok(sb) = SAFE_BROWSING.read() {
        if let Some(hit) = sb.check(None, url) {
            return Some(DownloadRisk {
                reason: format!("Blocked by Safe Browsing ({})", hit.category),
                dangerous: true,
            });
        }
    }
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())?;
    if DANGEROUS_EXTS.contains(&ext.as_str()) {
        return Some(DownloadRisk {
            reason: format!("Executable or script file (.{ext}) can harm your device"),
            dangerous: true,
        });
    }
    None
}

/// Writes the Windows Mark of the Web (`Zone.Identifier` NTFS alternate data stream) onto a
/// downloaded file so Windows SmartScreen/Explorer treat Rust-managed downloads identically to
/// Chromium downloads.
pub fn write_motw(path: &Path, source_url: &str) {
    let mut ads = path.as_os_str().to_os_string();
    ads.push(":Zone.Identifier");
    let clean_url: String = source_url
        .chars()
        .filter(|c| !c.is_control())
        .take(2048)
        .collect();
    let content = if clean_url.starts_with("http://") || clean_url.starts_with("https://") {
        format!("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl={clean_url}\r\n")
    } else {
        "[ZoneTransfer]\r\nZoneId=3\r\n".to_string()
    };
    let _ = fs::write(PathBuf::from(ads), content);
}

// ---------------------------------------------------------------------------------------------
// External protocol validation

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolDecision {
    Internal,
    Reject,
    Prompt { scheme: String, url: String },
}

pub fn validate_external_protocol(raw: &str) -> ProtocolDecision {
    let url = raw.trim();
    if url.is_empty() || url.len() > 2048 || url.chars().any(|c| c.is_control()) {
        return ProtocolDecision::Reject;
    }
    let Some((scheme_raw, rest)) = url.split_once(':') else {
        return ProtocolDecision::Reject;
    };
    if scheme_raw.is_empty() || scheme_raw.len() > 32 {
        return ProtocolDecision::Reject;
    }
    let mut chars = scheme_raw.chars();
    let valid_scheme = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid_scheme {
        return ProtocolDecision::Reject;
    }
    let scheme = scheme_raw.to_ascii_lowercase();
    if matches!(
        scheme.as_str(),
        "http" | "https" | "about" | "blob" | "data" | "chrome" | "chrome-extension"
            | "devtools" | "zenith" | "view-source" | "file" | "magnet" | "ws" | "wss"
    ) {
        return ProtocolDecision::Internal;
    }
    if matches!(scheme.as_str(), "javascript" | "vbscript" | "jscript" | "livescript" | "mocha")
        || rest.is_empty()
    {
        return ProtocolDecision::Reject;
    }
    ProtocolDecision::Prompt { scheme, url: url.to_string() }
}

// ---------------------------------------------------------------------------------------------
// Pending security callbacks (deny/cancel safely on drop or tab close)

pub struct PendingCertError {
    pub tab: String,
    pub host: String,
    pub url: String,
    callback: Option<Callback>,
}

impl PendingCertError {
    pub fn new(tab: String, host: String, url: String, cb: Callback) -> Self {
        Self { tab, host, url, callback: Some(cb) }
    }

    pub fn respond(mut self, proceed: bool) {
        if let Some(cb) = self.callback.take() {
            if proceed {
                cb.cont();
            } else {
                cb.cancel();
            }
        }
    }
}

impl Drop for PendingCertError {
    fn drop(&mut self) {
        if let Some(cb) = self.callback.take() {
            cb.cancel();
        }
    }
}

pub struct PendingProtocol {
    pub tab: String,
    pub scheme: String,
    pub url: String,
}

pub struct PendingDownloadConfirm {
    pub tab: String,
    pub shared: std::sync::Arc<TabShared>,
    pub(crate) started: Started,
    callback: Option<BeforeDownloadCallback>,
}

impl PendingDownloadConfirm {
    pub(crate) fn new(
        tab: String,
        shared: std::sync::Arc<TabShared>,
        started: Started,
        cb: BeforeDownloadCallback,
    ) -> Self {
        Self { tab, shared, started, callback: Some(cb) }
    }

    pub fn respond(mut self, allow: bool) {
        let Some(cb) = self.callback.take() else { return };
        if allow {
            let path = self.started.path.clone();
            let _ = crate::host::downloads::register_download(self.started, &self.shared);
            cb.cont(Some(&CefString::from(path.as_str())), 0);
        }
        // Dropping `cb` without calling `cont` cancels the CEF download safely.
    }
}

/// Evaluates external protocol and Safe Browsing checks for a main-frame navigation.
/// Returns `true` if navigation must be cancelled (rejected, prompted, or blocked).
pub fn intercept_navigation(tab_id: &str, uri: &str) -> bool {
    match validate_external_protocol(uri) {
        ProtocolDecision::Internal => {}
        ProtocolDecision::Reject => return true,
        ProtocolDecision::Prompt { scheme, url } => {
            let req_id = NEXT_SECURITY_REQ.fetch_add(1, Ordering::Relaxed);
            let tab = tab_id.to_string();
            let pending = PendingProtocol { tab: tab.clone(), scheme: scheme.clone(), url: url.clone() };
            crate::host::with_host(move |h| {
                h.protocol_prompts.insert(req_id, pending);
            });
            crate::host::emit_tab(
                &tab,
                "external-protocol",
                serde_json::json!({ "requestId": req_id, "tabId": tab, "scheme": scheme, "url": url }),
            );
            return true;
        }
    }
    if uri.starts_with("http://") || uri.starts_with("https://") {
        if let Some(hit) = SAFE_BROWSING.read().ok().and_then(|sb| sb.check(Some(tab_id), uri)) {
            crate::host::emit_tab(
                tab_id,
                "safebrowsing-block",
                serde_json::json!({ "tabId": tab_id, "url": uri, "host": hit.host, "category": hit.category }),
            );
            return true;
        }
    }
    false
}

/// Holds a CEF certificate error callback and emits a deny-by-default interstitial prompt.
pub fn queue_cert_error(tab_id: &str, cert_error: Errorcode, url: String, cb: Callback) -> bool {
    let host = host_of(&url).unwrap_or("").to_string();
    let req_id = NEXT_SECURITY_REQ.fetch_add(1, Ordering::Relaxed);
    let tab = tab_id.to_string();
    let pending = PendingCertError::new(tab.clone(), host.clone(), url.clone(), cb);
    if crate::host::with_host(move |h| {
        h.cert_errors.insert(req_id, pending);
    })
    .is_some()
    {
        crate::host::emit_tab(
            &tab,
            "cert-error",
            serde_json::json!({
                "requestId": req_id,
                "tabId": tab,
                "host": host,
                "url": url,
                "error": format!("{cert_error:?}"),
            }),
        );
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safebrowsing_matches_builtin_and_ingested_feeds_offline() {
        let mut sb = SafeBrowsing::with_builtin();
        assert!(sb.check(Some("t1"), "https://malware.testing.zenith.invalid/dl").is_some());
        assert!(sb.check(Some("t1"), "https://testsafebrowsing.appspot.com/s/phishing.html").is_some());
        assert!(sb.check(Some("t1"), "https://example.com/").is_none());

        sb.allow_bypass("t1", "https://malware.testing.zenith.invalid/dl");
        assert!(sb.check(Some("t1"), "https://malware.testing.zenith.invalid/dl").is_none());
        assert!(sb.check(Some("t2"), "https://malware.testing.zenith.invalid/dl").is_some());

        let feed = "# comment\n0.0.0.0 evil.example.test\nhttps://phish.example.org/login\n";
        sb.ingest_feeds(&[feed], None);
        assert!(sb.check(None, "https://sub.evil.example.test/a").is_some());
        assert!(sb.check(None, "https://phish.example.org/login#frag").is_some());
        assert!(sb.check(None, "https://phish.example.org/other").is_none());
    }

    #[test]
    fn corrupt_safebrowsing_db_fails_cleanly() {
        assert!(decode_db(b"BAD!").is_err());
        assert!(decode_db(&[b'Z', b'S', b'B', b'1', 5, 0, 0, 0]).is_err());
        let encoded = encode_db(&[42, 99]);
        assert_eq!(decode_db(&encoded).unwrap(), vec![42, 99]);
    }

    #[test]
    fn dangerous_downloads_and_external_protocols_are_classified() {
        assert!(classify_download("setup.exe", "https://cdn.example/setup.exe").is_some());
        assert!(classify_download("script.PS1", "https://cdn.example/a").is_some());
        assert!(classify_download("notes.pdf", "https://cdn.example/notes.pdf").is_none());
        assert!(classify_download("safe.txt", "https://malware.testing.zenith.invalid/a.txt").is_some());

        assert_eq!(validate_external_protocol("https://example.com"), ProtocolDecision::Internal);
        assert_eq!(validate_external_protocol("magnet:?xt=urn:btih:abc"), ProtocolDecision::Internal);
        assert_eq!(validate_external_protocol("javascript:alert(1)"), ProtocolDecision::Reject);
        assert_eq!(validate_external_protocol("mailto:a@b.com\r\nevil"), ProtocolDecision::Reject);
        assert!(matches!(
            validate_external_protocol("mailto:user@example.com"),
            ProtocolDecision::Prompt { scheme, .. } if scheme == "mailto"
        ));
    }

    #[test]
    fn motw_writes_zone_identifier_and_strips_control_chars() {
        let dir = std::env::temp_dir().join(format!("zenith-motw-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("sample.bin");
        fs::write(&file, b"hello").unwrap();
        write_motw(&file, "https://example.com/a\r\nZoneId=0");
        let mut ads = file.as_os_str().to_os_string();
        ads.push(":Zone.Identifier");
        let text = fs::read_to_string(PathBuf::from(ads)).unwrap();
        assert!(text.contains("ZoneId=3"));
        assert!(!text.contains("\r\nZoneId=0"));
        let _ = fs::remove_dir_all(&dir);
    }
}
