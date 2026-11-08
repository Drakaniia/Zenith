//! Builders for small synthetic fragmented MP4 streams (shared by the merge and engine tests).

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
