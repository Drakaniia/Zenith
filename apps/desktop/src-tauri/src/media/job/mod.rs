//! Media download jobs. Each job runs on its own thread: it resolves the stream (HLS playlists,
//! DASH segment lists, plain files), downloads segments in parallel, appends them in order,
//! merges separate audio and video, and reports progress into the regular downloads list.
//!
//! The engine (`Job`, `Ctx`) has no dependency on the browser; the runner at the bottom connects
//! it to the downloads UI, the cookie store and the settings.

mod fetch;
mod output;
mod record;
mod state;
#[cfg(test)]
mod tests;
mod track;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::dash::SegRef;
use super::http::{self, Client};

pub use fetch::{Ctx, Env, Halt};
pub use state::{Control, Progress, CANCEL, PAUSE, RUN, STOP};
use record::record;
use state::{complete, publish, settle, snapshot, update, Running, JOBS};
use track::{download, Track};

pub const PROTECTED: &str = "This video is DRM-protected, so it can't be saved";
const RETRIES: u32 = 5;
const PARALLEL: usize = 6;
/// Consecutive playlist or segment failures after which a live recording counts as ended.
const LIVE_FAILURES: u32 = 6;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------------------------
// What to download

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// An HLS media playlist (re-read while recording a live stream).
    Hls(String),
    /// Explicit segments (DASH SegmentTemplate/SegmentList).
    Segments { init: Option<SegRef>, media: Vec<SegRef>, ext: String },
    /// One whole file (progressive media, DASH SegmentBase).
    File { url: String, ext: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub main: Source,
    /// Separate audio played together with `main`.
    pub audio: Option<Source>,
    /// Record until stopped instead of downloading a fixed list.
    pub live: bool,
}

/// Files produced by a finished job: the first is the main result.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub files: Vec<PathBuf>,
    pub note: String,
}

pub struct Job {
    pub plan: Plan,
    tracks: Vec<Track>,
}

impl Job {
    pub fn new(plan: Plan, work: &Path) -> Job {
        let mut tracks = vec![Track::new(0, plan.main.clone(), work)];
        if let Some(a) = &plan.audio {
            tracks.push(Track::new(1, a.clone(), work));
        }
        Job { plan, tracks }
    }

