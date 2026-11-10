//! Fast mode for whole files: when the server answers byte ranges, the file is cut into pieces
//! that several connections download at once, each written at its own offset in the part file.
//! Pieces remember how far they got, so Pause/Retry continue every piece where it stopped.

use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use super::fetch::{backoff, io_halt, Ctx, Halt};
use super::state::TrackProgress;
use super::track::Track;
use super::{lock, FAST_MIN_BYTES, RETRIES};
use crate::media::http;

const PIECE_MIN: u64 = 1 << 20;
const PIECE_MAX: u64 = 64 << 20;

/// A byte range `start..end` of the file and how much of it is written.
#[derive(Debug)]
pub(super) struct Piece {
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) done: AtomicU64,
}

impl Piece {
    fn len(&self) -> u64 {
        self.end - self.start
    }
}

/// Cuts `total` bytes into pieces for `connections` workers: about four each, so connections
/// that finish early pick up the rest instead of waiting on one slow piece at the end.
pub(super) fn split(total: u64, connections: usize) -> Vec<Piece> {
    let target = total / (connections.max(1) as u64 * 4);
    let size = target.clamp(PIECE_MIN, PIECE_MAX);
    let mut pieces = Vec::new();
    let mut start = 0;
    while start < total {
        let end = (start + size).min(total);
        pieces.push(Piece { start, end, done: AtomicU64::new(0) });
        start = end;
    }
    pieces
}

/// Asks for the first byte. `Some(size)` when the server serves ranges; `None` when it doesn't
/// or refuses the request (the single-connection path then reports the real problem).
fn probe(ctx: &Ctx, url: &str) -> Result<Option<u64>, Halt> {
    let mut attempt = 0;
    loop {
        if ctx.stop_now() {
            return Err(ctx.halt());
        }
        match ctx.client.get(url, &|u| ctx.headers_for(u), Some((0, Some(0))), false) {
            Ok(r) => return Ok(if r.status == 206 { r.total } else { None }),
            Err(http::Error::Stopped) => return Err(ctx.halt()),
            Err(e) if e.retryable() && attempt < RETRIES => {
                attempt += 1;
                if !ctx.pause_for(backoff(attempt)) {
                    return Err(ctx.halt());
                }
            }
            Err(_) => return Ok(None),
        }
    }
}

/// Downloads a whole file over several connections. `Ok(false)` means it can't be split (fast
/// mode off, a small file, no range support, or a single-connection download already started)
/// and the caller should fetch it in one stream.
pub(super) fn fetch_split(ctx: &Ctx, t: &mut Track, url: &str) -> Result<bool, Halt> {
    if t.pieces.is_none() {
        let started = fs::metadata(&t.part).map(|m| m.len()).unwrap_or(0) > 0;
        if ctx.connections < 2 || started {
            return Ok(false);
        }
        let Some(total) = probe(ctx, url)?.filter(|&n| n >= FAST_MIN_BYTES) else {
            return Ok(false);
        };
        let f = OpenOptions::new().create(true).write(true).truncate(true).open(&t.part).map_err(io_halt)?;
        // Claims the space up front: a full disk fails now rather than near the end.
        f.set_len(total).map_err(io_halt)?;
        t.pieces = Some(split(total, ctx.connections));
    }
    let Some(pieces) = t.pieces.as_deref() else { return Ok(false) };
    let total = pieces.last().map_or(0, |p| p.end);
    let p = &ctx.progress.tracks[t.index];
    p.total.store(total, Ordering::Relaxed);
    p.bytes.store(pieces.iter().map(|x| x.done.load(Ordering::Relaxed)).sum(), Ordering::Relaxed);
    ctx.progress.fast.store(true, Ordering::Relaxed);

    let part = t.part.as_path();
    let next = AtomicUsize::new(0);
    let failure: Mutex<Option<String>> = Mutex::default();
    std::thread::scope(|s| {
        for _ in 0..ctx.connections.clamp(1, 16).min(pieces.len()) {
            s.spawn(|| loop {
                if ctx.stop_now() {
                    break;
                }
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(piece) = pieces.get(i) else { break };
                if let Err(e) = fetch_piece(ctx, p, part, url, piece) {
                    if let Some(e) = e {
                        ctx.abort.store(true, Ordering::Relaxed);
                        lock(&failure).get_or_insert(e.to_string());
                    }
                    break;
                }
            });
        }
    });
    if let Some(reason) = lock(&failure).take() {
        return Err(Halt::Failed(reason));
    }
    if pieces.iter().any(|x| x.done.load(Ordering::Relaxed) < x.len()) {
        return Err(ctx.halt());
    }
    t.part_len = total;
    Ok(true)
}

/// Downloads what is left of one piece, retrying network errors. `Err(None)`: asked to stop.
fn fetch_piece(ctx: &Ctx, p: &TrackProgress, part: &Path, url: &str, piece: &Piece) -> Result<(), Option<http::Error>> {
    let mut attempt = 0;
    loop {
        let done = piece.done.load(Ordering::Relaxed);
        if done >= piece.len() {
            return Ok(());
        }
        if ctx.stop_now() {
            return Err(None);
        }
        let from = piece.start + done;
        let result = (|| {
            let mut r = ctx.client.get(url, &|u| ctx.headers_for(u), Some((from, Some(piece.end - 1))), false)?;
            let io = |e: std::io::Error| http::Error::Io(e.to_string());
            let mut f = OpenOptions::new().write(true).open(part).map_err(io)?;
            f.seek(SeekFrom::Start(from)).map_err(io)?;
            // A server that stopped honouring ranges sends the whole file: skip to the piece.
            let skip = if r.status == 206 { 0 } else { from };
            r.copy_to(&mut f, skip, Some(piece.len() - done), &|| ctx.stop_now(), &mut |n| {
                piece.done.fetch_add(n, Ordering::Relaxed);
                p.bytes.fetch_add(n, Ordering::Relaxed);
            })
        })();
        // A connection that made progress before failing starts a fresh round of retries.
        if piece.done.load(Ordering::Relaxed) > done {
            attempt = 0;
        }
        match result {
            Ok(_) => {}
            Err(http::Error::Stopped) => return Err(None),
            Err(e) if e.retryable() && attempt < RETRIES => {
                attempt += 1;
                if !ctx.pause_for(backoff(attempt)) {
                    return Err(None);
                }
            }
            Err(e) => return Err(Some(e)),
        }
    }
}
