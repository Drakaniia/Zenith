//! App updates. Releases are published on GitHub (see docs/RELEASING.md); every update is signed,
//! and the updater refuses anything whose signature does not match the public key in
//! tauri.conf.json.
//!
//! Flow: a while after startup and then every few hours, Zenith checks the release feed. A newer
//! version is downloaded in the background and kept in memory. It is installed silently (the
//! NSIS installer in quiet mode, per-user, no prompts) when the browser quits, or right away when
//! the user picks "Restart to update" (the installer then relaunches Zenith, and session restore
//! brings the tabs back). Nothing interrupts browsing.

pub mod commands;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

/// First check after startup (startup stays fast and the network quiet while the session loads).
const FIRST_CHECK: Duration = Duration::from_secs(45);
/// Later checks.
const INTERVAL: Duration = Duration::from_secs(6 * 3600);

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// "idle" | "checking" | "downloading" | "ready" | "upToDate" | "error"
    pub state: &'static str,
    pub current_version: String,
    /// The version found (downloading or ready).
    pub version: Option<String>,
    pub notes: Option<String>,
    /// Download progress 0..1 (when the size is known).
    pub progress: Option<f64>,
    pub error: Option<String>,
    /// When the last check finished (ms since the epoch).
    pub checked: Option<i64>,
}

struct Ready {
    update: Update,
    bytes: Vec<u8>,
}

static STATUS: LazyLock<Mutex<Status>> = LazyLock::new(|| Mutex::new(Status { state: "idle", ..Default::default() }));
static READY: Mutex<Option<Ready>> = Mutex::new(None);
static BUSY: AtomicBool = AtomicBool::new(false);
/// "Restart to update" was chosen: the installer relaunches Zenith afterwards.
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

/// The updater, pointed at a test feed when `ZENITH_UPDATE_ENDPOINT` is set in a debug build
/// (used by the end-to-end test; release builds always use the configured feed).
fn updater(app: &AppHandle) -> tauri_plugin_updater::Result<tauri_plugin_updater::Updater> {
    let builder = app.updater_builder().timeout(Duration::from_secs(120));
    #[cfg(debug_assertions)]
    let builder = match std::env::var("ZENITH_UPDATE_ENDPOINT").ok().and_then(|u| tauri::Url::parse(&u).ok()) {
        Some(url) => builder.endpoints(vec![url])?,
        None => builder,
    };
    builder.build()
}

/// Checks the release feed and downloads a newer version in the background. Returns the status
/// afterwards; concurrent calls return the current status.
pub async fn check(app: AppHandle) -> Status {
    if lock(&READY).is_some() || BUSY.swap(true, Ordering::SeqCst) {
        return status();
    }
    set(|s| {
        s.state = "checking";
        s.error = None;
    });
    let result = async {
        let Some(update) = updater(&app)?.check().await? else { return Ok(None) };
        let version = update.version.clone();
        set(|s| {
            s.state = "downloading";
            s.version = Some(version.clone());
            s.notes = update.body.clone();
            s.progress = Some(0.0);
        });
        let mut received = 0u64;
        let mut last = 0.0;
        let bytes = update
            .download(
                |chunk, total| {
                    received += chunk as u64;
                    if let Some(total) = total.filter(|t| *t > 0) {
                        let p = (received as f64 / total as f64).min(1.0);
                        if p - last >= 0.05 {
                            last = p;
                            set(|s| s.progress = Some(p));
                        }
                    }
                },
                || {},
            )
            .await?;
        // The signature is checked again by `install`; checking now means a bad download is
        // reported (and retried at the next check) instead of failing at quit.
        update.verify(&bytes)?;
        Ok::<_, tauri_plugin_updater::Error>(Some(Ready { update, bytes }))
    }
    .await;
    let now = crate::storage::now_ms();
    match result {
        Ok(Some(ready)) => {
            let version = ready.update.version.clone();
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
                s.error = Some(e.to_string());
                s.progress = None;
                s.checked = Some(now);
            });
        }
    }
    BUSY.store(false, Ordering::SeqCst);
    status()
}

/// Background checks: shortly after startup, then every few hours, while automatic updates are on.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
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

/// Marks the coming quit as "restart into the new version".
pub fn restart_after_quit() {
    RESTART.store(true, Ordering::SeqCst);
}

/// Called last on quit (UI thread, session saved, webviews closed). Installs a downloaded update
/// silently; the installer exits this process. Returns normally only when there is nothing to
/// install (or the installer could not be started), and then says whether a restart was asked for.
pub fn install_on_exit() -> bool {
    let restart = RESTART.load(Ordering::SeqCst);
    // The installer closes every running Zenith. While another profile is open, leave the update
    // for the last profile to quit (its own updater downloads it again).
    if crate::profiles::others_running() {
        return false;
    }
    let Some(ready) = lock(&READY).take() else { return restart };
    crate::host::db().flush(Duration::from_secs(2));
    let update = ready.update.restart_after_install(restart);
    if let Err(e) = update.install(&ready.bytes) {
        eprintln!("[zenith] update install failed: {e}");
    }
    restart
}
