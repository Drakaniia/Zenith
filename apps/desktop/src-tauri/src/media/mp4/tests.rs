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
