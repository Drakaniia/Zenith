//! Torrent and magnet downloads, shown in the regular downloads list (kind "torrent").
//!
//! A torrent first gets its file list (from peers for a magnet link), may wait for the user to
//! choose files (`adding`), then downloads (each file roughly in order, so it can be played while
//! it downloads, see `stream`), seeds until the ratio limit and stays in the list as an ordinary
//! finished download (`list`). Torrents continue after a restart (`store` plus the engine's own
//! state); the engine itself is in `engine`.

mod adding;
pub mod commands;
mod engine;
mod list;
mod source;
mod status;
mod store;
mod stream;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, Once};

use librqbit::api::TorrentIdOrHash;
use librqbit::{ManagedTorrent, TorrentStatsState};
use serde::Serialize;

use crate::host::downloads::{DownloadEvent, Origin};
pub use adding::{add, add_file, choose_files, files, open_args, stream_url};
use adding::{begin, stream_target};
pub use engine::apply_limits;
use list::{publish, seed_ratio, send, snapshot, start_ticker};
pub use source::{is_magnet, is_torrent_file, torrent_args};
pub use status::TorrentView;
use status::{Stage, UploadCounter};
use store::Saved;

type Handle = Arc<ManagedTorrent>;

const MIME: &str = "application/x-bittorrent";

/// What starts a torrent.
pub enum Input {
    Magnet(String),
    File(Vec<u8>),
}

/// One file of a torrent, for the file picker.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFile {
    pub index: usize,
    pub name: String,
    pub size: u64,
    pub done: u64,
    pub selected: bool,
}

struct Entry {
    saved: Saved,
    /// The engine's torrent, once it was added.
    handle: Option<Handle>,
    /// The stage before the engine has the torrent (getting its info, waiting for the file
    /// choice, failed to start).
    early: Option<Stage>,
    reason: String,
    /// (path, size) of every file, once known.
    files: Vec<(String, u64)>,
    /// Peers found while getting a magnet link's info, to start with.
    peers: Vec<SocketAddr>,
    task: Option<tokio::task::AbortHandle>,
    counter: UploadCounter,
    tab: String,
    origin: Option<Origin>,
    /// The last event sent (events are only sent when something changed).
    last: Option<serde_json::Value>,
    /// The state last written to the download history.
    persisted: String,
}

impl Entry {
    fn new(saved: Saved, tab: &str, origin: Option<Origin>) -> Entry {
        Entry {
            saved,
            handle: None,
            early: Some(Stage::Metadata),
            reason: String::new(),
            files: Vec::new(),
            peers: Vec::new(),
            task: None,
            counter: UploadCounter::default(),
            tab: tab.to_string(),
            origin,
            last: None,
            persisted: String::new(),
        }
    }

    /// Finished (seeding) or failed: not downloading, so "Remove from list" applies.
    fn removable(&self) -> bool {
        self.saved.completed.is_some()
            || self.early == Some(Stage::Error)
            || self.handle.as_ref().is_some_and(|h| matches!(h.stats().state, TorrentStatsState::Error))
    }
}

static ENTRIES: LazyLock<Mutex<HashMap<i64, Entry>>> = LazyLock::new(Mutex::default);
/// The list changed in a way worth saving (upload totals); saved every few seconds.
static DIRTY: AtomicBool = AtomicBool::new(false);
static TICKER: Once = Once::new();

fn lock() -> MutexGuard<'static, HashMap<i64, Entry>> {
    ENTRIES.lock().unwrap_or_else(|e| e.into_inner())
}

fn update(id: i64, f: impl FnOnce(&mut Entry)) {
    if let Some(e) = lock().get_mut(&id) {
        f(e);
    }
}

fn save_list() {
    let list: Vec<Saved> = lock().values().map(|e| e.saved.clone()).collect();
    if let Err(e) = store::save(&engine::folder(), &list) {
        eprintln!("[zenith] could not save the torrent list: {e}");
    }
}

/// Torrent support is on in settings (magnet links, .torrent downloads and adding torrents).
pub fn enabled() -> bool {
    crate::host::config().torrents_enabled
}

/// A magnet link a page opened (a click, or a link to a new window): Zenith downloads it instead
/// of handing it to Windows. True when it was taken. Call on the UI thread.
pub fn open_from_page(uri: &str, tab: &str, private: bool, window: &str) -> bool {
    if !is_magnet(uri) || !enabled() {
        return false;
    }
    let origin = crate::host::downloads::pointer_in(window);
    if let Err(e) = add(Input::Magnet(uri.to_string()), tab, private, origin) {
        crate::host::emit_tab(tab, "torrent-error", e);
    }
    true
}

