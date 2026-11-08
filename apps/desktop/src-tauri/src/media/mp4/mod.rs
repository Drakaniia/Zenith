//! ISO BMFF (MP4) helpers for downloaded streams: DRM detection on initialisation segments and
//! merging a video-only and an audio-only *fragmented* MP4 (DASH, CMAF, HLS fMP4) into one file.
//!
//! The merge keeps every fragment byte-for-byte. It only rewrites track ids (tkhd, trex, tfhd),
//! renumbers fragments (mfhd), shifts absolute `base_data_offset`s, and interleaves the two
//! streams' fragments by decode time so players read them in order.

use std::io::{self, Read};

mod merge;
#[cfg(test)]
pub(crate) mod testkit;
#[cfg(test)]
mod tests;

pub use merge::merge;

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
    let Some((_, mut buf, total)) = read_header(r)? else { return Ok(None) };
    let total = match total {
        Some(total) => total,
        None => {
            // "Extends to the end of the file": read the rest and give it a real size.
            r.read_to_end(&mut buf)?;
            if buf.len() as u64 > u32::MAX as u64 {
                return Err(invalid("open-ended box is too large"));
            }
            let len = buf.len() as u32;
            put32(&mut buf, 0, len);
            return Ok(Some(buf));
        }
    };
    let header = buf.len() as u64;
    if total > MAX_BOX {
        return Err(invalid("corrupt box size"));
    }
    (&mut *r).take(total - header).read_to_end(&mut buf)?;
    if (buf.len() as u64) < total {
        return Err(invalid("truncated box"));
    }
    Ok(Some(buf))
}
