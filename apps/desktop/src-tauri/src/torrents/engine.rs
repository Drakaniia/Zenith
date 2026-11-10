//! The torrent engine: one librqbit session (DHT, PEX, trackers, local peer discovery, UPnP) on
//! its own Tokio runtime. It starts on first use, or at startup when torrents are waiting to
//! continue, and keeps its state in `torrents/` of the profile's local data folder.

use std::future::Future;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use librqbit::dht::DhtPersistenceConfig;
use librqbit::limits::LimitsConfig;
use librqbit::{DhtSessionConfig, ListenerOptions, Session, SessionOptions, SessionPersistenceConfig};
use tokio::runtime::Runtime;
use tokio::sync::OnceCell;

use crate::host::RuntimeConfig;

static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("zenith-torrent")
        .enable_all()
        .build()
        .expect("torrent runtime")
});

static SESSION: OnceCell<Arc<Session>> = OnceCell::const_new();

/// The torrents folder: Zenith's list, kept .torrent files and the engine's own state.
pub fn folder() -> PathBuf {
    crate::host::paths().local.join("torrents")
}

/// Runs `f` on the engine's runtime (librqbit spawns its tasks there) and waits for it from any
/// thread or runtime.
pub async fn run<T: Send + 'static>(f: impl Future<Output = Result<T, String>> + Send + 'static) -> Result<T, String> {
    RUNTIME.spawn(f).await.map_err(|e| e.to_string())?
}

/// Starts `f` on the engine's runtime without waiting for it.
pub fn spawn(f: impl Future<Output = ()> + Send + 'static) -> tokio::task::AbortHandle {
    RUNTIME.spawn(f).abort_handle()
}

/// The session, started (and its saved torrents restored) on first use. Call on the runtime.
pub async fn session() -> Result<Arc<Session>, String> {
    SESSION
        .get_or_try_init(|| async {
            let dir = folder();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let opts = SessionOptions {
                persistence: Some(SessionPersistenceConfig::Json { folder: Some(dir.join("session")) }),
                fastresume: true,
                dht: Some(DhtSessionConfig {
                    persistence: Some(DhtPersistenceConfig { config_filename: Some(dir.join("dht.json")), ..Default::default() }),
                    ..Default::default()
                }),
                // Accept incoming peers (needed to seed well), opening the port on the router.
                listen: Some(ListenerOptions { enable_upnp_port_forwarding: true, ..Default::default() }),
                ratelimits: limits(&crate::host::config()),
                ..Default::default()
            };
            let session = Session::new_with_opts(crate::media::job::download_dir(), opts).await.map_err(|e| format!("{e:#}"))?;
            super::start_ticker();
            Ok::<_, String>(session)
        })
        .await
        .cloned()
}

/// The session if it is running.
pub fn existing() -> Option<Arc<Session>> {
    SESSION.get().cloned()
}

fn bps(kib: u32) -> Option<NonZeroU32> {
    NonZeroU32::new(kib.saturating_mul(1024))
}

/// Speed limits from settings (KiB/s, 0 for none).
pub fn limits(cfg: &RuntimeConfig) -> LimitsConfig {
    LimitsConfig { download_bps: bps(cfg.torrent_down_kib), upload_bps: bps(cfg.torrent_up_kib) }
}

/// Applies changed speed limits to the running session.
pub fn apply_limits(cfg: &RuntimeConfig) {
    if let Some(s) = existing() {
        let l = limits(cfg);
        if s.ratelimits.get_config() != l {
            s.ratelimits.set_download_bps(l.download_bps);
            s.ratelimits.set_upload_bps(l.upload_bps);
        }
    }
}

/// Lets the engine save its state before Zenith exits (bounded, so quitting never hangs).
pub fn stop() {
    let Some(s) = existing() else { return };
    RUNTIME.block_on(async move {
        let _ = tokio::time::timeout(Duration::from_secs(2), s.stop()).await;
    });
}
