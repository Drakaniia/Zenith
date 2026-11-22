//! Trust boundaries, browser generation tracking, asset path safety, and Windows single-instance lock.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

pub const TRUSTED_SCHEME: &str = "zenith";
pub const TRUSTED_HOST: &str = "app";
pub const TRUSTED_BASE_URL: &str = "zenith://app/index.html";

pub const ALLOWED_COMMANDS: &[&str] = &[
    "ping",
    "get_engine_info",
    "echo",
    "trigger_event",
    "probe_complete",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserRole {
    TrustedUi,
    UntrustedPage { tab_id: String },
}

#[derive(Debug, Clone)]
pub struct BrowserEntry {
    pub role: BrowserRole,
    pub generation: u64,
    pub alive: bool,
}

/// Tracks CEF integer browser IDs with monotonic generations and tombstones so callbacks from
/// closed or crashed browsers can never mutate the wrong tab or invoke privileged commands.
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
            BrowserEntry {
                role,
                generation,
                alive: true,
            },
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
        self.entries
            .get(&browser_id)
            .filter(|e| e.alive)
            .map(|e| &e.role)
    }

    pub fn is_trusted_browser(&self, browser_id: i32) -> bool {
        matches!(self.role(browser_id), Some(BrowserRole::TrustedUi))
    }
}

/// Per-launch random capability token injected only into the trusted UI browser.
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

/// Strictly validates that a URL belongs to the trusted `zenith://app/` origin.
pub fn is_trusted_origin(url: &str) -> bool {
    let trimmed = url.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case(TRUSTED_SCHEME) {
        return false;
    }
    let authority = rest.split(&['/', '?', '#'][..]).next().unwrap_or("");
    if authority.contains('@') || authority.contains(':') {
        return false;
    }
    authority.eq_ignore_ascii_case(TRUSTED_HOST)
}

#[derive(Debug, PartialEq, Eq)]
pub enum TrustError {
    UntrustedBrowser,
    SubframeDenied,
    UntrustedOrigin,
    InvalidToken,
    UnknownCommand,
}

impl TrustError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UntrustedBrowser => "untrusted_browser",
            Self::SubframeDenied => "subframe_denied",
            Self::UntrustedOrigin => "untrusted_origin",
            Self::InvalidToken => "invalid_token",
            Self::UnknownCommand => "unknown_command",
        }
    }
}

/// Authorizes every privileged bridge invocation on the browser process side.
pub fn authorize_invoke(
    table: &GenerationTable,
    browser_id: i32,
    is_main_frame: bool,
    frame_url: &str,
    expected_token: &BridgeToken,
    provided_token: &str,
    command: &str,
) -> Result<(), TrustError> {
    if !table.is_trusted_browser(browser_id) {
        return Err(TrustError::UntrustedBrowser);
    }
    if !is_main_frame {
        return Err(TrustError::SubframeDenied);
    }
    if !is_trusted_origin(frame_url) {
        return Err(TrustError::UntrustedOrigin);
    }
    if !expected_token.verify(provided_token) {
        return Err(TrustError::InvalidToken);
    }
    if !ALLOWED_COMMANDS.contains(&command) {
        return Err(TrustError::UnknownCommand);
    }
    Ok(())
}

