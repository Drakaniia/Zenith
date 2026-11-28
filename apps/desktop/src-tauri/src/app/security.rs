//! Trust boundaries for the CEF trusted UI bridge: browser generation tracking, 256-bit
//! per-launch capability token, strict origin verification, and safe packaged asset resolution.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

pub const TRUSTED_SCHEME: &str = "zenith";
pub const TRUSTED_HOST: &str = "app";
pub const TRUSTED_BASE_URL: &str = "zenith://app/index.html";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserRole {
    TrustedUi { window: String },
    UntrustedPage { tab_id: String, window: String },
}

#[derive(Debug, Clone)]
pub struct BrowserEntry {
    pub role: BrowserRole,
    pub generation: u64,
    pub alive: bool,
}

/// Tracks CEF integer browser IDs with monotonic generations and tombstones so stale callbacks
/// after close or renderer crash can never mutate another tab or invoke privileged commands.
#[derive(Default)]
pub struct GenerationTable {
    next_gen: u64,
    entries: HashMap<i32, BrowserEntry>,
}

impl GenerationTable {
    pub fn register(&mut self, browser_id: i32, role: BrowserRole) -> u64 {
        self.next_gen += 1;
        let generation = self.next_gen;
        self.entries.insert(
            browser_id,
            BrowserEntry { role, generation, alive: true },
        );
        generation
    }

    pub fn bump_generation(&mut self, browser_id: i32) -> Option<u64> {
        let entry = self.entries.get_mut(&browser_id)?;
        self.next_gen += 1;
        entry.generation = self.next_gen;
        Some(entry.generation)
    }

    pub fn tombstone(&mut self, browser_id: i32) {
        if let Some(entry) = self.entries.get_mut(&browser_id) {
            entry.alive = false;
        }
    }

    pub fn is_live_generation(&self, browser_id: i32, generation: u64) -> bool {
        self.entries
            .get(&browser_id)
            .is_some_and(|e| e.alive && e.generation == generation)
    }

    pub fn role(&self, browser_id: i32) -> Option<&BrowserRole> {
        self.entries.get(&browser_id).filter(|e| e.alive).map(|e| &e.role)
    }

    pub fn trusted_window(&self, browser_id: i32) -> Option<&str> {
        match self.role(browser_id) {
            Some(BrowserRole::TrustedUi { window }) => Some(window.as_str()),
            _ => None,
        }
    }
}

/// Per-launch random 256-bit capability token injected only into TrustedUi main frames.
#[derive(Clone)]
pub struct BridgeToken(String);

impl BridgeToken {
    pub fn generate() -> Result<Self, String> {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).map_err(|e| format!("token entropy failed: {e}"))?;
        let mut hex = String::with_capacity(64);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(&mut hex, "{b:02x}");
        }
        Ok(Self(hex))
    }

    #[cfg(test)]
    pub fn from_raw(s: &str) -> Self {
        Self(s.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Constant-time comparison to avoid leaking token prefix timing across IPC calls.
    pub fn verify(&self, candidate: &str) -> bool {
        let a = self.0.as_bytes();
        let b = candidate.as_bytes();
        if a.len() != b.len() {
            return false;
        }
        let mut diff = 0u8;
        for (&x, &y) in a.iter().zip(b.iter()) {
            diff |= x ^ y;
        }
        diff == 0
    }
}

/// Strictly validates that a URL belongs to the trusted `zenith://app/` origin (or dev Vite
/// loopback origin when `ZENITH_DEV_URL` is explicitly enabled in debug builds).
pub fn is_trusted_origin(url: &str) -> bool {
    let trimmed = url.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return false;
    };
    if scheme.eq_ignore_ascii_case(TRUSTED_SCHEME) {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if authority.contains('@') || authority.contains(':') {
            return false;
        }
        return authority.eq_ignore_ascii_case(TRUSTED_HOST);
    }
    #[cfg(debug_assertions)]
    if let Ok(dev) = std::env::var("ZENITH_DEV_URL") {
        if let (Ok(u), Ok(d)) = (url::Url::parse(trimmed), url::Url::parse(dev.trim())) {
            if u.origin() == d.origin() {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, PartialEq, Eq)]
pub enum TrustError {
    UntrustedBrowser,
    SubframeDenied,
    UntrustedOrigin,
    InvalidToken,
}

impl TrustError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UntrustedBrowser => "untrusted_browser",
            Self::SubframeDenied => "subframe_denied",
            Self::UntrustedOrigin => "untrusted_origin",
            Self::InvalidToken => "invalid_token",
        }
    }
}