    /// Downloads (or records) every track. Can be called again after `Paused`/`Failed` to
    /// continue where it stopped.
    pub fn run(&mut self, ctx: &Ctx) -> Result<(), Halt> {
        ctx.abort.store(false, Ordering::Relaxed);
        let live = self.plan.live;
        let results: Vec<Result<(), Halt>> = std::thread::scope(|s| {
            let handles: Vec<_> = self
                .tracks
                .iter_mut()
                .map(|t| {
                    s.spawn(move || {
                        let r = if live { record(ctx, t) } else { download(ctx, t) };
                        match &r {
                            Ok(()) => {
                                t.complete = true;
                                // One live track ended (the broadcast is over, or Stop): end
                                // the other one too.
                                if live {
                                    ctx.control.set_if(RUN, STOP);
                                }
                            }
                            Err(Halt::Failed(_)) => ctx.abort.store(true, Ordering::Relaxed),
                            Err(_) => {}
                        }
                        r
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(Halt::Failed("internal error".into())))).collect()
        });
        let mut failed: Option<String> = None;
        let mut paused = false;
        for r in results {
            match r {
                Err(Halt::Canceled) => return Err(Halt::Canceled),
                Err(Halt::Failed(m)) => {
                    if failed.as_deref().is_none_or(str::is_empty) {
                        failed = Some(m);
                    }
                }
                // Stop only applies to live recordings, whose tracks end with Ok; anything else
                // stopped this way just waits like a pause.
                Err(Halt::Paused) | Err(Halt::Stopped) => paused = true,
                Ok(()) => {}
            }
        }
        if let Some(m) = failed {
            return Err(Halt::Failed(if m.is_empty() { "The download failed".into() } else { m }));
        }
        if paused {
            return Err(Halt::Paused);
        }
        Ok(())
    }

}

// ---------------------------------------------------------------------------------------------
// Runner: connects jobs to the downloads list

use crate::host::downloads::DownloadEvent;
use crate::downloads::DownloadRecord;
use crate::storage::now_ms;

/// Everything needed to start a job.
#[derive(Debug)]
pub struct Request {
    pub tab: String,
    /// File name without extension (already sanitised).
    pub stem: String,
    /// Extension used for the provisional name shown while downloading.
    pub ext: String,
    pub mime: String,
    /// Address shown in the downloads list.
    pub source_url: String,
    pub page_url: String,
    pub headers: Vec<(String, String)>,
    pub user_agent: String,
    pub plan: Plan,
}

/// The default downloads folder, or the one chosen in settings.
pub fn download_dir() -> PathBuf {
    use tauri::Manager;
    crate::host::config()
        .download_dir
        .filter(|d| d.is_dir())
        .or_else(|| crate::host::app().path().download_dir().ok())
        .unwrap_or_else(|| crate::host::paths().local.join("Downloads"))
}

/// FFmpeg from settings or PATH, if the user allows it.
pub fn find_ffmpeg() -> Option<PathBuf> {
    let cfg = crate::host::config();
    if !cfg.use_ffmpeg {
        return None;
    }
    if let Some(p) = cfg.ffmpeg_path.filter(|p| p.is_file()) {
        return Some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("ffmpeg.exe")).find(|p| p.is_file())
}

struct HostEnv {
    page_site: String,
}

/// Which addresses a download from `page_url` may contact: a public page may not point Zenith
/// into the local network (checked again on every redirect).
pub fn network_policy(page_url: &str) -> http::Policy {
    let page_private = crate::platform::urls::host_of(page_url).is_some_and(super::detect::is_private_host);
    Box::new(move |url: &str| page_private || !crate::platform::urls::host_of(url).is_some_and(super::detect::is_private_host))
}

impl Env for HostEnv {
    fn cookies(&self, url: &str) -> Option<String> {
        super::sniff::cookie_header(url, &self.page_site)
    }
    fn ffmpeg(&self) -> Option<PathBuf> {
        find_ffmpeg()
    }
}

fn work_root() -> PathBuf {
    crate::host::paths().local.join("media-tmp")
}

/// Removes partial data left behind by a previous session (jobs do not survive a restart).
pub fn cleanup_temp() {
    let _ = fs::remove_dir_all(work_root());
}

/// Starts a job and returns its download id.
pub fn start(req: Request) -> Result<i64, String> {
    let id = crate::host::downloads::next_id();
    let dir = download_dir();
    let control = Arc::new(Control::default());
    let progress = Arc::new(Progress::default());
    let record = DownloadRecord {
        id,
        url: req.source_url.clone(),
        path: dir.join(format!("{}.{}", req.stem, req.ext)).display().to_string(),
        mime: req.mime.clone(),
        total: 0,
        received: 0,
        state: "progress".into(),
        started: now_ms(),
        ended: None,
    };
    lock(&JOBS).insert(
        id,
        Running {
            record,
            tab: req.tab.clone(),
            control: control.clone(),
            progress: progress.clone(),
            live: req.plan.live,
            paused: false,
            reason: String::new(),
            note: String::new(),
            last_emit: Instant::now(),
            resumable: true,
            persisted: String::new(),
        },
    );
    publish(id, true);
    std::thread::Builder::new()
        .name(format!("zenith-media-{id}"))
        .spawn(move || run(id, req, dir, control, progress))
        .map_err(|e| {
            lock(&JOBS).remove(&id);
            e.to_string()
        })?;
    Ok(id)
}

/// A live recording that failed, then was stopped: save what was recorded.
fn save_recording(id: i64, req: &Request, job: &Job, ctx: &Ctx, dir: &Path) {
    match job.finish(ctx, dir, &req.stem) {
        Ok(out) => complete(id, req, out),
        Err(Halt::Failed(reason)) => settle(id, "interrupted", &reason, false, |_| {}),
        Err(_) => settle(id, "canceled", "", false, |_| {}),
    }
}

fn run(id: i64, req: Request, dir: PathBuf, control: Arc<Control>, progress: Arc<Progress>) {
    let work = work_root().join(id.to_string());
    let _ = fs::create_dir_all(&work);
    let site = crate::platform::urls::host_of(&req.page_url).map(crate::platform::urls::site_of).unwrap_or("").to_string();
    let env = HostEnv { page_site: site };
    let done = Arc::new(AtomicBool::new(false));
    {
        let done = done.clone();
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(400));
                publish(id, false);
            }
        });
    }
    let client = match Client::with_policy(&req.user_agent, network_policy(&req.page_url)) {
        Ok(c) => c,
        Err(e) => {
            done.store(true, Ordering::Relaxed);
            settle(id, "interrupted", &e.to_string(), false, |_| {});
            return;
        }
    };
    let ctx = Ctx::new(&client, &req.headers, &env, &control, &progress, &work);
    let mut job = Job::new(req.plan.clone(), &work);
    loop {
        let result = job.run(&ctx).and_then(|()| job.finish(&ctx, &dir, &req.stem));
        match result {
            Ok(out) => {
                complete(id, &req, out);
                break;
            }
            Err(Halt::Canceled) => {
                settle(id, "canceled", "", false, |_| {});
                break;
            }
            Err(Halt::Paused) | Err(Halt::Stopped) => {
                update(id, |j| j.paused = true);
                publish(id, true);
                let next = control.wait_resume();
                update(id, |j| j.paused = false);
                if next == CANCEL {
                    settle(id, "canceled", "", false, |_| {});
                    break;
                }
                // Stop only means something to a live recording (its next run finishes it).
                if next == STOP && !job.plan.live {
                    control.set(RUN);
                }
                publish(id, true);
            }
            Err(Halt::Failed(reason)) => {
                if reason == PROTECTED {
                    settle(id, "interrupted", &reason, false, |_| {});
                    break;
                }
                // Claim the wait before showing the failure, so a Cancel or Stop that already
                // arrived is not overwritten.
                if !control.set_if(RUN, PAUSE) && control.get() != PAUSE {
                    match control.get() {
                        STOP if job.plan.live => save_recording(id, &req, &job, &ctx, &dir),
                        _ => settle(id, "canceled", "", false, |_| {}),
                    }
                    break;
                }
                settle(id, "interrupted", &reason, true, |_| {});
                match control.wait_resume() {
                    RUN => {
                        update(id, |j| {
                            j.record.state = "progress".into();
                            j.record.ended = None;
                            j.reason.clear();
                        });
                        publish(id, true);
                    }
                    STOP if job.plan.live => {
                        save_recording(id, &req, &job, &ctx, &dir);
                        break;
                    }
                    _ => {
                        // Removed from the list (or canceled).
                        settle(id, "canceled", "", false, |_| {});
                        break;
                    }
                }
            }
        }
    }
    done.store(true, Ordering::Relaxed);
    let _ = fs::remove_dir_all(&work);
}

