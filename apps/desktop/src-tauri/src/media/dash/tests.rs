use super::*;

#[test]
fn xml_reader_handles_real_world_markup() {
    let doc = r#"<?xml version="1.0" encoding="UTF-8"?>
            <!-- generator: test -->
            <MPD xmlns="urn:mpeg:dash:schema:mpd:2011" xmlns:cenc="urn:mpeg:cenc:2013">
              <BaseURL><![CDATA[https://a.example/x?y=1&z=2]]></BaseURL>
              <Title>Tom &amp; Jerry &#x2014; &#169;</Title>
              <cenc:pssh>AAAA</cenc:pssh>
              <Empty a='1' b="two words"/>
            </MPD>"#;
    let root = parse_xml(doc).unwrap();
    assert_eq!(root.name, "MPD");
    assert_eq!(root.child("BaseURL").unwrap().text.trim(), "https://a.example/x?y=1&z=2");
    assert_eq!(root.child("Title").unwrap().text, "Tom & Jerry \u{2014} \u{a9}");
    assert_eq!(root.child("pssh").unwrap().text, "AAAA");
    let e = root.child("Empty").unwrap();
    assert_eq!(e.attr("a"), Some("1"));
    assert_eq!(e.attr("b"), Some("two words"));
    assert!(e.children.is_empty());
    assert!(parse_xml("").is_err());
}

#[test]
fn iso_durations() {
    assert_eq!(iso_duration("PT1H2M3.5S"), Some(3723.5));
    assert_eq!(iso_duration("PT0S"), Some(0.0));
    assert_eq!(iso_duration("P1DT1S"), Some(86401.0));
    assert_eq!(iso_duration("PT634.566S"), Some(634.566));
    assert_eq!(iso_duration("1:00"), None);
    assert_eq!(iso_duration("PT5X"), None);
}

#[test]
fn templates_fill_identifiers() {
    assert_eq!(fill_template("$RepresentationID$/seg-$Number%05d$.m4s", "v1", 7, 0, 0), "v1/seg-00007.m4s");
    assert_eq!(fill_template("t$Time$_b$Bandwidth$$$x.mp4", "a", 1, 9000, 128000), "t9000_b128000$x.mp4");
    assert_eq!(fill_template("$Unknown$-$Number$", "a", 3, 0, 0), "$Unknown$-3");
    assert_eq!(fill_template("broken$Number", "a", 3, 0, 0), "broken$Number");
}

const NUMBER_MPD: &str = r#"<?xml version="1.0"?>
<MPD type="static" mediaPresentationDuration="PT10S" xmlns="urn:mpeg:dash:schema:mpd:2011">
  <Period>
    <AdaptationSet mimeType="video/mp4" segmentAlignment="true">
      <SegmentTemplate media="video/$RepresentationID$/seg-$Number$.m4s" initialization="video/$RepresentationID$/init.mp4" startNumber="1" timescale="1000" duration="4000"/>
      <Representation id="720" bandwidth="2400000" width="1280" height="720" codecs="avc1.64001f" frameRate="30000/1001"/>
      <Representation id="1080" bandwidth="4800000" width="1920" height="1080" codecs="avc1.640028"/>
    </AdaptationSet>
    <AdaptationSet contentType="audio" lang="en">
      <Representation id="aac" bandwidth="128000" codecs="mp4a.40.2" mimeType="audio/mp4">
        <SegmentTemplate media="audio/$Number%03d$.m4s" initialization="audio/init.mp4" timescale="48000" duration="192000" startNumber="0"/>
      </Representation>
    </AdaptationSet>
    <AdaptationSet mimeType="image/jpeg"><Representation id="thumbs" bandwidth="1"/></AdaptationSet>
  </Period>
</MPD>"#;

#[test]
fn number_templates_expand_per_representation() {
    let mpd = parse(NUMBER_MPD, "https://cdn.example/show/manifest.mpd?sig=1").unwrap();
    assert!(!mpd.dynamic && !mpd.protected());
    assert_eq!(mpd.duration, Some(10.0));
    assert_eq!(mpd.representations.len(), 3, "thumbnails are skipped");
    let v = mpd.videos().find(|r| r.id == "1080").unwrap();
    assert_eq!((v.width, v.height), (Some(1920), Some(1080)));
    let Segments::List { init, media } = &v.segments else { panic!() };
    assert_eq!(init.as_ref().unwrap().url, "https://cdn.example/show/video/1080/init.mp4");
    assert_eq!(media.len(), 3, "10 s in 4 s segments");
    assert_eq!(media[2].url, "https://cdn.example/show/video/1080/seg-3.m4s");
    let v720 = mpd.videos().find(|r| r.id == "720").unwrap();
    assert!((v720.frame_rate.unwrap() - 29.97).abs() < 0.01);
    let a = mpd.audios().next().unwrap();
    assert_eq!(a.lang, "en");
    assert_eq!(a.container(), "m4a");
    let Segments::List { media, .. } = &a.segments else { panic!() };
    assert_eq!(media.len(), 3);
    assert_eq!(media[0].url, "https://cdn.example/show/audio/000.m4s");
    assert_eq!(media[2].url, "https://cdn.example/show/audio/002.m4s");
}

#[test]
fn timelines_with_repeats() {
    let doc = r#"<MPD type="static" mediaPresentationDuration="PT20S"><Period><BaseURL>media/</BaseURL>
          <AdaptationSet mimeType="video/mp4">
            <SegmentTemplate timescale="90000" media="v_$Time$.m4s" initialization="v_init.mp4">
              <SegmentTimeline><S t="0" d="180000" r="2"/><S d="90000"/><S d="180000" r="-1"/></SegmentTimeline>
            </SegmentTemplate>
            <Representation id="v" bandwidth="1000"/>
          </AdaptationSet></Period></MPD>"#;
    let mpd = parse(doc, "https://h.example/a/b.mpd").unwrap();
    let Segments::List { init, media } = &mpd.representations[0].segments else { panic!() };
    assert_eq!(init.as_ref().unwrap().url, "https://h.example/a/media/v_init.mp4");
    let times: Vec<String> = media.iter().map(|m| m.url.rsplit('_').next().unwrap().to_string()).collect();
    // 3 × 2 s, 1 × 1 s, then 2 s segments until 20 s: 7 s → 20 s is 13 s, i.e. 7 segments.
    assert_eq!(times.len(), 3 + 1 + 7);
    assert_eq!(times[0], "0.m4s");
    assert_eq!(times[3], "540000.m4s");
    assert_eq!(times[4], "630000.m4s");
    assert_eq!(times.last().unwrap(), "1710000.m4s");
}

#[test]
fn segment_base_and_base_url_chains() {
    let doc = r#"<MPD mediaPresentationDuration="PT1M"><BaseURL>https://media.example/root/</BaseURL><Period>
          <AdaptationSet mimeType="video/webm"><BaseURL>vid/</BaseURL>
            <Representation id="1" bandwidth="900000" height="480"><BaseURL>480.webm</BaseURL>
              <SegmentBase indexRange="200-900"><Initialization range="0-199"/></SegmentBase></Representation>
          </AdaptationSet>
          <AdaptationSet mimeType="audio/mp4"><ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011" value="cenc"/>
            <Representation id="2" bandwidth="96000"><BaseURL>/abs/a.m4a</BaseURL></Representation>
          </AdaptationSet></Period></MPD>"#;
    let mpd = parse(doc, "https://site.example/watch/manifest.mpd").unwrap();
    let v = &mpd.representations[0];
    assert_eq!(v.segments, Segments::Single("https://media.example/root/vid/480.webm".into()));
    assert_eq!(v.container(), "webm");
    assert!(!v.protected);
    let a = &mpd.representations[1];
    assert_eq!(a.segments, Segments::Single("https://media.example/abs/a.m4a".into()));
    assert!(a.protected);
    assert!(mpd.protected());
}

#[test]
fn segment_lists_with_ranges() {
    let doc = r#"<MPD mediaPresentationDuration="PT8S"><Period><AdaptationSet mimeType="audio/mp4">
          <Representation id="a" bandwidth="64000"><BaseURL>track.mp4</BaseURL>
            <SegmentList duration="4"><Initialization range="0-99"/>
              <SegmentURL mediaRange="100-4099"/><SegmentURL media="other.m4s"/></SegmentList>
          </Representation></AdaptationSet></Period></MPD>"#;
    let mpd = parse(doc, "https://x.example/p/m.mpd").unwrap();
    let Segments::List { init, media } = &mpd.representations[0].segments else { panic!() };
    assert_eq!(init.as_ref().unwrap(), &SegRef { url: "https://x.example/p/track.mp4".into(), range: Some((0, 100)) });
    assert_eq!(media[0], SegRef { url: "https://x.example/p/track.mp4".into(), range: Some((100, 4000)) });
    assert_eq!(media[1].url, "https://x.example/p/other.m4s");
}

#[test]
fn hostile_manifests_are_bounded() {
    // r='1000000000' in many representations would otherwise expand to billions of URLs.
    let reps: String = (0..150).map(|i| format!("<Representation id='r{i}' bandwidth='{i}'/>")).collect();
    let doc = format!(
        "<MPD mediaPresentationDuration='PT1000000H'><Period><AdaptationSet mimeType='video/mp4'><SegmentTemplate media='$Number$.m4s' timescale='1'><SegmentTimeline><S t='0' d='1' r='1000000000'/></SegmentTimeline></SegmentTemplate>{reps}</AdaptationSet></Period></MPD>"
    );
    let mpd = parse(&doc, "https://h.example/m.mpd").unwrap();
    assert_eq!(mpd.representations.len(), MAX_REPRESENTATIONS);
    let total: usize = mpd
        .representations
        .iter()
        .map(|r| match &r.segments {
            Segments::List { media, .. } => media.len(),
            Segments::Single(_) => 1,
        })
        .sum();
    assert!(total <= MAX_TOTAL_SEGMENTS, "{total}");
}

#[test]
fn live_and_invalid_manifests() {
    let live = r#"<MPD type="dynamic"><Period><AdaptationSet mimeType="video/mp4"><SegmentTemplate media="$Number$.m4s" duration="2" startNumber="1"/><Representation id="v" bandwidth="1"/></AdaptationSet></Period></MPD>"#;
    // A live template without a known duration cannot be expanded: nothing to download.
    assert!(parse(live, "https://l.example/m.mpd").is_err());
    assert!(parse("<html><body>no</body></html>", "https://l.example/m.mpd").is_err());
    assert!(parse("<MPD></MPD>", "https://l.example/m.mpd").is_err());
}
