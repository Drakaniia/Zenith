//! Media download jobs. Each job runs on its own thread: it resolves the stream (HLS playlists,
//! DASH segment lists, plain files), downloads segments in parallel, appends them in order,
//! merges separate audio and video, and reports progress into the regular downloads list.
//!
//! The engine (`Job`, `Ctx`) has no dependency on the browser; the runner at the bottom connects
//! it to the downloads UI, the cookie store and the settings.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::dash::SegRef;
use super::hls::{self, Playlist};
use super::http::{self, Client};
use super::mp4;

pub const PROTECTED: &str = "This video is DRM-protected, so it can't be saved";
const RETRIES: u32 = 5;
const PARALLEL: usize = 6;
/// Consecutive playlist or segment failures after which a live recording counts as ended.
const LIVE_FAILURES: u32 = 6;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

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
    fn set_stage(&self, text: &str) {
        *lock(&self.stage) = text.to_string();
    }
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

/// What the engine needs from the browser. (Which addresses may be contacted is the `Client`'s
/// policy.)
pub trait Env: Send + Sync {
    /// The `Cookie` header the browser would send to `url`.
    fn cookies(&self, url: &str) -> Option<String>;
    /// FFmpeg, when available and allowed.
    fn ffmpeg(&self) -> Option<PathBuf>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Halt {
    Paused,
    Canceled,
    /// The user stopped a live recording: finish with what was recorded.
    Stopped,
    /// An empty message means "stopped because another part of the job failed".
    Failed(String),
}

pub struct Ctx<'a> {
    pub client: &'a Client,
    pub headers: &'a [(String, String)],
    pub env: &'a dyn Env,
    pub control: &'a Control,
    pub progress: &'a Progress,
    pub work: &'a Path,
    pub parallel: usize,
    /// Waits of a live recording (playlist refresh, retry after a failure); tests shorten them.
    pub live_wait: Option<Duration>,
    abort: AtomicBool,
    cookies: Mutex<HashMap<String, Option<String>>>,
}

impl<'a> Ctx<'a> {
    pub fn new(
        client: &'a Client,
        headers: &'a [(String, String)],
        env: &'a dyn Env,
        control: &'a Control,
        progress: &'a Progress,
        work: &'a Path,
    ) -> Self {
        Ctx {
            client,
            headers,
            env,
            control,
            progress,
            work,
            parallel: PARALLEL,
            live_wait: None,
            abort: AtomicBool::new(false),
            cookies: Mutex::default(),
        }
    }

    fn stop_now(&self) -> bool {
        self.control.get() != RUN || self.abort.load(Ordering::Relaxed)
    }

    fn pause_for(&self, d: Duration) -> bool {
        self.control.sleep(d) && !self.abort.load(Ordering::Relaxed)
    }

    fn halt(&self) -> Halt {
        match self.control.get() {
            CANCEL => Halt::Canceled,
            STOP => Halt::Stopped,
            RUN if self.abort.load(Ordering::Relaxed) => Halt::Failed(String::new()),
            _ => Halt::Paused,
        }
    }

    fn aborted(&self) -> bool {
        self.abort.load(Ordering::Relaxed)
    }

    /// Request headers for `url`: the page's Referer/Origin plus the browser's cookies.
    fn headers_for(&self, url: &str) -> Vec<(String, String)> {
        let mut h = self.headers.to_vec();
        let key = tauri::Url::parse(url).map(|u| u.origin().ascii_serialization()).unwrap_or_default();
        let cached = lock(&self.cookies).get(&key).cloned();
        let cookie = match cached {
            Some(c) => c,
            None => {
                let c = self.env.cookies(url);
                lock(&self.cookies).insert(key, c.clone());
                c
            }
        };
        if let Some(c) = cookie.filter(|c| !c.is_empty()) {
            h.push(("Cookie".into(), c));
        }
        h
    }
}

fn io_halt(e: io::Error) -> Halt {
    Halt::Failed(format!("Could not write the file: {e}"))
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 << attempt.min(5))
}

fn with_retries<T>(ctx: &Ctx, mut f: impl FnMut() -> Result<T, http::Error>) -> Result<T, Halt> {
    let mut attempt = 0;
    loop {
        if ctx.stop_now() {
            return Err(ctx.halt());
        }
        match f() {
            Ok(v) => return Ok(v),
            Err(http::Error::Stopped) => return Err(ctx.halt()),
            Err(e) if e.retryable() && attempt < RETRIES => {
                attempt += 1;
                if !ctx.pause_for(backoff(attempt)) {
                    return Err(ctx.halt());
                }
            }
            Err(e) => return Err(Halt::Failed(e.to_string())),
        }
    }
}

/// (skip, take) to cut a requested byte range out of a response.
fn range_cut(range: Option<(u64, u64)>, status: u32) -> (u64, Option<u64>) {
    match (range, status) {
        // The server ignored the Range header and sent the whole resource.
        (Some((offset, len)), 200) => (offset, Some(len)),
        (Some((_, len)), _) => (0, Some(len)),
        (None, _) => (0, None),
    }
}

fn http_range(range: Option<(u64, u64)>) -> Option<(u64, Option<u64>)> {
    range.map(|(o, l)| (o, Some(o + l.max(1) - 1)))
}

/// Downloads a small resource (initialisation segment) into memory.
fn fetch_small(ctx: &Ctx, seg: &SegRef) -> Result<Vec<u8>, Halt> {
    with_retries(ctx, || {
        let mut r = ctx.client.get(&seg.url, &|u| ctx.headers_for(u), http_range(seg.range), false)?;
        match range_cut(seg.range, r.status) {
            (0, None) => r.bytes(64 << 20),
            (skip, take) => {
                let mut buf = Vec::new();
                r.copy_to(&mut buf, skip, take, &|| ctx.stop_now(), &mut |_| {})?;
                Ok(buf)
            }
        }
    })
}

/// Downloads one segment to `path` (written as `.part`, renamed when complete, reused when it is
/// already there). `Err(None)` means the job was asked to stop.
fn fetch_segment(ctx: &Ctx, p: &TrackProgress, seg: &SegRef, path: &Path) -> Result<u64, Option<http::Error>> {
    if let Ok(m) = fs::metadata(path) {
        return Ok(m.len());
    }
    let tmp = path.with_extension("part");
    let mut attempt = 0;
    loop {
        if ctx.stop_now() {
            return Err(None);
        }
        let mut counted = 0u64;
        let result = (|| {
            let mut r = ctx.client.get(&seg.url, &|u| ctx.headers_for(u), http_range(seg.range), false)?;
            let mut f = File::create(&tmp).map_err(|e| http::Error::Io(e.to_string()))?;
            let (skip, take) = range_cut(seg.range, r.status);
            let n = r.copy_to(&mut f, skip, take, &|| ctx.stop_now(), &mut |n| {
                counted += n;
                p.bytes.fetch_add(n, Ordering::Relaxed);
            })?;
            f.flush().map_err(|e| http::Error::Io(e.to_string()))?;
            Ok::<u64, http::Error>(n)
        })();
        match result {
            Ok(n) => {
                fs::rename(&tmp, path).map_err(|e| Some(http::Error::Io(e.to_string())))?;
                return Ok(n);
            }
            Err(e) => {
                p.bytes.fetch_sub(counted.min(p.bytes.load(Ordering::Relaxed)), Ordering::Relaxed);
                let _ = fs::remove_file(&tmp);
                match e {
                    http::Error::Stopped => return Err(None),
                    e if e.retryable() && attempt < RETRIES => {
                        attempt += 1;
                        if !ctx.pause_for(backoff(attempt)) {
                            return Err(None);
                        }
                    }
                    e => return Err(Some(e)),
                }
            }
        }
    }
}