/// Authorizes every privileged bridge invocation on the browser process side and returns the
/// owning trusted window label.
pub fn authorize_invoke<'a>(
    table: &'a GenerationTable,
    browser_id: i32,
    is_main_frame: bool,
    frame_url: &str,
    expected_token: &BridgeToken,
    provided_token: &str,
) -> Result<&'a str, TrustError> {
    let Some(window) = table.trusted_window(browser_id) else {
        return Err(TrustError::UntrustedBrowser);
    };
    if !is_main_frame {
        return Err(TrustError::SubframeDenied);
    }
    if !is_trusted_origin(frame_url) {
        return Err(TrustError::UntrustedOrigin);
    }
    if !expected_token.verify(provided_token) {
        return Err(TrustError::InvalidToken);
    }
    Ok(window)
}

/// Resolves a `zenith://app/<path>` request inside `ui_dir`, rejecting path traversal attempts.
pub fn resolve_asset_path(ui_dir: &Path, url: &str) -> Option<PathBuf> {
    if !is_trusted_origin(url) {
        return None;
    }
    let rest = url.split_once("://")?.1;
    let raw_path = rest.split_once('/').map(|(_, p)| p).unwrap_or("");
    let clean = raw_path.split(['?', '#']).next().unwrap_or("");
    let rel = if clean.is_empty() { "index.html" } else { clean };
    let mut out = ui_dir.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(seg) => out.push(seg),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.is_dir() {
        out.push("index.html");
    }
    Some(out)
}

#[derive(serde::Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, String>,
}

