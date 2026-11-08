//! ISO BMFF (MP4) helpers for downloaded streams: DRM detection on initialisation segments and
//! merging a video-only and an audio-only *fragmented* MP4 (DASH, CMAF, HLS fMP4) into one file.
//!
//! The merge keeps every fragment byte-for-byte. It only rewrites track ids (tkhd, trex, tfhd),
//! renumbers fragments (mfhd), shifts absolute `base_data_offset`s, and interleaves the two
//! streams' fragments by decode time so players read them in order.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

pub type FourCC = [u8; 4];

/// Largest single box read into memory (a fragment is a few MB; this only guards against
/// corrupt or hostile sizes).
const MAX_BOX: u64 = 256 << 20;

fn be32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at.checked_add(4)?).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}
fn be64(b: &[u8], at: usize) -> Option<u64> {
    let s = b.get(at..at.checked_add(8)?)?;
    Some(u64::from_be_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}
fn put32(b: &mut [u8], at: usize, v: u32) {
    if let Some(s) = b.get_mut(at..at + 4) {
        s.copy_from_slice(&v.to_be_bytes());
    }
}
fn put64(b: &mut [u8], at: usize, v: u64) {
    if let Some(s) = b.get_mut(at..at + 8) {
        s.copy_from_slice(&v.to_be_bytes());
    }
}

/// A box inside a byte slice: absolute offsets of its start, payload start and end.
#[derive(Clone, Copy, Debug)]
pub struct BoxAt {
    pub kind: FourCC,
    pub start: usize,
    pub body: usize,
    pub end: usize,
}

/// The boxes laid out back to back in `data[from..to]` (stops at the first malformed one).
pub fn children(data: &[u8], from: usize, to: usize) -> Vec<BoxAt> {
    let mut out = Vec::new();
    let to = to.min(data.len());
    let mut pos = from;
    while pos + 8 <= to {
        let size = be32(data, pos).unwrap_or(0) as u64;
        let kind: FourCC = [data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]];
        let (header, size) = match size {
            1 => match be64(data, pos + 8) {
                Some(s) if pos + 16 <= to => (16usize, s),
                _ => break,
            },
            0 => (8, (to - pos) as u64),
            n => (8, n),
        };
        if size < header as u64 || pos as u64 + size > to as u64 {
            break;
        }
        let end = pos + size as usize;
        out.push(BoxAt { kind, start: pos, body: pos + header, end });
        pos = end;
    }
    out
}

fn find(data: &[u8], from: usize, to: usize, kind: &FourCC) -> Option<BoxAt> {
    children(data, from, to).into_iter().find(|b| &b.kind == kind)
}

fn find_path(data: &[u8], within: BoxAt, path: &[&FourCC]) -> Option<BoxAt> {
    let mut cur = within;
    for k in path {
        cur = find(data, cur.body, cur.end, k)?;
    }
    Some(cur)
}

pub fn make_box(kind: &FourCC, payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u64 + 8;
    let mut out = Vec::with_capacity(payload.len() + 16);
    if len <= u32::MAX as u64 {
        out.extend_from_slice(&(len as u32).to_be_bytes());
        out.extend_from_slice(kind);
    } else {
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(&(len + 8).to_be_bytes());
    }
    out.extend_from_slice(payload);
    out
}

// ---------------------------------------------------------------------------------------------
// Inspection

const ENCRYPTED_ENTRIES: [&FourCC; 5] = [b"encv", b"enca", b"enct", b"encs", b"encm"];

