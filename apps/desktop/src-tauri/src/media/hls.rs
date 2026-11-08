//! HLS playlists (RFC 8216): master playlists (variants and alternative renditions) and media
//! playlists (segments). Parsing only; fetching happens in `job`.

use tauri::Url;

/// Limits for hostile playlists (real ones stay far below): a 32 MB playlist could otherwise
/// describe millions of segments.
const MAX_SEGMENTS: usize = 200_000;
const MAX_ENTRIES: usize = 500;

fn number(v: &str) -> Option<f64> {
    v.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Resolves `uri` against the address of the document that referenced it.
pub fn resolve(base: &str, uri: &str) -> String {
    match Url::parse(base).and_then(|b| b.join(uri.trim())) {
        Ok(u) => u.to_string(),
        Err(_) => uri.trim().to_string(),
    }
}

/// One `#EXT-X-STREAM-INF` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub uri: String,
    pub bandwidth: u64,
    pub resolution: Option<(u32, u32)>,
    pub codecs: String,
    pub frame_rate: Option<f64>,
    /// GROUP-ID of the audio renditions this variant plays with.
    pub audio: Option<String>,
}

impl Variant {
    /// A variant whose codecs list only audio (or that has no video resolution and an audio codec).
    pub fn audio_only(&self) -> bool {
        let c = self.codecs.to_ascii_lowercase();
        if c.is_empty() {
            return false;
        }
        let video = ["avc", "hvc", "hev", "vp0", "vp9", "av01", "dvh", "dva", "mp4v"]
            .iter()
            .any(|v| c.contains(v));
        !video && self.resolution.is_none()
    }
}

/// One `#EXT-X-MEDIA` entry (alternative audio, subtitles, ...).
#[derive(Debug, Clone, PartialEq)]
pub struct Rendition {
    /// AUDIO, VIDEO, SUBTITLES or CLOSED-CAPTIONS.
    pub kind: String,
    pub group: String,
    pub name: String,
    pub language: String,
    pub default: bool,
    /// `None` when the rendition is carried inside the variant streams themselves.
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Master {
    pub variants: Vec<Variant>,
    pub renditions: Vec<Rendition>,
    /// `#EXT-X-SESSION-KEY` with a real method: the media is encrypted.
    pub encrypted: bool,
}

impl Master {
    /// Separate audio renditions (with their own playlist) for an audio group.
    pub fn audio_renditions(&self, group: Option<&str>) -> Vec<&Rendition> {
        let Some(group) = group else { return Vec::new() };
        self.renditions
            .iter()
            .filter(|r| r.kind == "AUDIO" && r.group == group && r.uri.is_some())
            .collect()
    }

    /// The audio rendition a player would pick for a variant: the DEFAULT one, else the first.
    pub fn default_audio(&self, variant: &Variant) -> Option<&Rendition> {
        let all = self.audio_renditions(variant.audio.as_deref());
        all.iter().find(|r| r.default).or_else(|| all.first()).copied()
    }