/// Resolves a `zenith://app/<path>` request inside `ui_dir`, rejecting traversal attempts.
pub fn resolve_asset_path(ui_dir: &Path, url: &str) -> Option<PathBuf> {
    if !is_trusted_origin(url) {
        return None;
    }
    let rest = url.split_once("://")?.1;
    let raw_path = rest.split_once('/').map(|(_, p)| p).unwrap_or("");
    let clean = raw_path.split(&['?', '#'][..]).next().unwrap_or("");
    let rel = if clean.is_empty() {
        "index.html"
    } else {
        clean
    };
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

/// Per-profile Windows named mutex proving single-instance ownership without Tauri plugins.
pub struct SingleInstanceLock {
    handle: HANDLE,
}

impl SingleInstanceLock {
    pub fn acquire(profile_dir: &Path) -> Result<Option<Self>, String> {
        let key = profile_mutex_name(profile_dir);
        let wide = HSTRING::from(key);
        let handle = unsafe { CreateMutexW(None, false, &wide) }
            .map_err(|e| format!("CreateMutexW failed: {e}"))?;
        let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if already {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Ok(None);
        }
        Ok(Some(Self { handle }))
    }
}

impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

fn profile_mutex_name(profile_dir: &Path) -> String {
    // FNV-1a 64-bit hash keeps the mutex name valid regardless of path separators or length.
    let normalized = profile_dir.to_string_lossy().to_ascii_lowercase();
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in normalized.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("Local\\ZenithCefPhase1_{hash:016x}")
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
        assert!(!is_trusted_origin("http://127.0.0.1:8080/"));
        assert!(!is_trusted_origin("https://example.com/"));
        assert!(!is_trusted_origin("about:blank"));
        assert!(!is_trusted_origin("javascript:alert(1)"));
    }

    #[test]
    fn authorize_invoke_enforces_all_boundaries() {
        let mut table = GenerationTable::default();
        let g1 = table.register(1, BrowserRole::TrustedUi);
        table.register(2, BrowserRole::UntrustedPage { tab_id: "t1".into() });
        let tok = BridgeToken::from_raw("secret-token");

        assert!(authorize_invoke(&table, 1, true, TRUSTED_BASE_URL, &tok, "secret-token", "ping").is_ok());
        assert_eq!(
            authorize_invoke(&table, 2, true, TRUSTED_BASE_URL, &tok, "secret-token", "ping"),
            Err(TrustError::UntrustedBrowser)
        );
        assert_eq!(
            authorize_invoke(&table, 1, false, TRUSTED_BASE_URL, &tok, "secret-token", "ping"),
            Err(TrustError::SubframeDenied)
        );
        assert_eq!(
            authorize_invoke(&table, 1, true, "https://example.com/", &tok, "secret-token", "ping"),
            Err(TrustError::UntrustedOrigin)
        );
        assert_eq!(
            authorize_invoke(&table, 1, true, TRUSTED_BASE_URL, &tok, "wrong-token", "ping"),
            Err(TrustError::InvalidToken)
        );
        assert_eq!(
            authorize_invoke(&table, 1, true, TRUSTED_BASE_URL, &tok, "secret-token", "rm_rf"),
            Err(TrustError::UnknownCommand)
        );

        let g2 = table.bump_generation(1).unwrap();
        assert!(!table.is_live_generation(1, g1));
        assert!(table.is_live_generation(1, g2));

        table.tombstone(1);
        assert!(!table.is_live_generation(1, g2));
        assert_eq!(
            authorize_invoke(&table, 1, true, TRUSTED_BASE_URL, &tok, "secret-token", "ping"),
            Err(TrustError::UntrustedBrowser)
        );
    }

    #[test]
    fn asset_resolver_blocks_path_traversal() {
        let root = Path::new("C:\\ui");
        assert_eq!(
            resolve_asset_path(root, "zenith://app/"),
            Some(PathBuf::from("C:\\ui\\index.html"))
        );
        assert_eq!(
            resolve_asset_path(root, "zenith://app/assets/main.js?v=2"),
            Some(PathBuf::from("C:\\ui\\assets\\main.js"))
        );
        assert_eq!(resolve_asset_path(root, "zenith://app/../../Windows/win.ini"), None);
        assert_eq!(resolve_asset_path(root, "https://evil.com/index.html"), None);
    }

    #[test]
    fn single_instance_lock_blocks_second_holder_on_same_profile() {
        let p1 = Path::new("C:\\tmp\\zenith-phase1-unit-lock-a");
        let p2 = Path::new("C:\\tmp\\zenith-phase1-unit-lock-b");
        let hold = SingleInstanceLock::acquire(p1).unwrap();
        assert!(hold.is_some());
        assert!(SingleInstanceLock::acquire(p1).unwrap().is_none());
        assert!(SingleInstanceLock::acquire(p2).unwrap().is_some());
        drop(hold);
        assert!(SingleInstanceLock::acquire(p1).unwrap().is_some());
    }
}