/// Verifies `runtime-manifest.json` next to the executable when present. Fails closed on corrupt
/// JSON, path traversal entries, missing runtime files, or SHA-256 mismatches.
pub fn verify_runtime_manifest(exe_dir: &Path) -> Result<(), String> {
    let manifest_path = exe_dir.join("runtime-manifest.json");
    if !manifest_path.is_file() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read runtime-manifest.json: {e}"))?;
    let manifest: RuntimeManifest = serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("parse runtime-manifest.json: {e}"))?;
    if manifest.files.is_empty() {
        return Err("runtime-manifest.json has no file entries".into());
    }
    let mut entries = Vec::with_capacity(manifest.files.len());
    for (rel, expected_hex) in &manifest.files {
        let mut target = exe_dir.to_path_buf();
        let mut segments = 0usize;
        for comp in Path::new(rel).components() {
            match comp {
                Component::Normal(seg) => {
                    target.push(seg);
                    segments += 1;
                }
                Component::CurDir => {}
                _ => return Err(format!("invalid manifest path: {rel}")),
            }
        }
        if segments == 0 {
            return Err(format!("invalid manifest path: {rel}"));
        }
        entries.push((target, rel.as_str(), expected_hex.trim()));
    }

    let check_one = |(target, rel, expected_hex): &(PathBuf, &str, &str)| -> Result<(), String> {
        let actual_hex = sha256_file(target)
            .map_err(|e| format!("missing or unreadable runtime file {rel}: {e}"))?;
        if !actual_hex.eq_ignore_ascii_case(expected_hex) {
            return Err(format!("runtime file hash mismatch: {rel}"));
        }
        Ok(())
    };

    if entries.len() <= 2 {
        for item in &entries {
            check_one(item)?;
        }
        return Ok(());
    }

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8)
        .min(entries.len());
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let first_err: Mutex<Option<String>> = Mutex::new(None);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while !failed.load(Ordering::Relaxed) {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= entries.len() {
                        break;
                    }
                    if let Err(e) = check_one(&entries[i]) {
                        failed.store(true, Ordering::Relaxed);
                        if let Ok(mut slot) = first_err.lock() {
                            if slot.is_none() {
                                *slot = Some(e);
                            }
                        }
                        break;
                    }
                }
            });
        }
    });
    if let Some(err) = first_err.into_inner().ok().flatten() {
        return Err(err);
    }
    Ok(())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{b:02x}");
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_origin_validation_is_strict() {
        assert!(is_trusted_origin("zenith://app/"));
        assert!(is_trusted_origin("zenith://app/index.html"));
        assert!(is_trusted_origin("ZENITH://APP/assets/app.js?v=1"));
        assert!(!is_trusted_origin("zenith://evil/index.html"));
        assert!(!is_trusted_origin("zenith://app@evil/index.html"));
        assert!(!is_trusted_origin("zenith://app:8080/index.html"));
        assert!(!is_trusted_origin("https://example.com/"));
        assert!(!is_trusted_origin("about:blank"));
    }

    #[test]
    fn authorize_invoke_enforces_all_boundaries() {
        let mut table = GenerationTable::default();
        let g1 = table.register(1, BrowserRole::TrustedUi { window: "main".into() });
        table.register(2, BrowserRole::UntrustedPage { tab_id: "t1".into(), window: "main".into() });
        let tok = BridgeToken::from_raw("secret-token");

        assert_eq!(authorize_invoke(&table, 1, true, TRUSTED_BASE_URL, &tok, "secret-token"), Ok("main"));
        assert_eq!(
            authorize_invoke(&table, 2, true, TRUSTED_BASE_URL, &tok, "secret-token"),
            Err(TrustError::UntrustedBrowser)
        );
        assert_eq!(
            authorize_invoke(&table, 1, false, TRUSTED_BASE_URL, &tok, "secret-token"),
            Err(TrustError::SubframeDenied)
        );
        assert_eq!(
            authorize_invoke(&table, 1, true, "https://example.com/", &tok, "secret-token"),
            Err(TrustError::UntrustedOrigin)
        );
        assert_eq!(
            authorize_invoke(&table, 1, true, TRUSTED_BASE_URL, &tok, "wrong-token"),
            Err(TrustError::InvalidToken)
        );

        let g2 = table.bump_generation(1).unwrap();
        assert!(!table.is_live_generation(1, g1));
        assert!(table.is_live_generation(1, g2));

        table.tombstone(1);
        assert!(!table.is_live_generation(1, g2));
        assert_eq!(
            authorize_invoke(&table, 1, true, TRUSTED_BASE_URL, &tok, "secret-token"),
            Err(TrustError::UntrustedBrowser)
        );
    }

    #[test]
    fn asset_resolver_blocks_path_traversal() {
        let root = Path::new("C:\\ui");
        assert_eq!(resolve_asset_path(root, "zenith://app/"), Some(PathBuf::from("C:\\ui\\index.html")));
        assert_eq!(resolve_asset_path(root, "zenith://app/../../Windows/win.ini"), None);
        assert_eq!(resolve_asset_path(root, "https://evil.com/index.html"), None);
    }

    #[test]
    fn runtime_manifest_verifies_hashes_and_rejects_tampering() {
        let dir = std::env::temp_dir().join(format!("zenith-rt-manifest-{}", crate::storage::now_ms()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("locales")).unwrap();
        // Absent manifest is allowed (unbundled dev/test runs).
        assert!(verify_runtime_manifest(&dir).is_ok());

        let file = dir.join("locales").join("en-US.pak");
        std::fs::write(&file, b"hello-cef").unwrap();
        let hash = sha256_file(&file).unwrap();
        std::fs::write(
            dir.join("runtime-manifest.json"),
            format!(r#"{{"files":{{"locales/en-US.pak":"{hash}"}}}}"#),
        )
        .unwrap();
        assert!(verify_runtime_manifest(&dir).is_ok());

        std::fs::write(
            dir.join("runtime-manifest.json"),
            format!("\u{feff}{{\"files\":{{\"locales/en-US.pak\":\"{hash}\"}}}}"),
        )
        .unwrap();
        assert!(verify_runtime_manifest(&dir).is_ok());

        // Tampering fails closed.
        std::fs::write(&file, b"tampered").unwrap();
        assert!(verify_runtime_manifest(&dir).unwrap_err().contains("hash mismatch"));

        // Path traversal fails closed.
        std::fs::write(
            dir.join("runtime-manifest.json"),
            format!(r#"{{"files":{{"../outside.dll":"{hash}"}}}}"#),
        )
        .unwrap();
        assert!(verify_runtime_manifest(&dir).unwrap_err().contains("invalid manifest path"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runtime_manifest_parallel_verification_catches_tampering() {
        let dir = std::env::temp_dir().join(format!("zenith-rt-par-{}", crate::storage::now_ms()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let mut files = serde_json::Map::new();
        for i in 0..10 {
            let rel = format!("bin/part-{i}.pak");
            let p = dir.join(&rel);
            let payload = format!("chunk-payload-{i}");
            std::fs::write(&p, payload.as_bytes()).unwrap();
            files.insert(rel, serde_json::Value::String(sha256_file(&p).unwrap()));
        }
        let manifest = serde_json::json!({ "files": files });
        std::fs::write(dir.join("runtime-manifest.json"), manifest.to_string()).unwrap();
        assert!(verify_runtime_manifest(&dir).is_ok());

        std::fs::write(dir.join("bin/part-7.pak"), b"corrupted").unwrap();
        let err = verify_runtime_manifest(&dir).unwrap_err();
        assert!(err.contains("hash mismatch") && err.contains("bin/part-7.pak"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