    /// Every playlist this master points to (variants and renditions).
    pub fn playlist_uris(&self) -> impl Iterator<Item = &str> {
        self.variants
            .iter()
            .map(|v| v.uri.as_str())
            .chain(self.renditions.iter().filter_map(|r| r.uri.as_deref()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub uri: String,
    pub duration: f64,
    /// (offset, length) for `#EXT-X-BYTERANGE` segments.
    pub range: Option<(u64, u64)>,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InitSegment {
    pub uri: String,
    pub range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaPlaylist {
    pub target_duration: f64,
    pub media_sequence: u64,
    pub segments: Vec<Segment>,
    /// `#EXT-X-MAP`: the initialisation segment of fragmented MP4 streams.
    pub init: Option<InitSegment>,
    /// The playlist switches initialisation segments (e.g. inserted ads); concatenation would
    /// produce a broken file after the switch.
    pub multiple_maps: bool,
    /// `#EXT-X-ENDLIST`: the list is complete.
    pub ended: bool,
    /// `#EXT-X-PLAYLIST-TYPE:VOD`.
    pub vod: bool,
    /// Some segment is encrypted (`#EXT-X-KEY` with a method other than NONE).
    pub encrypted: bool,
}

impl MediaPlaylist {
    pub fn duration(&self) -> f64 {
        self.segments.iter().map(|s| s.duration).sum()
    }

    /// Still growing: a live stream (or an event that has not ended yet).
    pub fn live(&self) -> bool {
        !self.ended && !self.vod
    }

    /// File extension for the concatenated segments.
    pub fn container(&self) -> &'static str {
        if self.init.is_some() {
            return "mp4";
        }
        let first = self.segments.first().map(|s| path_ext(&s.uri)).unwrap_or_default();
        match first.as_str() {
            "aac" => "aac",
            "mp3" => "mp3",
            "m4s" | "mp4" | "m4a" | "m4v" | "cmfv" | "cmfa" => "mp4",
            "webm" => "webm",
            _ => "ts",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Playlist {
    Master(Master),
    Media(MediaPlaylist),
}

/// Lower-case extension of a URL's path (no query or fragment).
pub fn path_ext(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let last = path.rsplit('/').next().unwrap_or("");
    match last.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() && ext.len() <= 5 => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Parses an attribute list: `NAME=value,NAME="quoted, value",...`.
pub fn attributes(s: &str) -> Vec<(String, String)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i] == b',' || b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i] != b'=' && b[i] != b',' {
            i += 1;
        }
        let name = s[start..i].trim().to_ascii_uppercase();
        if i >= b.len() || b[i] == b',' {
            if !name.is_empty() {
                out.push((name, String::new()));
            }
            continue;
        }
        i += 1; // '='
        let value = if i < b.len() && b[i] == b'"' {
            let vs = i + 1;
            let mut j = vs;
            while j < b.len() && b[j] != b'"' {
                j += 1;
            }
            let v = s[vs..j].to_string();
            i = j;
            while i < b.len() && b[i] != b',' {
                i += 1;
            }
            v
        } else {
            let vs = i;
            while i < b.len() && b[i] != b',' {
                i += 1;
            }
            s[vs..i].trim().to_string()
        };
        if !name.is_empty() {
            out.push((name, value));
        }
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn method_encrypts(attrs: &[(String, String)]) -> bool {
    attr(attrs, "METHOD").is_some_and(|m| !m.eq_ignore_ascii_case("NONE"))
}

/// `n[@o]`; without `@o` the range continues where the previous one (same URI) ended.
fn byterange(spec: &str, continue_at: Option<u64>) -> Option<(u64, u64)> {
    let (len, off) = match spec.trim().split_once('@') {
        Some((l, o)) => (l.trim().parse().ok()?, o.trim().parse().ok()?),
        None => (spec.trim().parse().ok()?, continue_at.unwrap_or(0)),
    };
    Some((off, len))
}

/// Parses a playlist fetched from `base`.
pub fn parse(text: &str, base: &str) -> Result<Playlist, String> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().map(str::trim).find(|l| !l.is_empty());
    if !first.is_some_and(|l| l.starts_with("#EXTM3U")) {
        return Err("not an HLS playlist".into());
    }
    if text.contains("#EXT-X-STREAM-INF") {
        Ok(Playlist::Master(parse_master(text, base)))
    } else {
        parse_media(text, base).map(Playlist::Media)
    }
}

fn parse_master(text: &str, base: &str) -> Master {
    let mut m = Master::default();
    let mut pending: Option<Vec<(String, String)>> = None;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if m.variants.len() >= MAX_ENTRIES || m.renditions.len() >= MAX_ENTRIES {
            break;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending = Some(attributes(rest));
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let a = attributes(rest);
            m.renditions.push(Rendition {
                kind: attr(&a, "TYPE").unwrap_or("").to_ascii_uppercase(),
                group: attr(&a, "GROUP-ID").unwrap_or("").to_string(),
                name: attr(&a, "NAME").unwrap_or("").to_string(),
                language: attr(&a, "LANGUAGE").unwrap_or("").to_string(),
                default: attr(&a, "DEFAULT").is_some_and(|d| d.eq_ignore_ascii_case("YES")),
                uri: attr(&a, "URI").filter(|u| !u.is_empty()).map(|u| resolve(base, u)),
            });
        } else if let Some(rest) = line.strip_prefix("#EXT-X-SESSION-KEY:") {
            if method_encrypts(&attributes(rest)) {
                m.encrypted = true;
            }
        } else if line.starts_with('#') {
            continue;
        } else if let Some(a) = pending.take() {
            let resolution = attr(&a, "RESOLUTION").and_then(|r| {
                let (w, h) = r.split_once(['x', 'X'])?;
                Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
            });
            m.variants.push(Variant {
                uri: resolve(base, line),
                bandwidth: attr(&a, "BANDWIDTH").and_then(|b| b.parse().ok()).unwrap_or(0),
                resolution,
                codecs: attr(&a, "CODECS").unwrap_or("").to_string(),
                frame_rate: attr(&a, "FRAME-RATE").and_then(number),
                audio: attr(&a, "AUDIO").filter(|g| !g.is_empty()).map(str::to_string),
            });
        }
    }
    m
}

fn parse_media(text: &str, base: &str) -> Result<MediaPlaylist, String> {
    let mut p = MediaPlaylist::default();
    let mut duration: Option<f64> = None;
    let mut pending_range: Option<String> = None;
    let mut last_end: Option<(String, u64)> = None;
    let mut key_active = false;
    let mut saw_inf = false;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(v) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            p.target_duration = number(v).unwrap_or(0.0);
        } else if let Some(v) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            if p.segments.is_empty() {
                p.media_sequence = v.trim().parse().unwrap_or(0);
            }
        } else if let Some(v) = line.strip_prefix("#EXT-X-PLAYLIST-TYPE:") {
            p.vod = v.trim().eq_ignore_ascii_case("VOD");
        } else if line.starts_with("#EXT-X-ENDLIST") {
            p.ended = true;
        } else if let Some(v) = line.strip_prefix("#EXT-X-KEY:") {
            key_active = method_encrypts(&attributes(v));
        } else if let Some(v) = line.strip_prefix("#EXT-X-MAP:") {
            let a = attributes(v);
            if let Some(u) = attr(&a, "URI") {
                let map = InitSegment {
                    uri: resolve(base, u),
                    range: attr(&a, "BYTERANGE").and_then(|r| byterange(r, None)),
                };
                match &p.init {
                    None => p.init = Some(map),
                    Some(existing) if *existing != map => p.multiple_maps = true,
                    _ => {}
                }
                if key_active {
                    p.encrypted = true;
                }
            }
        } else if let Some(v) = line.strip_prefix("#EXTINF:") {
            saw_inf = true;
            duration = Some(number(v.split(',').next().unwrap_or("0")).unwrap_or(0.0));
        } else if let Some(v) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            pending_range = Some(v.trim().to_string());
        } else if line.starts_with('#') || p.segments.len() >= MAX_SEGMENTS {
            continue;
        } else {
            let uri = resolve(base, line);
            let continue_at = last_end.as_ref().filter(|(u, _)| *u == uri).map(|(_, e)| *e);
            let range = pending_range.take().and_then(|r| byterange(&r, continue_at));
            if let Some((off, len)) = range {
                last_end = Some((uri.clone(), off + len));
            }
            if key_active {
                p.encrypted = true;
            }
            let sequence = p.media_sequence + p.segments.len() as u64;
            p.segments.push(Segment { uri, duration: duration.take().unwrap_or(0.0), range, sequence });
        }
    }
    // Plain .m3u lists (internet radio) use `#EXTINF:-1` for endless streams; HLS never does.
    if (!saw_inf && p.target_duration <= 0.0) || p.segments.iter().any(|s| s.duration < 0.0) {
        return Err("not an HLS media playlist".into());
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://cdn.example.com/video/master.m3u8?token=abc";

    #[test]
    fn attribute_lists_keep_commas_inside_quotes() {
        let a = attributes(r#"BANDWIDTH=1280000,CODECS="avc1.4d401f,mp4a.40.2",RESOLUTION=1280x720,AUDIO="aud""#);
        assert_eq!(attr(&a, "BANDWIDTH"), Some("1280000"));
        assert_eq!(attr(&a, "CODECS"), Some("avc1.4d401f,mp4a.40.2"));
        assert_eq!(attr(&a, "RESOLUTION"), Some("1280x720"));
        assert_eq!(attr(&a, "AUDIO"), Some("aud"));
        assert!(attributes("").is_empty());
        let odd = attributes(r#"URI="unterminated"#);
        assert_eq!(attr(&odd, "URI"), Some("unterminated"));
    }

    #[test]
    fn master_playlist_variants_and_audio() {
        let text = "#EXTM3U\r\n\
            #EXT-X-INDEPENDENT-SEGMENTS\r\n\
            #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aac\",NAME=\"English\",LANGUAGE=\"en\",DEFAULT=YES,URI=\"audio/en.m3u8\"\r\n\
            #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aac\",NAME=\"Deutsch\",LANGUAGE=\"de\",URI=\"audio/de.m3u8\"\r\n\
            #EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS=\"avc1.640028,mp4a.40.2\",FRAME-RATE=30,AUDIO=\"aac\"\r\n\
            1080/index.m3u8\r\n\
            #EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,AUDIO=\"aac\"\r\n\
            /abs/360.m3u8\r\n\
            #EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS=\"mp4a.40.5\"\r\n\
            https://other.example.net/audio-only.m3u8\r\n";
        let Playlist::Master(m) = parse(text, BASE).unwrap() else { panic!("expected a master playlist") };
        assert_eq!(m.variants.len(), 3);
        assert_eq!(m.variants[0].uri, "https://cdn.example.com/video/1080/index.m3u8");
        assert_eq!(m.variants[0].resolution, Some((1920, 1080)));
        assert_eq!(m.variants[0].frame_rate, Some(30.0));
        assert_eq!(m.variants[1].uri, "https://cdn.example.com/abs/360.m3u8");
        assert_eq!(m.variants[2].uri, "https://other.example.net/audio-only.m3u8");
        assert!(m.variants[2].audio_only());
        assert!(!m.variants[0].audio_only());
        assert_eq!(m.renditions.len(), 2);
        let audio = m.default_audio(&m.variants[0]).unwrap();
        assert_eq!(audio.language, "en");
        assert_eq!(audio.uri.as_deref(), Some("https://cdn.example.com/video/audio/en.m3u8"));
        assert_eq!(m.playlist_uris().count(), 5);
        assert!(!m.encrypted);
    }

    #[test]
    fn media_playlist_with_fmp4_map_and_byteranges() {
        let text = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:6\n#EXT-X-MEDIA-SEQUENCE:10\n\
            #EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-MAP:URI=\"main.mp4\",BYTERANGE=\"720@0\"\n\
            #EXTINF:6.0,\n#EXT-X-BYTERANGE:1000@720\nmain.mp4\n\
            #EXTINF:5.5,\n#EXT-X-BYTERANGE:2000\nmain.mp4\n\
            #EXT-X-ENDLIST\n";
        let Playlist::Media(p) = parse(text, "https://h.example/v/playlist.m3u8").unwrap() else { panic!() };
        assert_eq!(p.target_duration, 6.0);
        assert_eq!(p.init.as_ref().unwrap().uri, "https://h.example/v/main.mp4");
        assert_eq!(p.init.as_ref().unwrap().range, Some((0, 720)));
        assert_eq!(p.segments.len(), 2);
        assert_eq!(p.segments[0].range, Some((720, 1000)));
        assert_eq!(p.segments[1].range, Some((1720, 2000)));
        assert_eq!(p.segments[0].sequence, 10);
        assert_eq!(p.segments[1].sequence, 11);
        assert!((p.duration() - 11.5).abs() < 1e-9);
        assert!(p.ended && p.vod && !p.live() && !p.encrypted && !p.multiple_maps);
        assert_eq!(p.container(), "mp4");
    }

    #[test]
    fn transport_stream_live_playlist() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-MEDIA-SEQUENCE:2041\n\
            #EXTINF:4.000,\nseg2041.ts?x=1\n#EXTINF:4.000,\nseg2042.ts?x=1\n";
        let Playlist::Media(p) = parse(text, "http://live.example/ch/index.m3u8").unwrap() else { panic!() };
        assert!(p.live());
        assert_eq!(p.container(), "ts");
        assert_eq!(p.segments[1].uri, "http://live.example/ch/seg2042.ts?x=1");
        assert_eq!(p.segments[1].sequence, 2042);
    }

    #[test]
    fn encryption_is_detected() {
        let aes = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:6,\na.ts\n#EXT-X-ENDLIST\n";
        let Playlist::Media(p) = parse(aes, BASE).unwrap() else { panic!() };
        assert!(p.encrypted);
        let clear = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:6,\na.ts\n#EXT-X-ENDLIST\n";
        let Playlist::Media(p) = parse(clear, BASE).unwrap() else { panic!() };
        assert!(!p.encrypted);
        let session = "#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,URI=\"skd://x\"\n#EXT-X-STREAM-INF:BANDWIDTH=1\nv.m3u8\n";
        let Playlist::Master(m) = parse(session, BASE).unwrap() else { panic!() };
        assert!(m.encrypted);
    }

    #[test]
    fn hostile_numbers_and_sizes_are_contained() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:NaN\n#EXTINF:inf,\na.ts\n#EXTINF:NaN,\nb.ts\n";
        let Playlist::Media(p) = parse(text, BASE).unwrap() else { panic!() };
        assert_eq!(p.target_duration, 0.0);
        assert!(p.segments.iter().all(|s| s.duration == 0.0));
        let mut big = String::from("#EXTM3U\n#EXT-X-TARGETDURATION:1\n");
        for i in 0..MAX_SEGMENTS + 10 {
            big.push_str(&format!("#EXTINF:1,\n{i}.ts\n"));
        }
        let Playlist::Media(p) = parse(&big, BASE).unwrap() else { panic!() };
        assert_eq!(p.segments.len(), MAX_SEGMENTS);
    }

    #[test]
    fn rejects_non_playlists() {
        assert!(parse("<html>nope</html>", BASE).is_err());
        assert!(parse("#EXTM3U\n#EXTENC:UTF-8\nradio.mp3\n", BASE).is_err());
        assert!(parse("#EXTM3U\n#EXTINF:-1,Radio\nhttp://radio.example/stream\n", BASE).is_err());
        assert!(parse("", BASE).is_err());
    }

    #[test]
    fn extensions_ignore_queries() {
        assert_eq!(path_ext("https://a.b/c/seg.TS?x=1.mp4"), "ts");
        assert_eq!(path_ext("https://a.b/c/noext?x=1"), "");
        assert_eq!(path_ext("https://a.b/c.d/file"), "");
    }
}
