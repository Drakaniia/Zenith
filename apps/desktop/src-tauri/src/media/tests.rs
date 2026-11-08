use super::detect::Sniffed;
use super::job::Source;
use super::naming::stem_from_url;
use super::tab_media::MAX_PENDING_MANIFESTS;
use super::*;

fn found(page: &str) -> Found<'_> {
    Found { by: "network", headers: vec![("Referer".into(), page.into())], page_url: page, title: "My Clip: Part 1" }
}

const MASTER: &str = "#EXTM3U\n\
        #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"English\",LANGUAGE=\"en\",DEFAULT=YES,URI=\"en.m3u8\"\n\
        #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"Français\",LANGUAGE=\"fr\",URI=\"fr.m3u8\"\n\
        #EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,AUDIO=\"a\"\n360.m3u8\n\
        #EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080,FRAME-RATE=60,AUDIO=\"a\"\n1080.m3u8\n";

#[test]
fn hls_media_playlists_fold_into_their_master() {
    let mut m = TabMedia::default();
    let page = "https://site.example/watch/1";
    let media = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6,\na.ts\n#EXTINF:4,\nb.ts\n#EXT-X-ENDLIST\n";
    // The player may fetch a variant before we have read the master.
    let early = hls::parse(media, "https://cdn.example/v/1080.m3u8").unwrap();
    assert!(m.add_hls("https://cdn.example/v/1080.m3u8", early, found(page)));
    let master = hls::parse(MASTER, "https://cdn.example/v/master.m3u8").unwrap();
    assert!(m.add_hls("https://cdn.example/v/master.m3u8", master, found(page)));
    assert_eq!(m.count(), 1, "the early variant folded into the master");
    let item = &m.items()[0];
    assert_eq!(item.duration, Some(10.0));
    assert_eq!(item.height, Some(1080));
    assert_eq!(item.variants[0].label, "1080p60 · 5.0 Mbps");
    assert_eq!(item.variants[1].label, "360p · 800 kbps");
    assert_eq!(item.audio_tracks.len(), 2);
    assert!(m.knows("https://cdn.example/v/fr.m3u8"));
    // A variant first seen after its master is still read once (it carries the duration);
    // its later refreshes are not.
    assert!(m.wants_manifest("https://cdn.example/v/360.m3u8"));
    assert!(!m.wants_manifest("https://cdn.example/v/1080.m3u8"), "folded when the master arrived");
    assert!(!m.wants_manifest("https://cdn.example/v/master.m3u8"));
    let again = hls::parse(media, "https://cdn.example/v/360.m3u8").unwrap();
    assert!(!m.add_hls("https://cdn.example/v/360.m3u8", again, found(page)), "same facts: no change");
    assert!(!m.wants_manifest("https://cdn.example/v/360.m3u8"));

    let req = m.request("t1", item.id, Some("v1"), Some("a1"), "UA/1").unwrap();
    assert_eq!(req.stem, "My Clip Part 1 (1080p)");
    assert_eq!(req.ext, "mp4");
    assert_eq!(req.plan.main, Source::Hls("https://cdn.example/v/1080.m3u8".into()));
    assert_eq!(req.plan.audio, Some(Source::Hls("https://cdn.example/v/fr.m3u8".into())));
    assert_eq!(req.user_agent, "UA/1");
    assert!(req.headers.iter().any(|(k, v)| k == "Referer" && v == page));
    let best = m.request("t1", item.id, None, None, "UA/1").unwrap();
    assert_eq!(best.plan.main, Source::Hls("https://cdn.example/v/1080.m3u8".into()));
    assert_eq!(best.plan.audio, Some(Source::Hls("https://cdn.example/v/en.m3u8".into())));
}

#[test]
fn files_are_deduplicated_and_enriched_by_scans() {
    let mut m = TabMedia::default();
    let page = "https://news.example/story";
    assert!(m.add_file("https://cdn.example/clip.mp4", "video/mp4", Some(5_000_000), found(page)));
    assert!(!m.add_file("https://cdn.example/clip.mp4", "video/mp4", Some(5_000_000), found(page)));
    let scan: Scan = serde_json::from_str(
        r#"{"title":"Story","url":"https://news.example/story","blobs":1,"drm":0,"items":[
                {"url":"https://cdn.example/clip.mp4","duration":31.5,"width":1280,"height":720},
                {"url":"https://cdn.example/other.webm","type":"video/webm","duration":12},
                {"url":"https://cdn.example/live/master.m3u8","meta":true},
                {"url":"https://player.example/embed/9","meta":true},
                {"url":"javascript:alert(1)"}]}"#,
    )
    .unwrap();
    let (changed, fetch) = m.apply_scan(&scan, page, "Story");
    assert!(changed);
    assert_eq!(fetch, vec![("https://cdn.example/live/master.m3u8".to_string(), Sniffed::Hls)]);
    // Manifest reads are budgeted per page and limited while in flight.
    for i in 0..MAX_PENDING_MANIFESTS {
        assert!(m.begin_manifest(&format!("https://cdn.example/{i}.m3u8")));
    }
    assert!(!m.begin_manifest("https://cdn.example/one-too-many.m3u8"));
    assert!(!m.wants_manifest("https://cdn.example/0.m3u8"), "already being read");
    m.pending.clear();
    assert_eq!(m.blobs, 1);
    let items = m.items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].url, "https://cdn.example/clip.mp4");
    assert_eq!(items[0].duration, Some(31.5));
    assert_eq!(items[0].height, Some(720));
    assert_eq!(items[1].ext, "webm");
    assert_eq!(items[1].found_by, "page");
    m.reset();
    assert_eq!(m.count(), 0);
}

