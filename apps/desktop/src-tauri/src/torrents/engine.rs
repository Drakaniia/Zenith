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
            let opts = options(&dir, limits(&crate::host::config()));
            let session = Session::new_with_opts(crate::media::job::download_dir(), opts).await.map_err(|e| format!("{e:#}"))?;
            super::start_ticker();
            Ok::<_, String>(session)
        })
        .await
        .cloned()
}

/// The engine's options, keeping its state in `dir`.
fn options(dir: &std::path::Path, ratelimits: LimitsConfig) -> SessionOptions {
    SessionOptions {
        persistence: Some(SessionPersistenceConfig::Json { folder: Some(dir.join("session")) }),
        fastresume: true,
        dht: Some(DhtSessionConfig {
            persistence: Some(DhtPersistenceConfig { config_filename: Some(dir.join("dht.json")), ..Default::default() }),
            ..Default::default()
        }),
        // Accept incoming peers (needed to seed well), opening the port on the router.
        listen: Some(ListenerOptions { enable_upnp_port_forwarding: true, ..Default::default() }),
        ratelimits,
        ..Default::default()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use librqbit::{AddTorrent, AddTorrentOptions};

    /// Live network check: the engine, set up as in the app, gets a popular magnet's file list.
    /// Run with `cargo test --lib torrents::engine -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs the internet"]
    fn magnet_metadata_live() {
        let dir = std::env::temp_dir().join(format!("zenith-torrent-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let result = RUNTIME.block_on(async {
            let session = Session::new_with_opts(dir.join("out"), options(&dir, LimitsConfig::default())).await.unwrap();
            println!("listen {:?}, announce port {:?}", session.listen_addr(), session.announce_port());
            let probe = session.clone();
            let stats = RUNTIME.spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    println!("dht {:?}", probe.get_dht().map(|d| d.stats()));
                }
            });
            // Sintel (a widely seeded Blender film), no trackers: only DHT can find peers.
            let magnet = "magnet:?xt=urn:btih:08ada5a7a6183aae1e09d831df6748d566095a10&dn=Sintel";
            let add = session.add_torrent(AddTorrent::from_url(magnet), Some(AddTorrentOptions { list_only: true, ..Default::default() }));
            let r = tokio::time::timeout(Duration::from_secs(90), add).await;
            stats.abort();
            r
        });
        let _ = std::fs::remove_dir_all(&dir);
        let listed = result.expect("no file list within 90 s").expect("add failed");
        assert!(listed.into_handle().is_none(), "list_only gives a file list");
    }
}
