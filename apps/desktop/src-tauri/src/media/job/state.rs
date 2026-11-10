//! Job state: the control and progress shared between a job thread and the UI, and the list of
//! running jobs with how their state reaches the downloads list (snapshot, emit, settle).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::output::mark_of_the_web;
use super::{lock, Output, Request};
use crate::downloads::DownloadRecord;
use crate::host::downloads::DownloadEvent;
use crate::storage::now_ms;

// ---------------------------------------------------------------------------------------------
// Control and progress (shared between the job thread and the UI)

pub const RUN: u8 = 0;
pub const PAUSE: u8 = 1;
pub const CANCEL: u8 = 2;
/// Finish now and keep what was recorded (live streams).
pub const STOP: u8 = 3;

#[derive(Default)]
pub struct Control {
    state: Mutex<u8>,
    cv: Condvar,
}

impl Control {
    pub fn get(&self) -> u8 {
        *lock(&self.state)
    }
    pub fn set(&self, v: u8) {
        *lock(&self.state) = v;
        self.cv.notify_all();
    }
    /// Changes the state only if it is still `from` (so a Cancel that already arrived wins).
    pub fn set_if(&self, from: u8, to: u8) -> bool {
        let mut g = lock(&self.state);
        if *g != from {
            return false;
        }
        *g = to;
        drop(g);
        self.cv.notify_all();
        true
    }
    /// Sleeps for up to `d`; returns false as soon as the job should stop.
    pub fn sleep(&self, d: Duration) -> bool {
        let g = lock(&self.state);
        let (g, _) = self.cv.wait_timeout_while(g, d, |s| *s == RUN).unwrap_or_else(|e| e.into_inner());
        *g == RUN
    }
    /// Blocks while paused and returns the state that ended the pause.
    pub fn wait_resume(&self) -> u8 {
        let mut g = lock(&self.state);
        while *g == PAUSE {
            g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
        }
        *g
    }
}

#[derive(Default)]
pub struct TrackProgress {
    pub bytes: AtomicU64,
    /// Expected size in bytes (0 = unknown).
    pub total: AtomicU64,
    /// Segments appended.
    pub done: AtomicU64,
    /// Bytes of those appended segments (downloaded-but-waiting ones are only in `bytes`).
    pub appended: AtomicU64,
    /// Segments expected (0 = unknown or live).
    pub count: AtomicU64,
    /// Media time recorded (live streams).
    pub recorded_ms: AtomicU64,
}

#[derive(Default)]
pub struct Progress {
    pub tracks: [TrackProgress; 2],
    pub stage: Mutex<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Summary {
    pub bytes: u64,
    /// Known or estimated total (0 = unknown).
    pub total: u64,
    pub done: u64,
    pub count: u64,
    pub recorded_ms: u64,
}

impl Progress {
    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();
        let mut unknown = false;
        for t in &self.tracks {
            let (bytes, total, done, count) = (
                t.bytes.load(Ordering::Relaxed),
                t.total.load(Ordering::Relaxed),
                t.done.load(Ordering::Relaxed),
                t.count.load(Ordering::Relaxed),
            );
            s.bytes += bytes;
            s.done += done;
            s.count += count;
            s.recorded_ms = s.recorded_ms.max(t.recorded_ms.load(Ordering::Relaxed));
            if total > 0 {
                s.total += total.max(bytes);
            } else if count > 0 && done > 0 {
                // Estimate from the appended segments (the workers run ahead of `done`).
                let appended = t.appended.load(Ordering::Relaxed);
                s.total += ((appended as f64 / done as f64 * count as f64) as u64).max(bytes);
            } else if count > 0 || bytes > 0 {
                unknown = true;
            }
        }
        if unknown {
            s.total = 0;
        }
        s
    }
    pub(super) fn set_stage(&self, text: &str) {
        *lock(&self.stage) = text.to_string();
    }
}