fn append(out: &mut File, path: &Path) -> io::Result<u64> {
    let mut f = BufReader::with_capacity(1 << 20, File::open(path)?);
    io::copy(&mut f, out)
}

// ---------------------------------------------------------------------------------------------
// The engine

struct Track {
    index: usize,
    source: Source,
    /// The assembled output so far.
    part: PathBuf,
    part_len: u64,
    /// VOD: number of segments appended.
    appended: usize,
    resolved: Option<(Option<SegRef>, Vec<SegRef>)>,
    /// Live: media sequence number of the last appended segment.
    last_seq: Option<u64>,
    /// Live: recently recorded segment addresses (to notice a restarted sequence).
    recent: VecDeque<String>,
    init_done: bool,
    /// Finished; a later run (after the other track paused or failed) leaves it alone.
    complete: bool,
    /// Container of the output ("mp4", "ts", "webm", ...).
    ext: String,
    /// The output is (fragmented) MP4.
    mp4: bool,
}

impl Track {
    fn new(index: usize, source: Source, work: &Path) -> Track {
        let ext = match &source {
            Source::Segments { ext, .. } | Source::File { ext, .. } => ext.clone(),
            Source::Hls(_) => "ts".into(),
        };
        Track {
            index,
            mp4: matches!(ext.as_str(), "mp4" | "m4a" | "m4v"),
            source,
            part: work.join(format!("track{index}.part")),
            part_len: 0,
            appended: 0,
            resolved: None,
            last_seq: None,
            recent: VecDeque::new(),
            init_done: false,
            complete: false,
            ext,
        }
    }

    /// The assembled part vanished (an earlier finish moved it, then failed): start over rather
    /// than pad an empty file to the old length.
    fn recover(&mut self, p: &TrackProgress) {
        if self.part_len > 0 && !self.part.exists() {
            self.part_len = 0;
            self.appended = 0;
            self.init_done = false;
            self.last_seq = None;
            self.recent.clear();
            self.complete = false;
            for c in [&p.bytes, &p.done, &p.appended, &p.recorded_ms] {
                c.store(0, Ordering::Relaxed);
            }
        }
    }

    fn open_part(&self) -> Result<File, Halt> {
        let mut f = OpenOptions::new().create(true).read(true).write(true).truncate(false).open(&self.part).map_err(io_halt)?;
        // Drop anything appended after the last complete segment (an interrupted write).
        f.set_len(self.part_len).map_err(io_halt)?;
        f.seek(SeekFrom::End(0)).map_err(io_halt)?;
        Ok(f)
    }

    fn write_init(&mut self, ctx: &Ctx, out: &mut File, init: &SegRef) -> Result<(), Halt> {
        let data = fetch_small(ctx, init)?;
        if mp4::is_protected(&data) {
            return Err(Halt::Failed(PROTECTED.into()));
        }
        if mp4::looks_like_mp4(&data) {
            self.mp4 = true;
        }
        out.write_all(&data).map_err(io_halt)?;
        self.part_len += data.len() as u64;
        self.init_done = true;
        Ok(())
    }
}

/// Reads an HLS media playlist (following a master playlist to its best variant if needed).
fn media_playlist(ctx: &Ctx, url: &str) -> Result<hls::MediaPlaylist, Halt> {
    let mut url = url.to_string();
    for _ in 0..2 {
        let (text, final_url) = with_retries(ctx, || ctx.client.text(&url, &|u| ctx.headers_for(u), 32 << 20))?;
        match hls::parse(&text, &final_url) {
            Ok(Playlist::Media(p)) if p.encrypted => return Err(Halt::Failed(PROTECTED.into())),
            Ok(Playlist::Media(p)) => return Ok(p),
            Ok(Playlist::Master(m)) if m.encrypted => return Err(Halt::Failed(PROTECTED.into())),
            Ok(Playlist::Master(m)) => match m.variants.iter().max_by_key(|v| v.bandwidth) {
                Some(best) => url = best.uri.clone(),
                None => return Err(Halt::Failed("The playlist lists no streams".into())),
            },
            Err(e) => return Err(Halt::Failed(format!("The stream's playlist could not be read ({e})"))),
        }
    }
    Err(Halt::Failed("The stream's playlist could not be read".into()))
}

fn to_segrefs(p: &hls::MediaPlaylist) -> (Option<SegRef>, Vec<SegRef>) {
    (
        p.init.as_ref().map(|i| SegRef { url: i.uri.clone(), range: i.range }),
        p.segments.iter().map(|s| SegRef { url: s.uri.clone(), range: s.range }).collect(),
    )
}

/// Downloads `segs[t.appended..]` with parallel workers and appends them to `out` in order.
fn fetch_segments(ctx: &Ctx, t: &mut Track, segs: &[SegRef], out: &mut File) -> Result<(), Halt> {
    let n = segs.len();
    let start = t.appended;
    if start >= n {
        return Ok(());
    }
    let p = &ctx.progress.tracks[t.index];
    let next = AtomicUsize::new(start);
    let running = AtomicUsize::new(0);
    let ready: Mutex<HashMap<usize, Result<PathBuf, Option<http::Error>>>> = Mutex::default();
    let cv = Condvar::new();
    let workers = ctx.parallel.clamp(1, 16).min(n - start);
    let index = t.index;
    let mut outcome: Result<(), Halt> = Ok(());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            running.fetch_add(1, Ordering::SeqCst);
            scope.spawn(|| {
                loop {
                    if ctx.stop_now() {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= n {
                        break;
                    }
                    let path = ctx.work.join(format!("t{index}-{i:06}.seg"));
                    let r = fetch_segment(ctx, p, &segs[i], &path).map(|_| path);
                    let stop = r.is_err();
                    lock(&ready).insert(i, r);
                    cv.notify_all();
                    if stop {
                        break;
                    }
                }
                running.fetch_sub(1, Ordering::SeqCst);
                cv.notify_all();
            });
        }
        let mut i = start;
        while i < n {
            let item = {
                let mut g = lock(&ready);
                loop {
                    if let Some(r) = g.remove(&i) {
                        break Some(r);
                    }
                    if running.load(Ordering::SeqCst) == 0 || ctx.stop_now() {
                        break None;
                    }
                    g = cv.wait_timeout(g, Duration::from_millis(250)).unwrap_or_else(|e| e.into_inner()).0;
                }
            };
            match item {
                Some(Ok(path)) => match append(out, &path) {
                    Ok(len) => {
                        let _ = fs::remove_file(&path);
                        t.part_len += len;
                        t.appended = i + 1;
                        p.appended.fetch_add(len, Ordering::Relaxed);
                        p.done.fetch_add(1, Ordering::Relaxed);
                        i += 1;
                    }
                    Err(e) => {
                        ctx.abort.store(true, Ordering::Relaxed);
                        outcome = Err(io_halt(e));
                        break;
                    }
                },
                Some(Err(Some(e))) => {
                    ctx.abort.store(true, Ordering::Relaxed);
                    outcome = Err(Halt::Failed(e.to_string()));
                    break;
                }
                Some(Err(None)) | None => break,
            }
        }
        if outcome.is_ok() && i < n {
            outcome = Err(ctx.halt());
        }
    });
    let _ = out.flush();
    outcome
}

