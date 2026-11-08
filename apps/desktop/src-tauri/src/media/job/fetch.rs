//! The engine's context (`Ctx`, `Env`, `Halt`) and single requests: retries with backoff, byte
//! ranges, small fetches and segment downloads.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use super::state::{Control, Progress, TrackProgress, CANCEL, RUN, STOP};
use super::{lock, PARALLEL, RETRIES};
use crate::media::dash::SegRef;
use crate::media::http::{self, Client};

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
    pub(super) abort: AtomicBool,
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

    pub(super) fn stop_now(&self) -> bool {
        self.control.get() != RUN || self.abort.load(Ordering::Relaxed)
    }

    pub(super) fn pause_for(&self, d: Duration) -> bool {
        self.control.sleep(d) && !self.abort.load(Ordering::Relaxed)
    }

    pub(super) fn halt(&self) -> Halt {
        match self.control.get() {
            CANCEL => Halt::Canceled,
            STOP => Halt::Stopped,
            RUN if self.abort.load(Ordering::Relaxed) => Halt::Failed(String::new()),
            _ => Halt::Paused,
        }
    }

    pub(super) fn aborted(&self) -> bool {
        self.abort.load(Ordering::Relaxed)
    }

    /// Request headers for `url`: the page's Referer/Origin plus the browser's cookies.
    pub(super) fn headers_for(&self, url: &str) -> Vec<(String, String)> {
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

pub(super) fn io_halt(e: io::Error) -> Halt {
    Halt::Failed(format!("Could not write the file: {e}"))
}

pub(super) fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 << attempt.min(5))
}

pub(super) fn with_retries<T>(ctx: &Ctx, mut f: impl FnMut() -> Result<T, http::Error>) -> Result<T, Halt> {
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
pub(super) fn fetch_small(ctx: &Ctx, seg: &SegRef) -> Result<Vec<u8>, Halt> {
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
pub(super) fn fetch_segment(ctx: &Ctx, p: &TrackProgress, seg: &SegRef, path: &Path) -> Result<u64, Option<http::Error>> {
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

pub(super) fn append(out: &mut File, path: &Path) -> io::Result<u64> {
    let mut f = BufReader::with_capacity(1 << 20, File::open(path)?);
    io::copy(&mut f, out)
}