/// True when the header (`ftyp`+`moov`, an init segment or a whole file's start) declares
/// encrypted tracks (Common Encryption) or carries DRM system data (`pssh`).
pub fn is_protected(data: &[u8]) -> bool {
    let Some(moov) = find(data, 0, data.len(), b"moov") else { return false };
    for b in children(data, moov.body, moov.end) {
        match &b.kind {
            b"pssh" => return true,
            b"trak" => {
                if let Some(stsd) = find_path(data, b, &[b"mdia", b"minf", b"stbl", b"stsd"]) {
                    // stsd: version/flags + entry_count, then the sample entries.
                    if children(data, stsd.body + 8, stsd.end).iter().any(|e| ENCRYPTED_ENTRIES.contains(&&e.kind)) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// True when the data starts like an MP4 (ftyp, styp or moov first), as opposed to MPEG-TS etc.
pub fn looks_like_mp4(data: &[u8]) -> bool {
    matches!(data.get(4..8), Some(b"ftyp") | Some(b"styp") | Some(b"moov") | Some(b"sidx") | Some(b"moof"))
}

// ---------------------------------------------------------------------------------------------
// Streaming box reader

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Reads a box header: its type, the header bytes and the total size (`None`: to the end of the
/// file). `None` at a clean end of file.
fn read_header(r: &mut impl Read) -> io::Result<Option<(FourCC, Vec<u8>, Option<u64>)>> {
    let mut head = [0u8; 8];
    let mut got = 0;
    while got < 8 {
        let n = r.read(&mut head[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    if got == 0 {
        return Ok(None);
    }
    if got < 8 {
        return Err(invalid("truncated box header"));
    }
    let kind: FourCC = [head[4], head[5], head[6], head[7]];
    let mut bytes = head.to_vec();
    let total = match u32::from_be_bytes([head[0], head[1], head[2], head[3]]) {
        1 => {
            let mut ext = [0u8; 8];
            r.read_exact(&mut ext)?;
            bytes.extend_from_slice(&ext);
            Some(u64::from_be_bytes(ext))
        }
        0 => None,
        n => Some(n as u64),
    };
    if total.is_some_and(|t| t < bytes.len() as u64) {
        return Err(invalid("corrupt box size"));
    }
    Ok(Some((kind, bytes, total)))
}

/// Skips a box's payload after `read_header`. Returns the bytes skipped.
fn skip_body(r: &mut impl Read, header: usize, total: Option<u64>) -> io::Result<u64> {
    match total {
        Some(t) => io::copy(&mut r.take(t - header as u64), &mut io::sink()),
        None => io::copy(r, &mut io::sink()),
    }
}

/// Reads the next top-level box (header included). `None` at a clean end of file.
fn read_box(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut head = [0u8; 8];
    let mut got = 0;
    while got < 8 {
        let n = r.read(&mut head[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    if got == 0 {
        return Ok(None);
    }
    if got < 8 {
        return Err(invalid("truncated box header"));
    }
    let size32 = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    let mut buf = head.to_vec();
    let total = match size32 {
        1 => {
            let mut ext = [0u8; 8];
            r.read_exact(&mut ext)?;
            buf.extend_from_slice(&ext);
            u64::from_be_bytes(ext)
        }
        0 => {
            // "Extends to the end of the file": read the rest and give it a real size.
            r.read_to_end(&mut buf)?;
            if buf.len() as u64 > u32::MAX as u64 {
                return Err(invalid("open-ended box is too large"));
            }
            let len = buf.len() as u32;
            put32(&mut buf, 0, len);
            return Ok(Some(buf));
        }
        n => n as u64,
    };
    let header = buf.len() as u64;
    if total < header || total > MAX_BOX {
        return Err(invalid("corrupt box size"));
    }
    (&mut *r).take(total - header).read_to_end(&mut buf)?;
    if (buf.len() as u64) < total {
        return Err(invalid("truncated box"));
    }
    Ok(Some(buf))
}

// ---------------------------------------------------------------------------------------------
// Merge

struct TrackInfo {
    id: u32,
    new_id: u32,
    timescale: u32,
    default_duration: u32,
}

struct Fragment {
    /// From the `moof` through the end of its `mdat` (contiguous in the input).
    bytes: Vec<u8>,
    /// Offset of the `moof` in the input file.
    src_pos: u64,
    start: f64,
}

struct Input {
    reader: BufReader<File>,
    pos: u64,
    ftyp: Option<Vec<u8>>,
    moov: Vec<u8>,
    tracks: Vec<TrackInfo>,
    /// End time of the previous fragment, for fragments without `tfdt`.
    running: f64,
}

fn version_offset(data: &[u8], b: &BoxAt, v0: usize, v1: usize) -> usize {
    b.body + if data.get(b.body) == Some(&1) { v1 } else { v0 }
}

impl Input {
    fn open(path: &Path) -> Result<Input, String> {
        let file = File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let mut reader = BufReader::with_capacity(1 << 20, file);
        let mut pos = 0u64;
        let mut ftyp = None;
        let no_init = || "the stream has no initialisation data (moov)".to_string();
        loop {
            let Some((kind, head, total)) = read_header(&mut reader).map_err(|e| e.to_string())? else {
                return Err(no_init());
            };
            match &kind {
                b"ftyp" | b"moov" => {
                    let total = total.filter(|t| *t <= MAX_BOX).ok_or("corrupt box size")?;
                    let mut b = head;
                    let header = b.len() as u64;
                    (&mut reader).take(total - header).read_to_end(&mut b).map_err(|e| e.to_string())?;
                    if (b.len() as u64) < total {
                        return Err("truncated box".into());
                    }
                    pos += total;
                    if &kind == b"ftyp" {
                        ftyp = Some(b);
                    } else {
                        let tracks = Self::tracks(&b)?;
                        return Ok(Input { reader, pos, ftyp, moov: b, tracks, running: 0.0 });
                    }
                }
                // Media before the header: not a fragmented stream (never read it into memory).
                b"moof" | b"mdat" => return Err(no_init()),
                _ => {
                    let header = head.len();
                    pos += header as u64 + skip_body(&mut reader, header, total).map_err(|e| e.to_string())?;
                }
            }
        }
    }

    fn tracks(moov: &[u8]) -> Result<Vec<TrackInfo>, String> {
        let root = children(moov, 0, moov.len()).into_iter().next().ok_or("bad moov")?;
        let mvex = find(moov, root.body, root.end, b"mvex").ok_or("the stream is not fragmented")?;
        let trex: Vec<(u32, u32)> = children(moov, mvex.body, mvex.end)
            .into_iter()
            .filter(|b| &b.kind == b"trex")
            .filter_map(|b| Some((be32(moov, b.body + 4)?, be32(moov, b.body + 12)?)))
            .collect();
        let mut out = Vec::new();
        for trak in children(moov, root.body, root.end).into_iter().filter(|b| &b.kind == b"trak") {
            let tkhd = find(moov, trak.body, trak.end, b"tkhd").ok_or("track without tkhd")?;
            let id = be32(moov, version_offset(moov, &tkhd, 12, 20)).ok_or("bad tkhd")?;
            let mdhd = find_path(moov, trak, &[b"mdia", b"mdhd"]).ok_or("track without mdhd")?;
            let timescale = be32(moov, version_offset(moov, &mdhd, 12, 20)).ok_or("bad mdhd")?;
            let default_duration = trex.iter().find(|(t, _)| *t == id).map(|(_, d)| *d).unwrap_or(0);
            out.push(TrackInfo { id, new_id: 0, timescale, default_duration });
        }
        if out.is_empty() {
            return Err("the stream has no tracks".into());
        }
        Ok(out)
    }

    fn map(&self, id: u32) -> Option<u32> {
        self.tracks.iter().find(|t| t.id == id).map(|t| t.new_id)
    }

    fn next_fragment(&mut self) -> Result<Option<Fragment>, String> {
        let mut current: Option<(Vec<u8>, u64)> = None;
        loop {
            let at = self.pos;
            let Some(b) = read_box(&mut self.reader).map_err(|e| e.to_string())? else { return Ok(None) };
            self.pos += b.len() as u64;
            let kind: FourCC = [b[4], b[5], b[6], b[7]];
            match (&kind, current.as_mut()) {
                (b"moof", _) => current = Some((b, at)),
                (b"mdat", Some((bytes, _))) => {
                    bytes.extend_from_slice(&b);
                    let (bytes, src_pos) = current.take().unwrap_or_default();
                    let (tfdt, duration) = self.timing(&bytes);
                    let start = tfdt.unwrap_or(self.running);
                    self.running = start + duration;
                    return Ok(Some(Fragment { bytes, src_pos, start }));
                }
                // Boxes between moof and mdat stay so relative data offsets remain valid.
                (_, Some((bytes, _))) => bytes.extend_from_slice(&b),
                // styp, sidx, prft, emsg, free... between fragments are dropped.
                (_, None) => {}
            }
        }
    }

    /// Start (from `tfdt`) and duration of a fragment, in seconds.
    fn timing(&self, b: &[u8]) -> (Option<f64>, f64) {
        let Some(moof) = children(b, 0, b.len()).into_iter().next() else { return (None, 0.0) };
        for traf in children(b, moof.body, moof.end).into_iter().filter(|c| &c.kind == b"traf") {
            let Some(tfhd) = find(b, traf.body, traf.end, b"tfhd") else { continue };
            let flags = be32(b, tfhd.body).unwrap_or(0) & 0x00FF_FFFF;
            let Some(track) = be32(b, tfhd.body + 4).and_then(|id| self.tracks.iter().find(|t| t.id == id)) else {
                continue;
            };
            let mut off = tfhd.body + 8;
            if flags & 0x01 != 0 {
                off += 8;
            }
            if flags & 0x02 != 0 {
                off += 4;
            }
            let default_duration = if flags & 0x08 != 0 { be32(b, off) } else { None }.unwrap_or(track.default_duration);
            let start = find(b, traf.body, traf.end, b"tfdt").and_then(|t| {
                if b.get(t.body) == Some(&1) {
                    be64(b, t.body + 4)
                } else {
                    be32(b, t.body + 4).map(u64::from)
                }
            });
            let mut duration = 0u64;
            for trun in children(b, traf.body, traf.end).into_iter().filter(|c| &c.kind == b"trun") {
                let tf = be32(b, trun.body).unwrap_or(0) & 0x00FF_FFFF;
                let count = be32(b, trun.body + 4).unwrap_or(0) as usize;
                let mut p = trun.body + 8;
                if tf & 0x01 != 0 {
                    p += 4;
                }
                if tf & 0x04 != 0 {
                    p += 4;
                }
                let per = [0x100, 0x200, 0x400, 0x800].iter().filter(|f| tf & **f != 0).count() * 4;
                if tf & 0x100 != 0 {
                    for i in 0..count {
                        match be32(b, p + i * per) {
                            Some(d) => duration += d as u64,
                            None => break,
                        }
                    }
                } else {
                    duration += count as u64 * default_duration as u64;
                }
            }
            let ts = track.timescale.max(1) as f64;
            return (start.map(|s| s as f64 / ts), duration as f64 / ts);
        }
        (None, 0.0)
    }
}

/// Rewrites a fragment for the merged file: new track ids, sequence number and absolute offsets.
fn patch_fragment(bytes: &mut [u8], ids: &[(u32, u32)], seq: u32, delta: i64) {
    let Some(moof) = children(bytes, 0, bytes.len()).into_iter().next().filter(|b| &b.kind == b"moof") else {
        return;
    };
    for c in children(bytes, moof.body, moof.end) {
        match &c.kind {
            b"mfhd" => put32(bytes, c.body + 4, seq),
            b"traf" => {
                let Some(tfhd) = find(bytes, c.body, c.end, b"tfhd") else { continue };
                let flags = be32(bytes, tfhd.body).unwrap_or(0) & 0x00FF_FFFF;
                if let Some(new) = be32(bytes, tfhd.body + 4).and_then(|id| ids.iter().find(|(o, _)| *o == id)).map(|p| p.1) {
                    put32(bytes, tfhd.body + 4, new);
                }
                if flags & 0x01 != 0 {
                    if let Some(base) = be64(bytes, tfhd.body + 8) {
                        put64(bytes, tfhd.body + 8, (base as i64).saturating_add(delta).max(0) as u64);
                    }
                }
            }
            _ => {}
        }
    }
}

fn movie_timescale(m: &[u8]) -> Option<u32> {
    let root = children(m, 0, m.len()).into_iter().next()?;
    let mvhd = find(m, root.body, root.end, b"mvhd")?;
    be32(m, version_offset(m, &mvhd, 12, 20)).filter(|t| *t > 0)
}

/// A copied `trak` keeps durations in the *movie* timescale of the file it came from (tkhd
/// duration, edit-list segment durations). Moving it under another `mvhd` needs them rescaled, or
/// an audio priming delay of 2816/48000 s turns into 2.8 s of silence under a 1000 Hz movie clock.
fn rescale_trak(bytes: &mut [u8], from: u32, to: u32) {
    if from == to || from == 0 || to == 0 {
        return;
    }
    let scale = |v: u64| ((v as u128 * to as u128 + from as u128 / 2) / from as u128).min(u64::MAX as u128) as u64;
    let Some(trak) = children(bytes, 0, bytes.len()).into_iter().next() else { return };
    if let Some(tkhd) = find(bytes, trak.body, trak.end, b"tkhd") {
        if bytes.get(tkhd.body) == Some(&1) {
            let at = tkhd.body + 28;
            if let Some(d) = be64(bytes, at).filter(|d| *d != u64::MAX) {
                put64(bytes, at, scale(d));
            }
        } else {
            let at = tkhd.body + 20;
            if let Some(d) = be32(bytes, at).filter(|d| *d != u32::MAX) {
                put32(bytes, at, scale(d as u64).min(u32::MAX as u64 - 1) as u32);
            }
        }
    }
    if let Some(elst) = find_path(bytes, trak, &[b"edts", b"elst"]) {
        let v1 = bytes.get(elst.body) == Some(&1);
        let count = be32(bytes, elst.body + 4).unwrap_or(0) as usize;
        let size = if v1 { 20 } else { 12 };
        for i in 0..count.min(1024) {
            let at = elst.body + 8 + i * size;
            if at + size > elst.end {
                break;
            }
            if v1 {
                if let Some(d) = be64(bytes, at) {
                    put64(bytes, at, scale(d));
                }
            } else if let Some(d) = be32(bytes, at) {
                put32(bytes, at, scale(d as u64).min(u32::MAX as u64) as u32);
            }
        }
    }
}

fn merged_moov(inputs: [&Input; 2]) -> Result<Vec<u8>, String> {
    let total: u32 = inputs.iter().map(|i| i.tracks.len() as u32).sum();
    let target_timescale = movie_timescale(&inputs[0].moov).unwrap_or(1000);
    let mut payload = Vec::new();
    let mut trexes = Vec::new();
    let mut mehd: Option<Vec<u8>> = None;
    for (n, input) in inputs.iter().enumerate() {
        let m = &input.moov;
        let root = children(m, 0, m.len()).into_iter().next().ok_or("bad moov")?;
        if n == 0 {
            let mvhd = find(m, root.body, root.end, b"mvhd").ok_or("moov without mvhd")?;
            let mut mvhd = m[mvhd.start..mvhd.end].to_vec();
            let at = mvhd.len() - 4;
            put32(&mut mvhd, at, total + 1);
            payload.extend_from_slice(&mvhd);
        }
        let source_timescale = movie_timescale(m).unwrap_or(target_timescale);
        for trak in children(m, root.body, root.end).into_iter().filter(|b| &b.kind == b"trak") {
            let mut bytes = m[trak.start..trak.end].to_vec();
            let local = children(&bytes, 0, bytes.len());
            if let Some(tkhd) = local.first().and_then(|t| find(&bytes, t.body, t.end, b"tkhd")) {
                let at = version_offset(&bytes, &tkhd, 12, 20);
                if let Some(new) = be32(&bytes, at).and_then(|id| input.map(id)) {
                    put32(&mut bytes, at, new);
                }
            }
            rescale_trak(&mut bytes, source_timescale, target_timescale);
            payload.extend_from_slice(&bytes);
        }
        let mvex = find(m, root.body, root.end, b"mvex").ok_or("the stream is not fragmented")?;
        for b in children(m, mvex.body, mvex.end) {
            match &b.kind {
                b"trex" => {
                    let mut trex = m[b.start..b.end].to_vec();
                    let at = b.body - b.start + 4;
                    if let Some(new) = be32(&trex, at).and_then(|id| input.map(id)) {
                        put32(&mut trex, at, new);
                    }
                    trexes.extend_from_slice(&trex);
                }
                b"mehd" if n == 0 => mehd = Some(m[b.start..b.end].to_vec()),
                _ => {}
            }
        }
    }
    let mut mvex = mehd.unwrap_or_default();
    mvex.extend_from_slice(&trexes);
    payload.extend_from_slice(&make_box(b"mvex", &mvex));
    Ok(make_box(b"moov", &payload))
}

fn default_ftyp() -> Vec<u8> {
    make_box(b"ftyp", b"isom\x00\x00\x02\x00isomiso6mp41")
}

/// Merges a video-only and an audio-only fragmented MP4 into `out`. `cancelled` is polled
/// between fragments.
pub fn merge(video: &Path, audio: &Path, out: &Path, cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    let mut v = Input::open(video)?;
    let mut a = Input::open(audio)?;
    if is_protected(&v.moov) || is_protected(&a.moov) {
        return Err("the stream is DRM-protected".into());
    }
    let mut next_id = 1;
    for input in [&mut v, &mut a] {
        for t in input.tracks.iter_mut() {
            t.new_id = next_id;
            next_id += 1;
        }
    }
    let v_ids: Vec<(u32, u32)> = v.tracks.iter().map(|t| (t.id, t.new_id)).collect();
    let a_ids: Vec<(u32, u32)> = a.tracks.iter().map(|t| (t.id, t.new_id)).collect();
    let moov = merged_moov([&v, &a])?;
    let ftyp = v.ftyp.clone().unwrap_or_else(default_ftyp);

    let file = File::create(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    let mut w = BufWriter::with_capacity(1 << 20, file);
    let io = |e: io::Error| e.to_string();
    w.write_all(&ftyp).map_err(io)?;
    w.write_all(&moov).map_err(io)?;
    let mut written = (ftyp.len() + moov.len()) as u64;
    let mut seq = 1u32;
    let mut nv = v.next_fragment()?;
    let mut na = a.next_fragment()?;
    loop {
        if cancelled() {
            return Err("canceled".into());
        }
        let take_video = match (&nv, &na) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(x), Some(y)) => x.start <= y.start + 1e-6,
        };
        let (frag, ids) = if take_video { (nv.take(), &v_ids) } else { (na.take(), &a_ids) };
        let Some(frag) = frag else { break };
        let mut bytes = frag.bytes;
        patch_fragment(&mut bytes, ids, seq, written as i64 - frag.src_pos as i64);
        w.write_all(&bytes).map_err(io)?;
        written += bytes.len() as u64;
        seq = seq.wrapping_add(1);
        if take_video {
            nv = v.next_fragment()?;
        } else {
            na = a.next_fragment()?;
        }
    }
    w.flush().map_err(io)?;
    if seq == 1 {
        return Err("the streams contain no media fragments".into());
    }
    Ok(())
}

/// Builders for small synthetic fragmented MP4 streams (shared by the merge and engine tests).
#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub fn full(version: u8, flags: u32, rest: &[u8]) -> Vec<u8> {
        let mut v = vec![version, (flags >> 16) as u8, (flags >> 8) as u8, flags as u8];
        v.extend_from_slice(rest);
        v
    }

    pub fn mvhd(next: u32) -> Vec<u8> {
        mvhd_ts(next, 1000)
    }

    pub fn mvhd_ts(next: u32, timescale: u32) -> Vec<u8> {
        let mut p = vec![0u8; 8];
        p.extend_from_slice(&timescale.to_be_bytes());
        p.extend_from_slice(&[0u8; 4]);
        p.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        p.extend_from_slice(&[1, 0]);
        p.extend_from_slice(&[0u8; 10 + 36 + 24]);
        p.extend_from_slice(&next.to_be_bytes());
        make_box(b"mvhd", &full(0, 0, &p))
    }

    /// An init segment whose track starts with an empty edit of `delay` movie-clock ticks (as
    /// FFmpeg writes for audio priming and B-frame delay).
    pub fn init_delayed(id: u32, movie_ts: u32, media_ts: u32, entry: &FourCC, delay: u32) -> Vec<u8> {
        let mut list = 2u32.to_be_bytes().to_vec();
        for (duration, media_time) in [(delay, -1i32), (0, 0)] {
            list.extend_from_slice(&duration.to_be_bytes());
            list.extend_from_slice(&media_time.to_be_bytes());
            list.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        }
        let edts = make_box(b"edts", &make_box(b"elst", &full(0, 0, &list)));
        let plain = trak(id, media_ts, entry);
        let parts = children(&plain, 0, plain.len());
        let t = parts[0];
        let inner = children(&plain, t.body, t.end);
        let delayed = make_box(b"trak", &[plain[inner[0].start..inner[0].end].to_vec(), edts, plain[inner[1].start..inner[1].end].to_vec()].concat());
        let mut trex = id.to_be_bytes().to_vec();
        trex.extend_from_slice(&1u32.to_be_bytes());
        trex.extend_from_slice(&[0u8; 12]);
        let mvex = make_box(b"mvex", &make_box(b"trex", &full(0, 0, &trex)));
        let moov = make_box(b"moov", &[mvhd_ts(id + 1, movie_ts), delayed, mvex].concat());
        [make_box(b"ftyp", b"iso6\x00\x00\x00\x00iso6cmfc"), moov].concat()
    }

    pub fn trak(id: u32, timescale: u32, entry: &FourCC) -> Vec<u8> {
        let mut tk = vec![0u8; 8];
        tk.extend_from_slice(&id.to_be_bytes());
        tk.extend_from_slice(&[0u8; 4 + 4 + 8 + 2 + 2 + 2 + 2 + 36 + 4 + 4]);
        let tkhd = make_box(b"tkhd", &full(0, 3, &tk));
        let mut md = vec![0u8; 8];
        md.extend_from_slice(&timescale.to_be_bytes());
        md.extend_from_slice(&[0u8; 8]);
        let mdhd = make_box(b"mdhd", &full(0, 0, &md));
        let stsd = make_box(b"stsd", &[full(0, 0, &1u32.to_be_bytes()), make_box(entry, &[0u8; 8])].concat());
        let minf = make_box(b"minf", &make_box(b"stbl", &stsd));
        let mdia = make_box(b"mdia", &[mdhd, minf].concat());
        make_box(b"trak", &[tkhd, mdia].concat())
    }

    /// ftyp + moov with one track and its trex (an initialisation segment).
    pub fn init(id: u32, timescale: u32, entry: &FourCC, extra: &[u8]) -> Vec<u8> {
        let mut trex = id.to_be_bytes().to_vec();
        trex.extend_from_slice(&1u32.to_be_bytes());
        trex.extend_from_slice(&[0u8; 12]);
        let mvex = make_box(b"mvex", &make_box(b"trex", &full(0, 0, &trex)));
        let moov = make_box(b"moov", &[mvhd(id + 1), trak(id, timescale, entry), mvex, extra.to_vec()].concat());
        [make_box(b"ftyp", b"iso6\x00\x00\x00\x00iso6cmfc"), moov].concat()
    }

    /// moof (mfhd, traf(tfhd default-base-is-moof, tfdt v1, trun with offset/duration/size)) + mdat.
    pub fn fragment(seq: u32, track: u32, time: u64, duration: u32, data: &[u8]) -> Vec<u8> {
        let build = |offset: u32| {
            let mfhd = make_box(b"mfhd", &full(0, 0, &seq.to_be_bytes()));
            let tfhd = make_box(b"tfhd", &full(0, 0x02_0000, &track.to_be_bytes()));
            let tfdt = make_box(b"tfdt", &full(1, 0, &time.to_be_bytes()));
            let mut run = 1u32.to_be_bytes().to_vec();
            run.extend_from_slice(&offset.to_be_bytes());
            run.extend_from_slice(&duration.to_be_bytes());
            run.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let trun = make_box(b"trun", &full(0, 0x301, &run));
            let traf = make_box(b"traf", &[tfhd, tfdt, trun].concat());
            make_box(b"moof", &[mfhd, traf].concat())
        };
        let len = build(0).len() as u32;
        let moof = build(len + 8);
        // CMAF segments start with a segment-type box, which the merge drops.
        [make_box(b"styp", b"msdh"), moof, make_box(b"mdat", data)].concat()
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    /// A file in a folder of its own per test (tests run in parallel and clean up after themselves).
    fn temp(test: &str, name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("zenith-mp4-test-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn detects_encrypted_and_clear_tracks() {
        assert!(!is_protected(&init(1, 90000, b"avc1", &[])));
        assert!(is_protected(&init(1, 90000, b"encv", &[])));
        assert!(is_protected(&init(1, 48000, b"mp4a", &make_box(b"pssh", &full(0, 0, &[0u8; 20])))));
        assert!(!is_protected(b"\x47\x40\x00\x10 not mp4 at all"));
        assert!(looks_like_mp4(&init(1, 90000, b"avc1", &[])));
        assert!(!looks_like_mp4(&[0x47u8; 188]));
    }

    #[test]
    fn merges_and_interleaves_video_and_audio() {
        // Video: three 2 s fragments (90 kHz). Audio: six 1 s fragments (48 kHz). Both use
        // track id 1, as separate DASH/HLS renditions do.
        let mut video = init(1, 90000, b"avc1", &[]);
        for i in 0..3u64 {
            video.extend(fragment(i as u32 + 1, 1, i * 180_000, 180_000, &[b'V', i as u8, 0, 0]));
        }
        let mut audio = init(1, 48000, b"mp4a", &[]);
        audio.extend(make_box(b"sidx", &[0u8; 24]));
        for i in 0..6u64 {
            audio.extend(fragment(i as u32 + 1, 1, i * 48_000, 48_000, &[b'A', i as u8]));
        }
        let (vp, ap, out) = (temp("merge", "v.mp4"), temp("merge", "a.m4a"), temp("merge", "out.mp4"));
        std::fs::write(&vp, &video).unwrap();
        std::fs::write(&ap, &audio).unwrap();
        merge(&vp, &ap, &out, &|| false).unwrap();
        let m = std::fs::read(&out).unwrap();

        let top = children(&m, 0, m.len());
        assert_eq!(&top[0].kind, b"ftyp");
        assert_eq!(&top[1].kind, b"moov");
        let moov = top[1];
        let ids: Vec<u32> = children(&m, moov.body, moov.end)
            .into_iter()
            .filter(|b| &b.kind == b"trak")
            .map(|t| be32(&m, find(&m, t.body, t.end, b"tkhd").unwrap().body + 12).unwrap())
            .collect();
        assert_eq!(ids, vec![1, 2]);
        let mvex = find(&m, moov.body, moov.end, b"mvex").unwrap();
        let trex: Vec<u32> = children(&m, mvex.body, mvex.end).iter().map(|t| be32(&m, t.body + 4).unwrap()).collect();
        assert_eq!(trex, vec![1, 2]);
        let mvhd = find(&m, moov.body, moov.end, b"mvhd").unwrap();
        assert_eq!(be32(&m, mvhd.end - 4), Some(3), "next_track_ID");

        // Fragments: interleaved by time, renumbered, track ids remapped, data offsets intact.
        let mut order = Vec::new();
        for (n, moof) in top.iter().filter(|b| &b.kind == b"moof").enumerate() {
            let mfhd = find(&m, moof.body, moof.end, b"mfhd").unwrap();
            assert_eq!(be32(&m, mfhd.body + 4), Some(n as u32 + 1));
            let traf = find(&m, moof.body, moof.end, b"traf").unwrap();
            let id = be32(&m, find(&m, traf.body, traf.end, b"tfhd").unwrap().body + 4).unwrap();
            let trun = find(&m, traf.body, traf.end, b"trun").unwrap();
            let offset = be32(&m, trun.body + 8).unwrap() as usize;
            let data = &m[moof.start + offset..moof.start + offset + 2];
            order.push((id, data[0], data[1]));
        }
        assert_eq!(
            order,
            vec![(1, b'V', 0), (2, b'A', 0), (2, b'A', 1), (1, b'V', 1), (2, b'A', 2), (2, b'A', 3), (1, b'V', 2), (2, b'A', 4), (2, b'A', 5)]
        );
        assert!(top.iter().all(|b| &b.kind != b"sidx" && &b.kind != b"styp"), "segment boxes are dropped");
        let _ = std::fs::remove_dir_all(vp.parent().unwrap());
    }

    #[test]
    fn edit_lists_keep_their_meaning_under_the_new_movie_clock() {
        // Video: movie clock 1000 Hz, 80 ms delay. Audio: movie clock 48 kHz, 2816 ticks (58.7 ms).
        let mut video = init_delayed(1, 1000, 12800, b"avc1", 80);
        video.extend(fragment(1, 1, 0, 25_600, b"V0"));
        let mut audio = init_delayed(1, 48000, 48000, b"mp4a", 2816);
        audio.extend(fragment(1, 1, 0, 96_000, b"A0"));
        let (vp, ap, out) = (temp("edits", "v.mp4"), temp("edits", "a.m4a"), temp("edits", "out.mp4"));
        std::fs::write(&vp, &video).unwrap();
        std::fs::write(&ap, &audio).unwrap();
        merge(&vp, &ap, &out, &|| false).unwrap();
        let m = std::fs::read(&out).unwrap();
        let moov = children(&m, 0, m.len()).into_iter().find(|b| &b.kind == b"moov").unwrap();
        let delays: Vec<u32> = children(&m, moov.body, moov.end)
            .into_iter()
            .filter(|b| &b.kind == b"trak")
            .map(|t| be32(&m, find_path(&m, t, &[b"edts", b"elst"]).unwrap().body + 8).unwrap())
            .collect();
        assert_eq!(delays, vec![80, 59], "2816/48000 s is 59 ms on the merged 1000 Hz clock");
        let _ = std::fs::remove_dir_all(vp.parent().unwrap());
    }

    #[test]
    fn refuses_protected_or_unfragmented_input() {
        let (vp, ap, out) = (temp("refuse", "v.mp4"), temp("refuse", "a.m4a"), temp("refuse", "out.mp4"));
        let mut video = init(1, 90000, b"encv", &[]);
        video.extend(fragment(1, 1, 0, 90_000, b"V0"));
        let mut audio = init(1, 48000, b"mp4a", &[]);
        audio.extend(fragment(1, 1, 0, 48_000, b"A0"));
        std::fs::write(&vp, &video).unwrap();
        std::fs::write(&ap, &audio).unwrap();
        assert!(merge(&vp, &ap, &out, &|| false).unwrap_err().contains("DRM"));
        std::fs::write(&vp, make_box(b"moov", &mvhd(2))).unwrap();
        assert!(merge(&vp, &ap, &out, &|| false).is_err());
        std::fs::write(&vp, [0x47u8; 376]).unwrap();
        assert!(merge(&vp, &ap, &out, &|| false).is_err());
        let _ = std::fs::remove_dir_all(vp.parent().unwrap());
    }

    /// Merges real files (e.g. FFmpeg's `-f dash` or `-hls_segment_type fmp4` output, init plus
    /// segments concatenated): set ZENITH_MP4_V, ZENITH_MP4_A and ZENITH_MP4_OUT, run with
    /// `--ignored`, and inspect the result with ffprobe.
    #[test]
    #[ignore]
    fn merges_real_files_from_env() {
        let get = |k: &str| std::env::var_os(k).map(std::path::PathBuf::from).expect(k);
        merge(&get("ZENITH_MP4_V"), &get("ZENITH_MP4_A"), &get("ZENITH_MP4_OUT"), &|| false).unwrap();
    }
}
