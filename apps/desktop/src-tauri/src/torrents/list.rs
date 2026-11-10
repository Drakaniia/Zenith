//! Torrents in the downloads list: their events (with the torrent extras), the history records of
//! finished ones, and the once-a-second ticker that keeps both current and stops seeding at the
//! ratio limit.

use std::sync::atomic::Ordering;
use std::time::Duration;

use librqbit::{TorrentStats, TorrentStatsState};

use super::status::{self, Sample, Stage, TorrentView};
use super::store::Saved;
use super::{engine, finish, is_magnet, lock, save_list, source, Entry, DIRTY, MIME, TICKER};
use crate::downloads::DownloadRecord;
use crate::host::downloads::DownloadEvent;
use crate::storage::now_ms;

pub(super) fn display_name(s: &Saved) -> String {
    if !s.name.is_empty() {
        return s.name.clone();
    }
    format!("Torrent {}", &s.hash[..8.min(s.hash.len())])
}

fn sample_of(t: &TorrentStats) -> Sample {
    let live = t.live.as_ref();
    let down_bps = live.map(|l| l.download_speed.as_bytes()).unwrap_or(0);
    let left = t.total_bytes.saturating_sub(t.progress_bytes);
    Sample {
        initializing: matches!(t.state, TorrentStatsState::Initializing { .. }),
        paused: matches!(t.state, TorrentStatsState::Paused | TorrentStatsState::Initializing { paused: true }),
        error: t.error.clone().filter(|_| matches!(t.state, TorrentStatsState::Error)),
        finished: t.finished,
        have: t.progress_bytes,
        total: t.total_bytes,
        down_bps,
        up_bps: live.map(|l| l.upload_speed.as_bytes()).unwrap_or(0),
        eta_secs: (down_bps > 0 && left > 0).then(|| left / down_bps),
        peers: live.map(|l| l.snapshot.peer_stats.live).unwrap_or(0),
        seeders: live.map(|l| l.snapshot.peer_stats.live_seeders).unwrap_or(0),
    }
}

/// The ratio at which seeding stops, from settings (0: no seeding).
pub(super) fn seed_ratio() -> f64 {
    let r = crate::host::config().torrent_seed_ratio;
    if r.is_finite() {
        r.max(0.0)
    } else {
        1.0
    }
}

/// The torrent's downloads-list event and stage. Also counts what it uploaded.
pub(super) fn snapshot(id: i64, e: &mut Entry, limit: f64) -> (DownloadEvent, Stage) {
    let stats = e.handle.as_ref().map(|h| h.stats());
    if let Some(t) = &stats {
        let before = e.saved.uploaded;
        e.counter.add(t.uploaded_bytes, &mut e.saved.uploaded);
        if before != e.saved.uploaded {
            DIRTY.store(true, Ordering::Relaxed);
        }
    }
    let sample = stats.as_ref().map(sample_of).unwrap_or_default();
    let stage = match (e.early, &stats) {
        (Some(stage), _) => stage,
        (None, Some(_)) => status::stage_of(&sample, false),
        // Files chosen, handing over to the engine.
        (None, None) => Stage::Checking,
    };
    let chosen = |i: &usize| e.saved.only_files.as_ref().is_none_or(|o| o.contains(i));
    let total = if stats.is_some() { sample.total } else { e.files.iter().enumerate().filter(|(i, _)| chosen(i)).map(|(_, f)| f.1).sum() };
    let state = status::list_state(stage);
    let view = TorrentView {
        stage: Some(stage),
        peers: sample.peers,
        seeders: sample.seeders,
        down_speed: sample.down_bps,
        up_speed: sample.up_bps,
        eta: sample.eta_secs,
        uploaded: e.saved.uploaded,
        ratio: status::ratio(e.saved.uploaded, sample.have),
        ratio_limit: limit,
        files: e.files.len() as u32,
        selected: e.saved.only_files.as_ref().map_or(e.files.len(), Vec::len) as u32,
        streamable: e.handle.is_some() && !e.files.is_empty(),
    };
    let record = DownloadRecord {
        id,
        url: if is_magnet(&e.saved.source) { e.saved.source.clone() } else { source::magnet_for(&e.saved.hash, &e.saved.name) },
        path: e.saved.path.clone(),
        mime: MIME.into(),
        total: total as i64,
        received: sample.have as i64,
        state: state.into(),
        started: e.saved.started,
        ended: if state == "completed" { e.saved.completed } else { None },
    };
    let reason = match stage {
        Stage::Error => sample.error.clone().unwrap_or_else(|| e.reason.clone()),
        _ => String::new(),
    };
    let event = DownloadEvent {
        record,
        file_name: display_name(&e.saved),
        paused: stage == Stage::Paused,
        can_resume: stage == Stage::Error,
        tab_id: e.tab.clone(),
        reason,
        kind: "torrent",
        detail: status::detail(stage).to_string(),
        live: false,
        origin: e.origin.take(),
        torrent: Some(view),
    };
    (event, stage)
}

/// Sends an event to the UI, and its record to the history when the list state changed.
pub(super) fn send(e: &mut Entry, event: DownloadEvent) {
    let state = event.record.state.clone();
    if state == "completed" {
        crate::downloads::remember_finished(event.record.id, &event.record.path);
    }
    if state != "progress" && e.persisted != state && !e.saved.private {
        e.persisted = state;
        let record = event.record.clone();
        crate::host::db().exec(move |c| {
            let _ = crate::downloads::download_upsert(c, &record);
        });
    }
    e.last = serde_json::to_value(&event).ok();
    crate::host::emit("download", event);
}

pub(super) fn publish(id: i64) {
    let limit = seed_ratio();
    let mut entries = lock();
    let Some(e) = entries.get_mut(&id) else { return };
    let (event, _) = snapshot(id, e, limit);
    send(e, event);
}

pub(super) fn start_ticker() {
    TICKER.call_once(|| {
        engine::spawn(async {
            let mut n = 0u32;
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                tick();
                n = n.wrapping_add(1);
                if n % 15 == 0 && DIRTY.swap(false, Ordering::Relaxed) {
                    save_list();
                }
            }
        });
    });
}

/// Once a second: progress to the UI, finished downloads to the history, and seeding stopped at
/// the ratio limit.
fn tick() {
    let limit = seed_ratio();
    let mut done_seeding = Vec::new();
    let mut finished_now = false;
    {
        let mut entries = lock();
        for (&id, e) in entries.iter_mut() {
            let (event, stage) = snapshot(id, e, limit);
            if matches!(stage, Stage::Seeding | Stage::Done) && e.saved.completed.is_none() {
                e.saved.completed = Some(now_ms());
                finished_now = true;
            }
            let have = event.record.received.max(0) as u64;
            if stage == Stage::Done || (stage == Stage::Seeding && status::seeding_done(true, e.saved.uploaded, have, limit)) {
                done_seeding.push(id);
            }
            if serde_json::to_value(&event).ok() != e.last {
                send(e, event);
            }
        }
    }
    if finished_now {
        save_list();
    }
    for id in done_seeding {
        finish(id, false);
    }
}