#[test]
fn protected_and_dash_items() {
    let mut m = TabMedia::default();
    let enc = hls::parse("#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,URI=\"skd://k\"\n#EXT-X-STREAM-INF:BANDWIDTH=1\nv.m3u8\n", "https://a.example/m.m3u8").unwrap();
    m.add_hls("https://a.example/m.m3u8", enc, found("https://a.example/"));
    let id = m.items()[0].id;
    assert!(m.items()[0].protected);
    assert_eq!(m.request("t", id, None, None, "").unwrap_err(), job::PROTECTED);

    let mpd = dash::parse(
        r#"<MPD mediaPresentationDuration="PT8S"><Period>
              <AdaptationSet mimeType="video/mp4"><SegmentTemplate media="$RepresentationID$/$Number$.m4s" initialization="$RepresentationID$/i.mp4" duration="4" startNumber="1"/>
                <Representation id="hd" bandwidth="3000000" width="1280" height="720"/><Representation id="sd" bandwidth="900000" width="640" height="360"/></AdaptationSet>
              <AdaptationSet mimeType="audio/mp4" lang="en"><SegmentTemplate media="a/$Number$.m4s" initialization="a/i.mp4" duration="4" startNumber="1"/>
                <Representation id="a" bandwidth="128000"/></AdaptationSet></Period></MPD>"#,
        "https://d.example/x/manifest.mpd",
    )
    .unwrap();
    m.add_dash("https://d.example/x/manifest.mpd", mpd, found("https://d.example/"));
    let item = m.items().into_iter().find(|i| i.kind == "dash").unwrap();
    assert_eq!(item.variants.iter().map(|v| v.label.as_str()).collect::<Vec<_>>(), vec!["720p · 3.0 Mbps", "360p · 900 kbps", "Audio only · en · 128 kbps"]);
    let req = m.request("t", item.id, None, None, "").unwrap();
    let Source::Segments { media, init, .. } = &req.plan.main else { panic!() };
    assert_eq!(media.len(), 2);
    assert_eq!(init.as_ref().unwrap().url, "https://d.example/x/hd/i.mp4");
    assert!(matches!(&req.plan.audio, Some(Source::Segments { media, .. }) if media[0].url == "https://d.example/x/a/1.m4s"));
    let audio_only = m.request("t", item.id, Some(&item.variants[2].id), None, "").unwrap();
    assert!(audio_only.plan.audio.is_none());
    assert_eq!(audio_only.ext, "m4a");
    assert!(audio_only.stem.ends_with("(audio)"));
}

#[test]
fn file_names_are_windows_safe() {
    assert_eq!(sanitize_stem("  A/B: \"C\" <d>?*|  "), "A B C d");
    assert_eq!(sanitize_stem("con"), "_con");
    assert_eq!(sanitize_stem("COM3"), "_COM3");
    assert_eq!(sanitize_stem("con.x"), "_con.x");
    assert_eq!(sanitize_stem("Nul .tar"), "_Nul .tar");
    assert_eq!(sanitize_stem("LPT\u{b9}"), "_LPT\u{b9}");
    assert_eq!(sanitize_stem("Console"), "Console");
    assert_eq!(sanitize_stem("COM10"), "COM10");
    assert_eq!(sanitize_stem("Trailer..."), "Trailer");
    assert_eq!(sanitize_stem("\u{0}\u{7}"), "video");
    assert_eq!(sanitize_stem(&"x".repeat(300)).len(), 120);
    assert_eq!(stem_from_url("https://a/b/My%20Movie.mp4?x=1"), "My Movie");
    assert_eq!(stem_from_url("https://a/b/master.m3u8"), "");
    assert_eq!(quality_label(None, 0, None, false), "Default");
    assert_eq!(quality_label(Some(720), 2_500_000, Some(30.0), false), "720p · 2.5 Mbps");
    assert_eq!(quality_label(None, 96_000, None, true), "Audio only · 96 kbps");
}