/// True when `id` is a media job (as opposed to a WebView2 download).
pub fn owns(id: i64) -> bool {
    lock(&JOBS).contains_key(&id)
}

/// pause / resume / cancel / stop from the downloads UI.
pub fn action(id: i64, action: &str) -> Result<(), String> {
    let (control, live) = {
        let jobs = lock(&JOBS);
        let j = jobs.get(&id).ok_or("download is not active")?;
        (j.control.clone(), j.live)
    };
    match action {
        // A live recording cannot pause (the stream moves on); it can only stop.
        "pause" if live => return Err("A live recording can't be paused; stop it to keep what was recorded".into()),
        "pause" => control.set(PAUSE),
        "resume" => control.set(RUN),
        "cancel" => control.set(CANCEL),
        "stop" if live => control.set(STOP),
        other => return Err(format!("unknown download action {other}")),
    }
    publish(id, true);
    Ok(())
}

/// Snapshots of every job still in memory.
pub fn active() -> Vec<DownloadEvent> {
    lock(&JOBS).values_mut().map(snapshot).collect()
}

/// Forgets a job the user removed from the list (stops it if it is waiting for a retry).
pub fn forget(id: Option<i64>) {
    let mut jobs = lock(&JOBS);
    let ids: Vec<i64> = match id {
        Some(id) => vec![id],
        None => jobs.iter().filter(|(_, j)| j.record.state != "progress").map(|(k, _)| *k).collect(),
    };
    for id in ids {
        let removable = jobs.get(&id).is_some_and(|j| j.record.state != "progress");
        if removable {
            if let Some(j) = jobs.remove(&id) {
                j.control.set(CANCEL);
            }
        }
    }
}