/// Downloads a whole file into the track's part, resuming from what is already there.
fn fetch_file(ctx: &Ctx, t: &mut Track, url: &str) -> Result<(), Halt> {
    let p = &ctx.progress.tracks[t.index];
    let mut attempt = 0;
    loop {
        if ctx.stop_now() {
            return Err(ctx.halt());
        }
        let have = fs::metadata(&t.part).map(|m| m.len()).unwrap_or(0);
        let error = match ctx.client.get(url, &|u| ctx.headers_for(u), (have > 0).then_some((have, None)), false) {
            Ok(mut resp) => {
                let restart = have > 0 && resp.status != 206;
                let base = if restart { 0 } else { have };
                let mut f = match OpenOptions::new().create(true).write(true).append(!restart).truncate(restart).open(&t.part) {
                    Ok(f) => f,
                    Err(e) => return Err(io_halt(e)),
                };
                p.bytes.store(base, Ordering::Relaxed);
                if let Some(total) = resp.total.or(resp.content_length.map(|l| l + base)) {
                    p.total.store(total, Ordering::Relaxed);
                }
                match resp.copy_to(&mut f, 0, None, &|| ctx.stop_now(), &mut |n| {
                    p.bytes.fetch_add(n, Ordering::Relaxed);
                }) {
                    Ok(_) => {
                        f.flush().map_err(io_halt)?;
                        t.part_len = fs::metadata(&t.part).map(|m| m.len()).unwrap_or(0);
                        p.total.store(t.part_len, Ordering::Relaxed);
                        return Ok(());
                    }
                    Err(e) => e,
                }
            }
            // Nothing left to fetch: an earlier attempt already got the whole file.
            Err(http::Error::Status(416)) if have > 0 => {
                t.part_len = have;
                return Ok(());
            }
            Err(e) => e,
        };
        match error {
            http::Error::Stopped => return Err(ctx.halt()),
            e if e.retryable() && attempt < RETRIES + 1 => {
                attempt += 1;
                if !ctx.pause_for(backoff(attempt)) {
                    return Err(ctx.halt());
                }
            }
            e => return Err(Halt::Failed(e.to_string())),
        }
    }
}

/// Downloads one track of an on-demand plan.
fn download(ctx: &Ctx, t: &mut Track) -> Result<(), Halt> {
    t.recover(&ctx.progress.tracks[t.index]);
    if t.complete {
        return Ok(());
    }
    let (init, media) = match t.source.clone() {
        Source::File { url, .. } => {
            fetch_file(ctx, t, &url)?;
            // A DASH single-file representation may still carry Common Encryption.
            let mut head = Vec::new();
            if let Ok(f) = File::open(&t.part) {
                let _ = f.take(4 << 20).read_to_end(&mut head);
            }
            if mp4::is_protected(&head) {
                return Err(Halt::Failed(PROTECTED.into()));
            }
            return Ok(());
        }
        Source::Segments { init, media, .. } => (init, media),
        Source::Hls(url) => {
            if t.resolved.is_none() {
                let pl = media_playlist(ctx, &url)?;
                t.ext = pl.container().to_string();
                t.mp4 = pl.init.is_some() || t.ext == "mp4";
                t.resolved = Some(to_segrefs(&pl));
            }
            t.resolved.clone().unwrap_or_default()
        }
    };
    let p = &ctx.progress.tracks[t.index];
    p.count.store(media.len() as u64, Ordering::Relaxed);
    p.done.store(t.appended as u64, Ordering::Relaxed);
    let mut out = t.open_part()?;
    if !t.init_done {
        match &init {
            Some(init) => t.write_init(ctx, &mut out, init)?,
            None => t.init_done = true,
        }
    }
    fetch_segments(ctx, t, &media, &mut out)
}

/// Records a live HLS track until the user stops it or the stream ends. Stopping, the end of the
/// broadcast and a stream that keeps failing after something was recorded all keep the file.
fn record(ctx: &Ctx, t: &mut Track) -> Result<(), Halt> {
    t.recover(&ctx.progress.tracks[t.index]);
    if t.complete {
        return Ok(());
    }
    match record_until_end(ctx, t) {
        Err(Halt::Stopped) => Ok(()),
        other => other,
    }
}