pub(super) struct Running {
    pub(super) record: DownloadRecord,
    pub(super) tab: String,
    pub(super) control: Arc<Control>,
    pub(super) progress: Arc<Progress>,
    pub(super) live: bool,
    pub(super) paused: bool,
    pub(super) reason: String,
    pub(super) note: String,
    pub(super) last_emit: Instant,
    /// A failed job waits for Retry; DRM and setup failures can't be retried.
    pub(super) resumable: bool,
    /// The state last written to the database (it is written on changes only).
    pub(super) persisted: String,
}

pub(super) static JOBS: LazyLock<Mutex<HashMap<i64, Running>>> = LazyLock::new(Mutex::default);

pub(super) fn format_clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub(super) fn snapshot(j: &mut Running) -> DownloadEvent {
    let s = j.progress.summary();
    if j.record.state == "progress" {
        j.record.received = s.bytes as i64;
        j.record.total = s.total as i64;
    }
    let stage = lock(&j.progress.stage).clone();
    let detail = if j.record.state == "completed" {
        j.note.clone()
    } else if j.record.state != "progress" {
        String::new()
    } else if !stage.is_empty() {
        stage
    } else if j.live {
        format!("Recording · {}", format_clock(s.recorded_ms))
    } else if s.count > 1 {
        format!("Segment {} of {}", s.done.min(s.count), s.count)
    } else {
        String::new()
    };
    let file_name = Path::new(&j.record.path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    DownloadEvent {
        record: j.record.clone(),
        file_name,
        paused: j.paused,
        can_resume: j.record.state == "interrupted" && j.resumable,
        tab_id: j.tab.clone(),
        reason: j.reason.clone(),
        kind: "media",
        detail,
        live: j.live,
        origin: None,
        torrent: None,
    }
}

/// Sends a job's state to the UI, and to the database when the state changed. Runs under the
/// JOBS lock, so events leave in the order the job changed and a job removed from the list can't
/// be written back after the removal (both calls only queue work).
fn emit_locked(j: &mut Running) {
    j.last_emit = Instant::now();
    let event = snapshot(j);
    if event.record.state == "completed" {
        crate::downloads::remember_finished(event.record.id, &event.record.path);
    }
    if event.record.state != "progress" && j.persisted != event.record.state {
        j.persisted = event.record.state.clone();
        let record = event.record.clone();
        crate::host::db().exec(move |c| {
            let _ = crate::downloads::download_upsert(c, &record);
        });
    }
    crate::host::emit("download", event);
}

/// `force` publishes now; otherwise this is the progress ticker, which only reports running jobs
/// and at most every 300 ms.
pub(super) fn publish(id: i64, force: bool) {
    let mut jobs = lock(&JOBS);
    let Some(j) = jobs.get_mut(&id) else { return };
    if !force && (j.paused || j.record.state != "progress" || j.last_emit.elapsed() < Duration::from_millis(300)) {
        return;
    }
    emit_locked(j);
}

pub(super) fn update(id: i64, f: impl FnOnce(&mut Running)) {
    if let Some(j) = lock(&JOBS).get_mut(&id) {
        f(j);
    }
}

/// Moves a job to a final or failed state in one step (so the progress ticker can't interleave),
/// publishes it, and forgets it unless it waits for Retry.
pub(super) fn settle(id: i64, state: &str, reason: &str, resumable: bool, patch: impl FnOnce(&mut Running)) {
    let mut jobs = lock(&JOBS);
    let Some(j) = jobs.get_mut(&id) else { return };
    patch(j);
    j.record.state = state.into();
    j.record.ended = Some(now_ms());
    j.reason = reason.into();
    j.paused = false;
    j.resumable = resumable;
    *lock(&j.progress.stage) = String::new();
    emit_locked(j);
    if state != "interrupted" || !resumable {
        jobs.remove(&id);
    }
}

pub(super) fn complete(id: i64, req: &Request, out: Output) {
    for f in &out.files {
        mark_of_the_web(f, &req.source_url, &req.page_url);
    }
    let size = out.files.first().and_then(|f| fs::metadata(f).ok()).map(|m| m.len()).unwrap_or(0) as i64;
    settle(id, "completed", "", false, |j| {
        j.record.path = out.files.first().map(|f| f.display().to_string()).unwrap_or_default();
        j.record.received = size;
        j.record.total = size;
        j.note = out.note;
    });
}
