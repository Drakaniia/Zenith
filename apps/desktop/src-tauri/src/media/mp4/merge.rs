//! Merging a video-only and an audio-only fragmented MP4 into one file (see the parent module).

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

use super::{be32, be64, children, find, find_path, is_protected, make_box, put32, put64, read_box, read_header, skip_body, BoxAt, FourCC, MAX_BOX};

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