fn record_until_end(ctx: &Ctx, t: &mut Track) -> Result<(), Halt> {
    let Source::Hls(url) = t.source.clone() else { return download(ctx, t) };
    let p = &ctx.progress.tracks[t.index];
    let mut out = t.open_part()?;
    let (mut playlist_failures, mut segment_failures) = (0u32, 0u32);
    let wait = |d: Duration| ctx.live_wait.unwrap_or(d);
    // What to return when the stream stays unreachable: keep a recording that has data.
    let ended = |t: &Track, error: String| if t.appended > 0 { Ok(()) } else { Err(Halt::Failed(error)) };
    loop {
        match ctx.control.get() {
            CANCEL => return Err(Halt::Canceled),
            STOP => return Ok(()),
            _ if ctx.aborted() => return Err(Halt::Failed(String::new())),
            _ => {}
        }
        let pl = match media_playlist(ctx, &url) {
            Ok(pl) => {
                playlist_failures = 0;
                pl
            }
            Err(Halt::Failed(e)) if e == PROTECTED || ctx.aborted() => return Err(Halt::Failed(e)),
            Err(Halt::Failed(e)) => {
                playlist_failures += 1;
                if playlist_failures >= LIVE_FAILURES {
                    return ended(t, e);
                }
                ctx.control.sleep(wait(Duration::from_secs(2)));
                continue;
            }
            Err(h) => return Err(h),
        };
        if !t.init_done {
            t.ext = pl.container().to_string();
            t.mp4 = pl.init.is_some() || t.ext == "mp4";
            if let Some(init) = &pl.init {
                let init = SegRef { url: init.uri.clone(), range: init.range };
                t.write_init(ctx, &mut out, &init)?;
            }
            t.init_done = true;
        }
        let newest = pl.segments.last().map(|s| s.sequence);
        let fresh: Vec<&hls::Segment> = match t.last_seq {
            None if pl.ended => pl.segments.iter().collect(),
            // Start near the live edge, like a player.
            None => pl.segments.iter().skip(pl.segments.len().saturating_sub(3)).collect(),
            // The numbering restarted (encoder restart, or no EXT-X-MEDIA-SEQUENCE in a sliding
            // window): anything not recorded recently is new.
            Some(last) if newest.is_some_and(|n| n < last) => {
                pl.segments.iter().filter(|s| !t.recent.contains(&s.uri)).collect()
            }
            Some(last) => pl.segments.iter().filter(|s| s.sequence > last).collect(),
        };
        for seg in fresh {
            if ctx.stop_now() {
                break;
            }
            let path = ctx.work.join(format!("t{}-live-{}.seg", t.index, seg.sequence));
            match fetch_segment(ctx, p, &SegRef { url: seg.uri.clone(), range: seg.range }, &path) {
                Ok(_) => {
                    let len = append(&mut out, &path).map_err(io_halt)?;
                    let _ = fs::remove_file(&path);
                    t.part_len += len;
                    t.appended += 1;
                    p.appended.fetch_add(len, Ordering::Relaxed);
                    p.done.fetch_add(1, Ordering::Relaxed);
                    p.recorded_ms.fetch_add((seg.duration.max(0.0) * 1000.0) as u64, Ordering::Relaxed);
                    segment_failures = 0;
                }
                // The disk is full or the folder is gone: say so instead of recording nothing.
                Err(Some(e @ http::Error::Io(_))) => return Err(Halt::Failed(e.to_string())),
                // One segment can fall out of the live window; a run of failures (expired
                // tokens, the broadcast taken down) ends the recording.
                Err(Some(e)) => {
                    segment_failures += 1;
                    if segment_failures >= LIVE_FAILURES {
                        let _ = out.flush();
                        return ended(t, e.to_string());
                    }
                }
                Err(None) => break,
            }
            t.last_seq = Some(seg.sequence);
            t.recent.push_back(seg.uri.clone());
            if t.recent.len() > 64 {
                t.recent.pop_front();
            }
        }
        let _ = out.flush();
        if pl.ended {
            return Ok(());
        }
        let refresh = if pl.target_duration.is_finite() { (pl.target_duration / 2.0).clamp(1.0, 10.0) } else { 2.0 };
        ctx.control.sleep(wait(Duration::from_secs_f64(refresh)));
    }
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

    /// Converts or merges the downloaded tracks and moves the results into `dir`.
    pub fn finish(&self, ctx: &Ctx, dir: &Path, stem: &str) -> Result<Output, Halt> {
        let ffmpeg = ctx.env.ffmpeg();
        let main = &self.tracks[0];
        if main.part_len == 0 && fs::metadata(&main.part).map(|m| m.len()).unwrap_or(0) == 0 {
            return Err(Halt::Failed("Nothing was downloaded".into()));
        }
        let cancelled = || ctx.control.get() == CANCEL;
        let Some(audio) = self.tracks.get(1) else {
            if main.ext == "ts" && !main.mp4 {
                if let Some(ff) = &ffmpeg {
                    ctx.progress.set_stage("Converting to MP4…");
                    let out = ctx.work.join("converted.mp4");
                    let mut args = ffmpeg_input(main);
                    args.extend([os("-map"), os("0:v?"), os("-map"), os("0:a?"), os("-c"), os("copy"), out.as_os_str()]);
                    match run_ffmpeg(ff, &args, &cancelled) {
                        Ok(()) => return place(&out, dir, stem, "mp4").map(|f| Output { files: vec![f], note: String::new() }),
                        Err(_) if cancelled() => return Err(Halt::Canceled),
                        Err(e) => eprintln!("[zenith] FFmpeg conversion failed: {e}"),
                    }
                }
            }
            return place(&main.part, dir, stem, &main.ext).map(|f| Output { files: vec![f], note: String::new() });
        };
        ctx.progress.set_stage("Merging audio and video…");
        if main.mp4 && audio.mp4 {
            let out = ctx.work.join("merged.mp4");
            match mp4::merge(&main.part, &audio.part, &out, &cancelled) {
                Ok(()) => return place(&out, dir, stem, "mp4").map(|f| Output { files: vec![f], note: String::new() }),
                Err(_) if cancelled() => return Err(Halt::Canceled),
                Err(e) if e.contains("DRM") => return Err(Halt::Failed(PROTECTED.into())),
                Err(e) => eprintln!("[zenith] native merge failed: {e}"),
            }
        }
        if let Some(ff) = &ffmpeg {
            let webm = |e: &str| matches!(e, "webm" | "weba" | "opus" | "ogg");
            let ext = if webm(&main.ext) && webm(&audio.ext) {
                "webm"
            } else if webm(&main.ext) || webm(&audio.ext) {
                "mkv"
            } else {
                "mp4"
            };
            let out = ctx.work.join(format!("merged.{ext}"));
            let mut args = ffmpeg_input(main);
            args.extend(ffmpeg_input(audio));
            args.extend([os("-map"), os("0:v:0"), os("-map"), os("1:a:0"), os("-c"), os("copy"), out.as_os_str()]);
            match run_ffmpeg(ff, &args, &cancelled) {
                Ok(()) => return place(&out, dir, stem, ext).map(|f| Output { files: vec![f], note: String::new() }),
                Err(_) if cancelled() => return Err(Halt::Canceled),
                Err(e) => eprintln!("[zenith] FFmpeg merge failed: {e}"),
            }
        }
        // Keep both parts rather than nothing. If the second can't be placed, put the first back
        // so a retry finds both.
        let audio_ext = if audio.mp4 { "m4a" } else { audio.ext.as_str() };
        let v = place(&main.part, dir, stem, &main.ext)?;
        let a = match place(&audio.part, dir, &format!("{stem} (audio)"), audio_ext) {
            Ok(a) => a,
            Err(e) => {
                if fs::rename(&v, &main.part).is_err() && fs::copy(&v, &main.part).is_ok() {
                    let _ = fs::remove_file(&v);
                }
                return Err(e);
            }
        };
        Ok(Output {
            files: vec![v, a],
            note: "Video and audio were saved as separate files (install FFmpeg to have them merged)".into(),
        })
    }
}

