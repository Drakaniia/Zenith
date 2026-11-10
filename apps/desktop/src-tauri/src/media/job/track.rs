//! A track of a job and how it is downloaded: HLS media playlists, explicit segment lists
//! (fetched in parallel and appended in order) and whole files with resume.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use super::fetch::{append, backoff, fetch_segment, fetch_small, io_halt, with_retries, Ctx, Halt};
use super::ranged::{fetch_split, Piece};
use super::state::TrackProgress;
use super::{lock, Source, PROTECTED, RETRIES};
use crate::media::dash::SegRef;
use crate::media::hls::{self, Playlist};
use crate::media::http;
use crate::media::mp4;

// ---------------------------------------------------------------------------------------------
// The engine

pub(super) struct Track {
    pub(super) index: usize,
    pub(super) source: Source,
    /// The assembled output so far.
    pub(super) part: PathBuf,
    pub(super) part_len: u64,
    /// VOD: number of segments appended.
    pub(super) appended: usize,
    pub(super) resolved: Option<(Option<SegRef>, Vec<SegRef>)>,
    /// Live: media sequence number of the last appended segment.
    pub(super) last_seq: Option<u64>,
    /// Live: recently recorded segment addresses (to notice a restarted sequence).
    pub(super) recent: VecDeque<String>,
    pub(super) init_done: bool,
    /// Finished; a later run (after the other track paused or failed) leaves it alone.
    pub(super) complete: bool,
    /// Container of the output ("mp4", "ts", "webm", ...).
    pub(super) ext: String,
    /// The output is (fragmented) MP4.
    pub(super) mp4: bool,
    /// Whole files in fast mode: the byte ranges being fetched over several connections.
    pub(super) pieces: Option<Vec<Piece>>,
}

impl Track {
    pub(super) fn new(index: usize, source: Source, work: &Path) -> Track {
        let ext = match &source {
            Source::Segments { ext, .. } | Source::File { ext, .. } | Source::Download { ext, .. } => ext.clone(),
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
            pieces: None,
        }
    }

    /// The assembled part vanished (an earlier finish moved it, then failed): start over rather
    /// than pad an empty file to the old length.
    pub(super) fn recover(&mut self, p: &TrackProgress) {
        if self.part_len > 0 && !self.part.exists() {
            self.part_len = 0;
            self.appended = 0;
            self.init_done = false;
            self.last_seq = None;
            self.recent.clear();
            self.complete = false;
            self.pieces = None;
            for c in [&p.bytes, &p.done, &p.appended, &p.recorded_ms] {
                c.store(0, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn open_part(&self) -> Result<File, Halt> {
        let mut f = OpenOptions::new().create(true).read(true).write(true).truncate(false).open(&self.part).map_err(io_halt)?;
        // Drop anything appended after the last complete segment (an interrupted write).
        f.set_len(self.part_len).map_err(io_halt)?;
        f.seek(SeekFrom::End(0)).map_err(io_halt)?;
        Ok(f)
    }

    pub(super) fn write_init(&mut self, ctx: &Ctx, out: &mut File, init: &SegRef) -> Result<(), Halt> {
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
pub(super) fn media_playlist(ctx: &Ctx, url: &str) -> Result<hls::MediaPlaylist, Halt> {
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
pub(super) fn download(ctx: &Ctx, t: &mut Track) -> Result<(), Halt> {
    t.recover(&ctx.progress.tracks[t.index]);
    if t.complete {
        return Ok(());
    }
    let (init, media) = match t.source.clone() {
        Source::Download { url, .. } => {
            if !fetch_split(ctx, t, &url)? {
                fetch_file(ctx, t, &url)?;
            }
            return Ok(());
        }
        Source::File { url, .. } => {
            if !fetch_split(ctx, t, &url)? {
                fetch_file(ctx, t, &url)?;
            }
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