/// True when `id` is a torrent.
pub fn owns(id: i64) -> bool {
    lock().contains_key(&id)
}

/// pause / resume / cancel / stop (seeding) from the downloads UI.
pub fn action(id: i64, action: &str) -> Result<(), String> {
    let handle = lock().get(&id).ok_or("download is not active")?.handle.clone();
    match (action, handle) {
        ("pause", Some(h)) => {
            engine::spawn(async move {
                if let Ok(s) = engine::session().await {
                    let _ = s.pause(&h).await;
                }
                publish(id);
            });
        }
        ("pause", None) => return Err("This torrent can't be paused until it starts downloading.".into()),
        ("resume", Some(h)) => {
            engine::spawn(async move {
                if let Ok(s) = engine::session().await {
                    if let Err(e) = s.unpause(&h).await {
                        crate::host::emit("torrent-error", format!("{e:#}"));
                    }
                }
                publish(id);
            });
        }
        // Retry a torrent that failed to start.
        ("resume", None) => begin(id),
        ("cancel", _) => cancel(id),
        ("stop", _) => finish(id, true),
        (other, _) => return Err(format!("unknown download action {other}")),
    }
    Ok(())
}

/// Stops a torrent and deletes what it downloaded.
fn cancel(id: i64) {
    let Some(mut e) = lock().remove(&id) else { return };
    if let Some(t) = e.task.take() {
        t.abort();
    }
    let (mut event, _) = snapshot(id, &mut e, seed_ratio());
    event.record.state = "canceled".into();
    event.record.ended = Some(crate::storage::now_ms());
    event.torrent = None;
    send(&mut e, event);
    remove_from_engine(e, true);
}

/// Stops seeding (the ratio limit was reached, or the user stopped it): the torrent leaves the
/// engine, its files stay, and it remains in the list as a finished download.
fn finish(id: i64, by_user: bool) {
    let Some(mut e) = lock().remove(&id) else { return };
    let (mut event, stage) = snapshot(id, &mut e, seed_ratio());
    if !matches!(stage, Stage::Seeding | Stage::Done) {
        // Only finished torrents stop seeding; anything else stays as it was.
        lock().insert(id, e);
        if by_user {
            return crate::host::emit("torrent-error", "Only a finished torrent can stop seeding.".to_string());
        }
        return;
    }
    if let Some(t) = e.task.take() {
        t.abort();
    }
    if let Some(v) = event.torrent.as_mut() {
        *v = TorrentView { stage: Some(Stage::Done), uploaded: v.uploaded, ratio: v.ratio, ratio_limit: v.ratio_limit, files: v.files, selected: v.selected, ..Default::default() };
    }
    send(&mut e, event);
    remove_from_engine(e, false);
}

fn remove_from_engine(e: Entry, delete_files: bool) {
    save_list();
    store::forget_torrent_file(&engine::folder(), &e.saved.hash);
    let Some(h) = e.handle else { return };
    engine::spawn(async move {
        if let Some(s) = engine::existing() {
            let _ = s.delete(TorrentIdOrHash::Id(h.id()), delete_files).await;
        }
    });
}

/// The user removed torrents from the list (one, or every one that is not downloading): finished
/// ones stop seeding and failed ones are dropped, keeping their files.
pub fn forget(id: Option<i64>) {
    let ids: Vec<i64> = lock().iter().filter(|(k, e)| id.is_none_or(|id| id == **k) && e.removable()).map(|(k, _)| *k).collect();
    for id in ids {
        if let Some(e) = lock().remove(&id) {
            if let Some(t) = &e.task {
                t.abort();
            }
            remove_from_engine(e, false);
        }
    }
}

/// Snapshots of every torrent, for the downloads page.
pub fn active() -> Vec<DownloadEvent> {
    let limit = seed_ratio();
    let mut entries = lock();
    entries.iter_mut().map(|(&id, e)| snapshot(id, e, limit).0).collect()
}

/// Saves upload totals and lets the engine save its state (Zenith is quitting).
pub fn shutdown() {
    if !lock().is_empty() {
        save_list();
    }
    engine::stop();
}

/// Continues the torrents of the last run (called at startup).
pub fn restore() {
    let list = store::load(&engine::folder());
    if list.is_empty() {
        return;
    }
    let ids: Vec<i64> = {
        let mut entries = lock();
        list.into_iter()
            .map(|s| {
                let id = s.id;
                entries.insert(id, Entry::new(s, "", None));
                id
            })
            .collect()
    };
    for id in ids {
        begin(id);
    }
}
