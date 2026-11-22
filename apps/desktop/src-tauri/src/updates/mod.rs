//! App updates. Every release is signed with minisign; the updater refuses anything whose
//! signature does not match Zenith's pinned public key.
//!
//! Flow: checks the release feed, streams a newer package to a staging file on disk, verifies its
//! minisign signature, and launches the quiet per-user installer on exit.

pub mod commands;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

use crate::app::runtime::AppHandle;

const FIRST_CHECK: Duration = Duration::from_secs(45);
const INTERVAL: Duration = Duration::from_secs(6 * 3600);
const DEFAULT_ENDPOINT: &str = "https://github.com/Drakaniia/Zenith/releases/latest/download/latest.json";
const PREVIEW_ENDPOINT: &str = "https://github.com/Drakaniia/Zenith/releases/download/preview/preview.json";
const PUBKEY_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDlEOTU0REY4Njc5NTBGQUQKUldTdEQ1Vm4rRTJWblk4K0Vha3hFR1gxYXZiVmhIZzVpZ3V2NU44NGdCc3Uyajg5NGFpWXdXcFUK";
const TARGET_PLATFORM: &str = "windows-x86_64";
const MAX_FEED_BYTES: usize = 512 * 1024;
const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
const STAGING_DIR: &str = "update-staging";
const ROLLBACK_DIR: &str = "update-rollback";

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// "idle" | "checking" | "downloading" | "ready" | "upToDate" | "error"
    pub state: &'static str,
    pub current_version: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub progress: Option<f64>,
    pub error: Option<String>,
    pub checked: Option<i64>,
}

#[derive(Deserialize)]
struct Feed {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    platforms: HashMap<String, FeedPlatform>,
}

#[derive(Deserialize)]
struct FeedPlatform {
    url: String,
    signature: String,
}

struct Ready {
    version: String,
    installer_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingInstall {
    from_version: String,
    to_version: String,
    installer: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedInstaller {
    version: String,
}

static STATUS: LazyLock<Mutex<Status>> = LazyLock::new(|| Mutex::new(Status { state: "idle", ..Default::default() }));
static READY: Mutex<Option<Ready>> = Mutex::new(None);
static BUSY: AtomicBool = AtomicBool::new(false);
static RESTART: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn set(f: impl FnOnce(&mut Status)) {
    let snapshot = {
        let mut s = lock(&STATUS);
        f(&mut s);
        s.clone()
    };
    crate::host::emit("update-status", snapshot);
}

pub fn status() -> Status {
    let mut s = lock(&STATUS).clone();
    s.current_version = env!("CARGO_PKG_VERSION").to_string();
    s
}

fn feed_endpoints(preview: bool) -> Vec<String> {
    #[cfg(debug_assertions)]
    if let Ok(u) = std::env::var("ZENITH_UPDATE_ENDPOINT") {
        if !u.trim().is_empty() {
            return vec![u];
        }
    }
    if preview {
        vec![PREVIEW_ENDPOINT.to_string(), DEFAULT_ENDPOINT.to_string()]
    } else {
        vec![DEFAULT_ENDPOINT.to_string()]
    }
}

fn parse_version(s: &str) -> Option<semver::Version> {
    semver::Version::parse(s.trim().trim_start_matches('v')).ok()
}

fn pick_best_feed(current: &str, feeds: Vec<Feed>) -> Option<Feed> {
    let cur = parse_version(current)?;
    feeds
        .into_iter()
        .filter_map(|f| {
            let v = parse_version(&f.version)?;
            (v > cur && f.platforms.contains_key(TARGET_PLATFORM)).then_some((v, f))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, f)| f)
}

fn parse_pubkey() -> Result<PublicKey, String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(PUBKEY_B64)
        .map_err(|e| format!("bad updater pubkey encoding: {e}"))?;
    let text = String::from_utf8(decoded).map_err(|e| format!("bad updater pubkey utf8: {e}"))?;
    PublicKey::decode(&text).map_err(|e| format!("bad updater pubkey: {e}"))
}

fn verify_signature(path: &Path, sig_b64: &str) -> Result<(), String> {
    let pk = parse_pubkey()?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(sig_b64.trim())
        .map_err(|e| format!("bad signature encoding: {e}"))?;
    let sig_text = String::from_utf8(decoded).map_err(|e| format!("bad signature utf8: {e}"))?;
    let sig = Signature::decode(&sig_text).map_err(|e| format!("bad signature: {e}"))?;
    let mut verifier = pk.verify_stream(&sig).map_err(|e| format!("signature header mismatch: {e}"))?;
    let mut file = fs::File::open(path).map_err(|e| format!("open staged update: {e}"))?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        use std::io::Read;
        let n = file.read(&mut buf).map_err(|e| format!("read staged update: {e}"))?;
        if n == 0 {
            break;
        }
        verifier.update(&buf[..n]);
    }
    verifier.finalize().map_err(|e| format!("signature verification failed: {e}"))
}