/// `-protocol_whitelist file -f <demuxer> -i <part>`: FFmpeg reads the part as the container it
/// is, and never follows addresses inside it (a hostile "segment" could be a playlist).
fn ffmpeg_input(t: &Track) -> Vec<&std::ffi::OsStr> {
    let demuxer = if t.mp4 {
        "mov"
    } else {
        match t.ext.as_str() {
            "ts" => "mpegts",
            "mp4" | "m4a" | "m4v" | "mov" => "mov",
            "webm" | "weba" | "mkv" => "matroska",
            "aac" => "aac",
            "mp3" => "mp3",
            "ogg" | "oga" | "opus" => "ogg",
            _ => "",
        }
    };
    let mut args = vec![os("-protocol_whitelist"), os("file")];
    if !demuxer.is_empty() {
        args.extend([os("-f"), os(demuxer)]);
    }
    args.extend([os("-i"), t.part.as_os_str()]);
    args
}

fn os(s: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(s)
}

/// Runs FFmpeg without a console window; kills it when `cancelled` turns true.
fn run_ffmpeg(ffmpeg: &Path, args: &[&std::ffi::OsStr], cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = std::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y"])
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Drain stderr on a thread so a chatty FFmpeg never blocks on a full pipe.
    let stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut e) = stderr {
            let _ = (&mut e).take(64 * 1024).read_to_string(&mut text);
            let _ = io::copy(&mut e, &mut io::sink());
        }
        text
    });
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let err = reader.join().unwrap_or_default();
                return if status.success() {
                    Ok(())
                } else {
                    Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("FFmpeg failed").trim().to_string())
                };
            }
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("canceled".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Picks a free name in `dir` (`stem.ext`, `stem (1).ext`, ...) and claims it.
fn reserve(dir: &Path, stem: &str, ext: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    for n in 0..1000 {
        let name = if n == 0 { format!("{stem}.{ext}") } else { format!("{stem} ({n}).{ext}") };
        let path = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free file name"))
}

/// Moves a finished file into the downloads folder under a free name.
fn place(src: &Path, dir: &Path, stem: &str, ext: &str) -> Result<PathBuf, Halt> {
    let dest = reserve(dir, stem, ext).map_err(io_halt)?;
    if fs::rename(src, &dest).is_err() {
        // Different volume: copy, then remove the original. A failed copy leaves nothing behind.
        if let Err(e) = fs::copy(src, &dest) {
            let _ = fs::remove_file(&dest);
            return Err(io_halt(e));
        }
        let _ = fs::remove_file(src);
    }
    Ok(dest)
}

/// Marks a downloaded file as coming from the internet (the `Zone.Identifier` stream that
/// browsers write), so Windows and other apps treat it with the usual care. Query strings are
/// left out: they often carry access tokens.
fn mark_of_the_web(path: &Path, host_url: &str, referrer: &str) {
    let clean = |u: &str| -> String {
        let u = u.split(['?', '#']).next().unwrap_or("");
        u.chars().filter(|c| !c.is_control()).take(2048).collect()
    };
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let body = format!("[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl={}\r\nHostUrl={}\r\n", clean(referrer), clean(host_url));
    // Not every file system has alternate data streams (FAT, exFAT); that is fine.
    let _ = fs::write(PathBuf::from(stream), body);
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

struct Running {
    record: DownloadRecord,
    tab: String,
    control: Arc<Control>,
    progress: Arc<Progress>,
    live: bool,
    paused: bool,
    reason: String,
    note: String,
    last_emit: Instant,
    /// A failed job waits for Retry; DRM and setup failures can't be retried.
    resumable: bool,
    /// The state last written to the database (it is written on changes only).
    persisted: String,
}

static JOBS: LazyLock<Mutex<HashMap<i64, Running>>> = LazyLock::new(Mutex::default);

fn format_clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn snapshot(j: &mut Running) -> DownloadEvent {
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
    }
}

/// Sends a job's state to the UI, and to the database when the state changed. Runs under the
/// JOBS lock, so events leave in the order the job changed and a job removed from the list can't
/// be written back after the removal (both calls only queue work).
fn emit_locked(j: &mut Running) {
    j.last_emit = Instant::now();
    let event = snapshot(j);
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
fn publish(id: i64, force: bool) {
    let mut jobs = lock(&JOBS);
    let Some(j) = jobs.get_mut(&id) else { return };
    if !force && (j.paused || j.record.state != "progress" || j.last_emit.elapsed() < Duration::from_millis(300)) {
        return;
    }
    emit_locked(j);
}

fn update(id: i64, f: impl FnOnce(&mut Running)) {
    if let Some(j) = lock(&JOBS).get_mut(&id) {
        f(j);
    }
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

/// Moves a job to a final or failed state in one step (so the progress ticker can't interleave),
/// publishes it, and forgets it unless it waits for Retry.
fn settle(id: i64, state: &str, reason: &str, resumable: bool, patch: impl FnOnce(&mut Running)) {
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

fn complete(id: i64, req: &Request, out: Output) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::mp4::testkit::{fragment, init};
    use std::io::BufRead;
    use std::net::TcpListener;

    type Routes = HashMap<String, (String, Vec<u8>)>;
    type Log = Arc<Mutex<Vec<(String, String, String)>>>;

    /// A tiny HTTP/1.1 server for the engine tests: serves fixed bodies, honours single byte
    /// ranges and records the requests it saw (path, range header, cookie header).
    struct Server {
        base: String,
        log: Log,
    }

    fn serve(routes: Routes) -> Server {
        serve_seq(routes, HashMap::new())
    }

    /// `seqs`: replies served in turn for a path (the last one repeats). A content type of
    /// "redirect" sends the body as the Location; "status/NNN" answers with that status.
    fn serve_seq(routes: Routes, seqs: HashMap<String, Vec<(String, Vec<u8>)>>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let routes = Arc::new(routes);
        let seqs: HashMap<String, VecDeque<(String, Vec<u8>)>> = seqs.into_iter().map(|(k, v)| (k, v.into())).collect();
        let seqs = Arc::new(Mutex::new(seqs));
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let routes = routes.clone();
                let seqs = seqs.clone();
                let log = log2.clone();
                std::thread::spawn(move || {
                    let mut reader = io::BufReader::new(stream.try_clone().unwrap());
                    let mut stream = stream;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                        let (mut range, mut cookie) = (String::new(), String::new());
                        loop {
                            let mut h = String::new();
                            if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                                break;
                            }
                            let lower = h.to_ascii_lowercase();
                            if let Some(v) = lower.strip_prefix("range:") {
                                range = v.trim().to_string();
                            } else if lower.starts_with("cookie:") {
                                cookie = h["cookie:".len()..].trim().to_string();
                            }
                        }
                        lock(&log).push((path.clone(), range.clone(), cookie));
                        let key = path.split('?').next().unwrap_or("").to_string();
                        let next = {
                            let mut s = lock(&seqs);
                            match s.get_mut(&key) {
                                Some(q) if q.len() > 1 => q.pop_front(),
                                Some(q) => q.front().cloned(),
                                None => None,
                            }
                        };
                        let route = next.or_else(|| routes.get(&key).cloned());
                        let (status, ctype, body, extra) = match route {
                            None => ("404 Not Found".to_string(), "text/plain".to_string(), b"missing".to_vec(), String::new()),
                            Some((ct, body)) if ct == "redirect" => {
                                let location = format!("Location: {}\r\n", String::from_utf8_lossy(&body));
                                ("302 Found".to_string(), "text/plain".to_string(), Vec::new(), location)
                            }
                            Some((ct, _)) if ct.starts_with("status/") => {
                                (format!("{} Error", &ct[7..]), "text/plain".to_string(), Vec::new(), String::new())
                            }
                            Some((ct, body)) => match range.strip_prefix("bytes=").and_then(|r| r.split_once('-')) {
                                Some((a, b)) => {
                                    let a: usize = a.parse().unwrap();
                                    let b: usize = if b.is_empty() { body.len() - 1 } else { b.parse::<usize>().unwrap().min(body.len() - 1) };
                                    let extra = format!("Content-Range: bytes {a}-{b}/{}\r\n", body.len());
                                    ("206 Partial Content".to_string(), ct, body[a..=b].to_vec(), extra)
                                }
                                None => ("200 OK".to_string(), ct, body, String::new()),
                            },
                        };
                        let head = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n{extra}Connection: keep-alive\r\n\r\n",
                            body.len()
                        );
                        if stream.write_all(head.as_bytes()).is_err() || stream.write_all(&body).is_err() {
                            return;
                        }
                    }
                });
            }
        });
        Server { base, log }
    }

    struct TestEnv;
    impl Env for TestEnv {
        fn cookies(&self, _url: &str) -> Option<String> {
            Some("session=abc".into())
        }
        fn ffmpeg(&self) -> Option<PathBuf> {
            None
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("zenith-job-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Cookies that name the origin they are for, to see which host received which.
    struct OriginCookies;
    impl Env for OriginCookies {
        fn cookies(&self, url: &str) -> Option<String> {
            Some(format!("site={}", tauri::Url::parse(url).ok()?.port()?))
        }
        fn ffmpeg(&self) -> Option<PathBuf> {
            None
        }
    }

    /// Runs a plan with a given environment; `watch` runs alongside (it may press Stop).
    fn run_with(
        plan: Plan,
        name: &str,
        env: &dyn Env,
        live_wait: Option<Duration>,
        watch: impl FnOnce(&Control, &Progress) + Send,
    ) -> (Result<Output, Halt>, PathBuf) {
        let root = temp_dir(name);
        let (work, out) = (root.join("work"), root.join("out"));
        fs::create_dir_all(&work).unwrap();
        let client = Client::new("ZenithTest/1.0").unwrap();
        let headers = vec![("Referer".to_string(), "https://page.example/watch".to_string())];
        let (control, progress) = (Control::default(), Progress::default());
        let result = std::thread::scope(|s| {
            s.spawn(|| watch(&control, &progress));
            let mut ctx = Ctx::new(&client, &headers, env, &control, &progress, &work);
            ctx.live_wait = live_wait;
            let mut job = Job::new(plan, &work);
            job.run(&ctx).and_then(|()| job.finish(&ctx, &out, "Clip"))
        });
        (result, root)
    }

    fn run_plan(plan: Plan, name: &str, prepare: impl FnOnce(&Path)) -> (Result<Output, Halt>, PathBuf) {
        let root = temp_dir(name);
        let (work, out) = (root.join("work"), root.join("out"));
        fs::create_dir_all(&work).unwrap();
        prepare(&work);
        let client = Client::new("ZenithTest/1.0").unwrap();
        let headers = vec![("Referer".to_string(), "https://page.example/watch".to_string())];
        let (env, control, progress) = (TestEnv, Control::default(), Progress::default());
        let ctx = Ctx::new(&client, &headers, &env, &control, &progress, &work);
        let mut job = Job::new(plan, &work);
        let result = job.run(&ctx).and_then(|()| job.finish(&ctx, &out, "Clip"));
        (result, root)
    }

    #[test]
    fn hls_transport_stream_with_byte_ranges() {
        let body: Vec<u8> = (0..30_000u32).map(|i| (i % 251) as u8).collect();
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-PLAYLIST-TYPE:VOD\n\
            #EXTINF:4,\n#EXT-X-BYTERANGE:10000@0\nall.ts\n\
            #EXTINF:4,\n#EXT-X-BYTERANGE:10000\nall.ts\n\
            #EXTINF:4,\n#EXT-X-BYTERANGE:10000\nall.ts\n#EXT-X-ENDLIST\n";
        let mut routes = Routes::new();
        routes.insert("/v/index.m3u8".into(), ("application/vnd.apple.mpegurl".into(), playlist.as_bytes().to_vec()));
        routes.insert("/v/all.ts".into(), ("video/mp2t".into(), body.clone()));
        let server = serve(routes);
        let plan = Plan { main: Source::Hls(format!("{}/v/index.m3u8", server.base)), audio: None, live: false };
        let (result, root) = run_plan(plan, "ts", |_| {});
        let out = result.unwrap();
        assert_eq!(out.files.len(), 1);
        assert_eq!(out.files[0].file_name().unwrap(), "Clip.ts");
        assert_eq!(fs::read(&out.files[0]).unwrap(), body);
        let log = lock(&server.log).clone();
        assert!(log.iter().any(|(p, r, _)| p == "/v/all.ts" && r == "bytes=10000-19999"), "{log:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hls_fmp4_with_separate_audio_is_merged() {
        let mut routes = Routes::new();
        let video_pl = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MAP:URI=\"v/init.mp4\"\n\
            #EXTINF:2,\nv/0.m4s\n#EXTINF:2,\nv/1.m4s\n#EXTINF:2,\nv/2.m4s\n#EXT-X-ENDLIST\n";
        let mut audio_pl = String::from("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MAP:URI=\"a/init.mp4\"\n");
        for i in 0..6 {
            audio_pl.push_str(&format!("#EXTINF:1,\na/{i}.m4s\n"));
        }
        audio_pl.push_str("#EXT-X-ENDLIST\n");
        routes.insert("/s/video.m3u8".into(), ("application/x-mpegurl".into(), video_pl.as_bytes().to_vec()));
        routes.insert("/s/audio.m3u8".into(), ("application/x-mpegurl".into(), audio_pl.into_bytes()));
        routes.insert("/s/v/init.mp4".into(), ("video/mp4".into(), init(1, 90000, b"avc1", &[])));
        routes.insert("/s/a/init.mp4".into(), ("audio/mp4".into(), init(1, 48000, b"mp4a", &[])));
        for i in 0..3u64 {
            routes.insert(format!("/s/v/{i}.m4s"), ("video/mp4".into(), fragment(1, 1, i * 180_000, 180_000, &[b'V', i as u8])));
        }
        for i in 0..6u64 {
            routes.insert(format!("/s/a/{i}.m4s"), ("audio/mp4".into(), fragment(1, 1, i * 48_000, 48_000, &[b'A', i as u8])));
        }
        let server = serve(routes);
        let plan = Plan {
            main: Source::Hls(format!("{}/s/video.m3u8", server.base)),
            audio: Some(Source::Hls(format!("{}/s/audio.m3u8", server.base))),
            live: false,
        };
        let (result, root) = run_plan(plan, "fmp4", |_| {});
        let out = result.unwrap();
        assert_eq!(out.files.len(), 1, "merged into one file: {}", out.note);
        assert_eq!(out.files[0].extension().unwrap(), "mp4");
        let m = fs::read(&out.files[0]).unwrap();
        let top = mp4::children(&m, 0, m.len());
        assert_eq!(top.iter().filter(|b| &b.kind == b"moof").count(), 9);
        let moov = top.iter().find(|b| &b.kind == b"moov").unwrap();
        assert_eq!(mp4::children(&m, moov.body, moov.end).iter().filter(|b| &b.kind == b"trak").count(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dash_segments_are_appended_in_order() {
        let mut routes = Routes::new();
        let mut media = Vec::new();
        let mut expected = b"INIT".to_vec();
        routes.insert("/d/init.mp4".into(), ("video/mp4".into(), b"INIT".to_vec()));
        for i in 0..40u32 {
            let body = format!("seg{i:03};").into_bytes();
            expected.extend_from_slice(&body);
            routes.insert(format!("/d/{i}.m4s"), ("video/iso.segment".into(), body));
            media.push(SegRef { url: String::new(), range: None });
        }
        let server = serve(routes);
        for (i, m) in media.iter_mut().enumerate() {
            m.url = format!("{}/d/{i}.m4s", server.base);
        }
        let init = Some(SegRef { url: format!("{}/d/init.mp4", server.base), range: None });
        let plan = Plan { main: Source::Segments { init, media, ext: "mp4".into() }, audio: None, live: false };
        let (result, root) = run_plan(plan, "dash", |_| {});
        let out = result.unwrap();
        assert_eq!(fs::read(&out.files[0]).unwrap(), expected);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn files_resume_from_partial_data() {
        let body: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 256) as u8).collect();
        let mut routes = Routes::new();
        routes.insert("/f/movie.mp4".into(), ("video/mp4".into(), body.clone()));
        let server = serve(routes);
        let plan = Plan {
            main: Source::File { url: format!("{}/f/movie.mp4", server.base), ext: "mp4".into() },
            audio: None,
            live: false,
        };
        let prefix = body[..50_000].to_vec();
        let (result, root) = run_plan(plan, "file", move |work| fs::write(work.join("track0.part"), &prefix).unwrap());
        let out = result.unwrap();
        assert_eq!(fs::read(&out.files[0]).unwrap(), body);
        assert!(lock(&server.log).iter().any(|(_, r, _)| r == "bytes=50000-"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn encrypted_and_missing_streams_fail_cleanly() {
        let mut routes = Routes::new();
        routes.insert(
            "/e/index.m3u8".into(),
            (
                "application/vnd.apple.mpegurl".into(),
                b"#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n#EXTINF:4,\n0.ts\n#EXT-X-ENDLIST\n".to_vec(),
            ),
        );
        let server = serve(routes);
        let plan = Plan { main: Source::Hls(format!("{}/e/index.m3u8", server.base)), audio: None, live: false };
        let (result, root) = run_plan(plan, "enc", |_| {});
        assert_eq!(result.unwrap_err(), Halt::Failed(PROTECTED.into()));
        let _ = fs::remove_dir_all(root);

        let plan = Plan { main: Source::Hls(format!("{}/missing.m3u8", server.base)), audio: None, live: false };
        let (result, root) = run_plan(plan, "missing", |_| {});
        assert!(matches!(result, Err(Halt::Failed(m)) if m.contains("no longer available")));
        let _ = fs::remove_dir_all(root);
    }

    fn content_type_for(path: &str) -> &'static str {
        match path.rsplit('.').next().unwrap_or("") {
            "m3u8" => "application/vnd.apple.mpegurl",
            "mpd" => "application/dash+xml",
            "ts" => "video/mp2t",
            "mp4" | "m4s" => "video/mp4",
            _ => "application/octet-stream",
        }
    }

    struct RealEnv(Option<PathBuf>);
    impl Env for RealEnv {
        fn cookies(&self, _url: &str) -> Option<String> {
            None
        }
        fn ffmpeg(&self) -> Option<PathBuf> {
            self.0.clone()
        }
    }

    /// End to end on real streams made by FFmpeg: serves ZENITH_MEDIA_DIR over HTTP, reads
    /// ZENITH_MEDIA_ENTRY (master playlist, media playlist or MPD) the way the media panel does,
    /// downloads the default (best) choice into ZENITH_MEDIA_OUT. ZENITH_FFMPEG optionally enables
    /// FFmpeg conversion. Run with `--ignored --nocapture` and check the result with ffprobe.
    #[test]
    #[ignore]
    fn real_stream_from_env() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
        let dir = PathBuf::from(var("ZENITH_MEDIA_DIR"));
        let entry = var("ZENITH_MEDIA_ENTRY");
        let out = PathBuf::from(var("ZENITH_MEDIA_OUT"));
        let mut routes = Routes::new();
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/");
                    routes.insert(format!("/{rel}"), (content_type_for(&rel).to_string(), fs::read(&p).unwrap()));
                }
            }
        }
        let server = serve(routes);
        let url = format!("{}/{entry}", server.base);
        let client = Client::new("ZenithTest/1.0").unwrap();
        let (text, base) = client.text(&url, &|_| Vec::new(), 1 << 24).unwrap();
        let found = || crate::media::Found { by: "network", headers: vec![], page_url: "https://page.example/", title: "Real stream" };
        let mut media = crate::media::TabMedia::default();
        if entry.ends_with(".mpd") {
            media.add_dash(&url, crate::media::dash::parse(&text, &base).unwrap(), found());
        } else {
            media.add_hls(&url, hls::parse(&text, &base).unwrap(), found());
        }
        let item = media.items().remove(0);
        println!("item: {} {:?} variants={:?} audio={:?}", item.kind, item.duration, item.variants.iter().map(|v| &v.label).collect::<Vec<_>>(), item.audio_tracks.len());
        let req = media.request("t", item.id, None, None, "ZenithTest/1.0").unwrap();
        println!("plan: {:?}", req.plan);
        let work = out.join("work");
        fs::create_dir_all(&work).unwrap();
        let env = RealEnv(std::env::var_os("ZENITH_FFMPEG").map(PathBuf::from));
        let (control, progress) = (Control::default(), Progress::default());
        let ctx = Ctx::new(&client, &req.headers, &env, &control, &progress, &work);
        let mut job = Job::new(req.plan.clone(), &work);
        job.run(&ctx).unwrap();
        let output = job.finish(&ctx, &out, &req.stem).unwrap();
        println!("output: {output:?} progress: {:?}", progress.summary());
    }

    #[test]
    fn redirects_give_each_host_its_own_cookies() {
        let body: Vec<u8> = (0..100_000u32).map(|i| (i % 256) as u8).collect();
        let mut b_routes = Routes::new();
        b_routes.insert("/clip.mp4".into(), ("video/mp4".into(), body.clone()));
        let b = serve(b_routes);
        let mut a_routes = Routes::new();
        a_routes.insert("/r/clip.mp4".into(), ("redirect".into(), format!("{}/clip.mp4", b.base).into_bytes()));
        let a = serve(a_routes);
        let plan = Plan { main: Source::File { url: format!("{}/r/clip.mp4", a.base), ext: "mp4".into() }, audio: None, live: false };
        let (result, root) = run_with(plan, "redirect", &OriginCookies, None, |_, _| {});
        let out = result.unwrap();
        assert_eq!(fs::read(&out.files[0]).unwrap(), body);
        let port = |base: &str| base.rsplit(':').next().unwrap().to_string();
        let (a_log, b_log) = (lock(&a.log).clone(), lock(&b.log).clone());
        assert!(a_log.iter().all(|(_, _, c)| *c == format!("site={}", port(&a.base))), "{a_log:?}");
        assert!(!b_log.is_empty() && b_log.iter().all(|(_, _, c)| *c == format!("site={}", port(&b.base))), "{b_log:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_address_policy_applies_to_every_redirect() {
        let mut b_routes = Routes::new();
        b_routes.insert("/x".into(), ("video/mp4".into(), vec![1, 2, 3]));
        let b = serve(b_routes);
        let mut a_routes = Routes::new();
        a_routes.insert("/r".into(), ("redirect".into(), format!("{}/x", b.base).into_bytes()));
        let a = serve(a_routes);
        let b_base = b.base.clone();
        let client = Client::with_policy("ZenithTest/1.0", Box::new(move |u: &str| !u.starts_with(&b_base))).unwrap();
        let r = client.get(&format!("{}/r", a.base), &|_| Vec::new(), None, false);
        assert_eq!(r.err(), Some(http::Error::Blocked));
        assert!(lock(&b.log).is_empty(), "the blocked host was never contacted");
        // The real policy: a public page may not reach the local network; a local page may.
        assert!(!network_policy("https://news.example/story")("http://127.0.0.1:8080/v.mp4"));
        assert!(!network_policy("https://news.example/story")("http://192.168.1.1/admin"));
        assert!(network_policy("https://news.example/story")("https://cdn.example/v.mp4"));
        assert!(network_policy("http://192.168.1.20/library")("http://192.168.1.20/v.mp4"));
    }

    fn live_playlist(first: u64, last: u64, ended: bool) -> (String, Vec<u8>) {
        let mut s = format!("#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:{first}\n");
        for n in first..last {
            s.push_str(&format!("#EXTINF:2,\ns{n}.ts\n"));
        }
        if ended {
            s.push_str("#EXT-X-ENDLIST\n");
        }
        ("application/vnd.apple.mpegurl".to_string(), s.into_bytes())
    }

    #[test]
    fn a_recording_is_kept_when_the_stream_goes_away() {
        let mut routes = Routes::new();
        let mut expected = Vec::new();
        for n in 0..4u64 {
            let body = format!("seg{n};").into_bytes();
            expected.extend_from_slice(&body);
            routes.insert(format!("/live/s{n}.ts"), ("video/mp2t".into(), body));
        }
        // Two refreshes, then the playlist is gone for good (no EXT-X-ENDLIST).
        let mut seqs = HashMap::new();
        seqs.insert(
            "/live/index.m3u8".to_string(),
            vec![live_playlist(0, 3, false), live_playlist(1, 4, false), ("status/404".to_string(), Vec::new())],
        );
        let server = serve_seq(routes, seqs);
        let plan = Plan { main: Source::Hls(format!("{}/live/index.m3u8", server.base)), audio: None, live: true };
        let (result, root) = run_with(plan, "live-gone", &TestEnv, Some(Duration::from_millis(10)), |_, _| {});
        let out = result.unwrap();
        assert_eq!(fs::read(&out.files[0]).unwrap(), expected, "each segment once, in order");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stopping_a_live_recording_saves_it() {
        let mut routes = Routes::new();
        let mut expected = Vec::new();
        for n in 7..10u64 {
            let body = format!("live{n};").into_bytes();
            expected.extend_from_slice(&body);
            routes.insert(format!("/l/s{n}.ts"), ("video/mp2t".into(), body));
        }
        routes.insert("/l/index.m3u8".into(), live_playlist(7, 10, false));
        let server = serve(routes);
        let plan = Plan { main: Source::Hls(format!("{}/l/index.m3u8", server.base)), audio: None, live: true };
        let (result, root) = run_with(plan, "live-stop", &TestEnv, Some(Duration::from_millis(10)), |control, progress| {
            let until = Instant::now() + Duration::from_secs(10);
            while progress.tracks[0].done.load(Ordering::Relaxed) < 3 && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(5));
            }
            control.set(STOP);
        });
        let out = result.unwrap();
        assert_eq!(fs::read(&out.files[0]).unwrap(), expected);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn names_are_unique() {
        let dir = temp_dir("names");
        let a = reserve(&dir, "Video", "mp4").unwrap();
        let b = reserve(&dir, "Video", "mp4").unwrap();
        assert_eq!(a.file_name().unwrap(), "Video.mp4");
        assert_eq!(b.file_name().unwrap(), "Video (1).mp4");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn progress_estimates_totals_from_segments() {
        let p = Progress::default();
        // Two of ten segments appended (800 bytes); workers already hold 200 more.
        p.tracks[0].bytes.store(1000, Ordering::Relaxed);
        p.tracks[0].appended.store(800, Ordering::Relaxed);
        p.tracks[0].done.store(2, Ordering::Relaxed);
        p.tracks[0].count.store(10, Ordering::Relaxed);
        p.tracks[1].bytes.store(300, Ordering::Relaxed);
        p.tracks[1].total.store(900, Ordering::Relaxed);
        let s = p.summary();
        assert_eq!(s.bytes, 1300);
        assert_eq!(s.total, 4000 + 900, "estimated from appended segments, not in-flight bytes");
        assert_eq!((s.done, s.count), (2, 10));
        assert_eq!(format_clock(3_725_000), "1:02:05");
        assert_eq!(format_clock(65_000), "1:05");
    }
}