fn is_newer(current: &str, candidate: &str) -> bool {
    parse_version(current)
        .zip(parse_version(candidate))
        .is_some_and(|(cur, cand)| cand > cur)
}

fn sanitize_version_tag(v: &str) -> String {
    v.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect()
}

fn stage_and_verify(feed: Feed) -> Result<Option<Ready>, String> {
    if !is_newer(env!("CARGO_PKG_VERSION"), &feed.version) {
        return Ok(None);
    }
    let Some(plat) = feed.platforms.get(TARGET_PLATFORM) else {
        return Ok(None);
    };
    let version = feed.version.clone();
    set(|s| {
        s.state = "downloading";
        s.version = Some(version.clone());
        s.notes = feed.notes.clone();
        s.progress = Some(0.0);
    });

    let stage_dir = crate::host::paths().local.join(STAGING_DIR);
    fs::create_dir_all(&stage_dir).map_err(|e| format!("create update staging dir: {e}"))?;
    let staged_name = format!("zenith-update-{}.exe", sanitize_version_tag(&version));
    let staged = stage_dir.join(&staged_name);
    let tmp = staged.with_extension("part");

    let client = crate::media::http::Client::with_policy("ZenithUpdater/1", Box::new(|_| true))
        .map_err(|e| e.to_string())?;
    let mut resp = client.get(&plat.url, &|_| Vec::new(), None, false).map_err(|e| format!("download failed: {e}"))?;
    let total = resp.total.unwrap_or(0);
    let mut received = 0u64;
    let mut last_progress = 0.0f64;
    {
        let mut out = fs::File::create(&tmp).map_err(|e| format!("create staged file: {e}"))?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = resp.read(&mut buf).map_err(|e| format!("read update stream: {e}"))?;
            if n == 0 {
                break;
            }
            received += n as u64;
            if received > MAX_PACKAGE_BYTES {
                let _ = fs::remove_file(&tmp);
                return Err("update package exceeds size limit".into());
            }
            out.write_all(&buf[..n]).map_err(|e| format!("write staged file: {e}"))?;
            if total > 0 {
                let p = (received as f64 / total as f64).min(1.0);
                if p - last_progress >= 0.05 {
                    last_progress = p;
                    set(|s| s.progress = Some(p));
                }
            }
        }
        out.sync_all().map_err(|e| format!("sync staged file: {e}"))?;
    }

    if let Err(e) = verify_signature(&tmp, &plat.signature) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, &staged).map_err(|e| format!("finalize staged update: {e}"))?;
    // Remove stale staging artifacts while keeping `current.exe`/`current.json` for rollback.
    if let Ok(entries) = fs::read_dir(&stage_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if s != staged_name && s != "current.exe" && s != "current.json" && s != "pending.json" {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    Ok(Some(Ready { version, installer_path: staged }))
}

pub async fn check(_app: AppHandle) -> Status {
    if lock(&READY).is_some() || BUSY.swap(true, Ordering::SeqCst) {
        return status();
    }
    set(|s| {
        s.state = "checking";
        s.error = None;
    });
    let endpoints = feed_endpoints(crate::host::config().preview_updates);
    let result = crate::app::runtime::spawn_blocking(move || {
        let mut feeds = Vec::new();
        let mut last_err = None;
        for endpoint in endpoints {
            match crate::platform::net::get(&endpoint, MAX_FEED_BYTES, 30_000)
                .and_then(|body| serde_json::from_slice::<Feed>(&body).map_err(|e| format!("invalid update feed: {e}")))
            {
                Ok(feed) => feeds.push(feed),
                Err(e) => last_err = Some(e),
            }
        }
        if feeds.is_empty() {
            return Err(last_err.unwrap_or_else(|| "no update feed reachable".into()));
        }
        match pick_best_feed(env!("CARGO_PKG_VERSION"), feeds) {
            Some(best) => stage_and_verify(best),
            None => Ok(None),
        }
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));

    let now = crate::storage::now_ms();
    match result {
        Ok(Some(ready)) => {
            let version = ready.version.clone();
            *lock(&READY) = Some(ready);
            set(|s| {
                s.state = "ready";
                s.version = Some(version);
                s.progress = Some(1.0);
                s.checked = Some(now);
            });
        }
        Ok(None) => set(|s| {
            s.state = "upToDate";
            s.version = None;
            s.notes = None;
            s.progress = None;
            s.checked = Some(now);
        }),
        Err(e) => {
            eprintln!("[zenith] update check failed: {e}");
            set(|s| {
                s.state = "error";
                s.error = Some(e);
                s.progress = None;
                s.checked = Some(now);
            });
        }
    }
    BUSY.store(false, Ordering::SeqCst);
    status()
}

pub fn start(app: AppHandle) {
    crate::app::runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if crate::host::config().auto_update && lock(&READY).is_none() {
                check(app.clone()).await;
            }
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

pub fn ready() -> bool {
    lock(&READY).is_some()
}

pub fn restart_after_quit() {
    RESTART.store(true, Ordering::SeqCst);
}

/// Installs a verified staged update silently on exit.
pub fn install_on_exit() -> bool {
    let restart = RESTART.load(Ordering::SeqCst);
    if crate::profiles::others_running() {
        return false;
    }
    let Some(ready) = lock(&READY).take() else { return restart };
    crate::host::db().flush(Duration::from_secs(2));
    if let Some(name) = ready.installer_path.file_name().and_then(|n| n.to_str()) {
        let pending = PendingInstall {
            from_version: env!("CARGO_PKG_VERSION").into(),
            to_version: ready.version.clone(),
            installer: name.to_string(),
        };
        if let Ok(bytes) = serde_json::to_vec(&pending) {
            let _ = crate::storage::write_atomic(
                &crate::host::paths().local.join(STAGING_DIR).join("pending.json"),
                &bytes,
            );
        }
    }
    let mut cmd = std::process::Command::new(&ready.installer_path);
    cmd.arg("/S");
    if !restart {
        cmd.arg("/NORUN");
    }
    if let Err(e) = cmd.spawn() {
        eprintln!("[zenith] update install failed: {e}");
    }
    restart
}

/// Called once at startup after runtime-manifest verification succeeds. Rotates the previous
/// verified installer into `update-rollback/` so at most one rollback installer is kept on disk.
pub fn finalize_pending_install(local_dir: &Path, current_version: &str) {
    let stage_dir = local_dir.join(STAGING_DIR);
    let pending_path = stage_dir.join("pending.json");
    let Ok(raw) = fs::read_to_string(&pending_path) else { return };
    let Ok(pending) = serde_json::from_str::<PendingInstall>(&raw) else {
        let _ = fs::remove_file(&pending_path);
        return;
    };
    let _ = fs::remove_file(&pending_path);

    let staged_exe = stage_dir.join(&pending.installer);
    if parse_version(current_version) != parse_version(&pending.to_version) {
        eprintln!(
            "[zenith] pending update to {} did not complete (running {}); keeping current install",
            pending.to_version, current_version
        );
        return;
    }

    let current_exe = stage_dir.join("current.exe");
    let current_meta = stage_dir.join("current.json");
    if current_exe.is_file() {
        let prev_ver = fs::read_to_string(&current_meta)
            .ok()
            .and_then(|s| serde_json::from_str::<SavedInstaller>(&s).ok())
            .map(|m| m.version)
            .unwrap_or_else(|| pending.from_version.clone());
        if parse_version(&prev_ver) != parse_version(current_version) {
            let rb_dir = local_dir.join(ROLLBACK_DIR);
            if fs::create_dir_all(&rb_dir).is_ok() {
                if let Ok(entries) = fs::read_dir(&rb_dir) {
                    for e in entries.flatten() {
                        let _ = fs::remove_file(e.path());
                    }
                }
                let rb_exe = rb_dir.join(format!("zenith-rollback-{}.exe", sanitize_version_tag(&prev_ver)));
                let _ = fs::rename(&current_exe, &rb_exe);
            }
        }
    }

    if staged_exe.is_file() {
        let _ = fs::rename(&staged_exe, &current_exe);
        if let Ok(bytes) = serde_json::to_vec(&SavedInstaller {
            version: current_version.to_string(),
        }) {
            let _ = crate::storage::write_atomic(&current_meta, &bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_feed(version: &str) -> Feed {
        let mut platforms = HashMap::new();
        platforms.insert(
            TARGET_PLATFORM.to_string(),
            FeedPlatform {
                url: format!("https://example.com/{version}.exe"),
                signature: "sig".into(),
            },
        );
        Feed { version: version.into(), notes: None, platforms }
    }

    #[test]
    fn pubkey_decodes_and_version_comparison_works() {
        assert!(parse_pubkey().is_ok());
        assert!(is_newer("0.1.13", "0.1.14"));
        assert!(is_newer("0.1.13", "v0.2.0-preview.1"));
        assert!(is_newer("0.2.0-preview.1", "0.2.0-preview.2"));
        assert!(is_newer("0.2.0-preview.2", "0.2.0"));
        assert!(!is_newer("0.1.13", "0.1.13"));
        assert!(!is_newer("0.1.13", "0.1.12"));
    }

    #[test]
    fn pick_best_feed_prefers_highest_newer_across_preview_and_stable() {
        let best = pick_best_feed("0.1.13", vec![fake_feed("0.2.0-preview.1"), fake_feed("0.1.14")]).unwrap();
        assert_eq!(best.version, "0.2.0-preview.1");
        let promoted = pick_best_feed("0.2.0-preview.1", vec![fake_feed("0.2.0-preview.1"), fake_feed("0.2.0")]).unwrap();
        assert_eq!(promoted.version, "0.2.0");
        assert!(pick_best_feed("0.2.0", vec![fake_feed("0.2.0-preview.9"), fake_feed("0.2.0")]).is_none());
    }

    #[test]
    fn finalize_pending_install_rotates_rollback_and_handles_interrupted_upgrade() {
        let dir = std::env::temp_dir().join(format!("zenith-upd-rb-{}", crate::storage::now_ms()));
        let stage = dir.join(STAGING_DIR);
        fs::create_dir_all(&stage).unwrap();

        // Simulate existing verified v0.1.13 installer and pending upgrade to v0.2.0.
        fs::write(stage.join("current.exe"), b"old-0.1.13").unwrap();
        fs::write(stage.join("current.json"), r#"{"version":"0.1.13"}"#).unwrap();
        fs::write(stage.join("zenith-update-0.2.0.exe"), b"new-0.2.0").unwrap();
        fs::write(
            stage.join("pending.json"),
            r#"{"fromVersion":"0.1.13","toVersion":"0.2.0","installer":"zenith-update-0.2.0.exe"}"#,
        )
        .unwrap();

        // Interrupted upgrade (still on 0.1.13): clears pending.json without clobbering current.
        finalize_pending_install(&dir, "0.1.13");
        assert!(!stage.join("pending.json").exists());
        assert_eq!(fs::read(stage.join("current.exe")).unwrap(), b"old-0.1.13");

        // Completed upgrade to 0.2.0: promotes 0.1.13 to rollback and 0.2.0 to current.
        fs::write(
            stage.join("pending.json"),
            r#"{"fromVersion":"0.1.13","toVersion":"0.2.0","installer":"zenith-update-0.2.0.exe"}"#,
        )
        .unwrap();
        finalize_pending_install(&dir, "0.2.0");
        assert_eq!(fs::read(stage.join("current.exe")).unwrap(), b"new-0.2.0");
        assert_eq!(
            fs::read(dir.join(ROLLBACK_DIR).join("zenith-rollback-0.1.13.exe")).unwrap(),
            b"old-0.1.13"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
