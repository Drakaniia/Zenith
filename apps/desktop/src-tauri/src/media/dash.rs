//! MPEG-DASH manifests (MPD). A small tolerant XML reader plus the subset of the MPD model that
//! on-demand players use: BaseURL chains, SegmentTemplate (number or timeline addressing),
//! SegmentList and SegmentBase, with inheritance from Period and AdaptationSet.

use super::hls::resolve;

/// Upper bounds for what one manifest may expand to (a hostile manifest must not make us
/// allocate millions of URLs): per representation, in total, and representations.
const MAX_SEGMENTS: usize = 200_000;
const MAX_TOTAL_SEGMENTS: usize = 400_000;
const MAX_REPRESENTATIONS: usize = 100;

// ---------------------------------------------------------------------------------------------
// XML

#[derive(Debug, Default, Clone)]
pub struct Node {
    /// Local name (namespace prefix removed).
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        if let Some(end) = rest.find(';').filter(|&e| e <= 12) {
            let ent = &rest[1..end];
            let ch = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                    u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)
                }
                _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(c) = ch {
                out.push(c);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

fn find_from(src: &str, from: usize, pat: &str) -> Result<usize, String> {
    src.get(from..)
        .and_then(|s| s.find(pat))
        .map(|p| from + p)
        .ok_or_else(|| format!("unterminated markup (expected {pat})"))
}

/// Parses a start tag beginning after `<`. Returns the node, whether it closed itself, and the
/// position after `>`.
fn start_tag(src: &str, mut i: usize) -> Result<(Node, bool, usize), String> {
    let b = src.as_bytes();
    let ws = |c: u8| c == b' ' || c == b'\t' || c == b'\r' || c == b'\n';
    let name_start = i;
    while i < b.len() && !ws(b[i]) && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    let mut node = Node { name: local(&src[name_start..i]).to_string(), ..Default::default() };
    loop {
        while i < b.len() && ws(b[i]) {
            i += 1;
        }
        if i >= b.len() {
            return Err("unterminated tag".into());
        }
        match b[i] {
            b'>' => return Ok((node, false, i + 1)),
            b'/' => {
                let close = find_from(src, i, ">")?;
                return Ok((node, true, close + 1));
            }
            _ => {}
        }
        let an = i;
        while i < b.len() && !ws(b[i]) && b[i] != b'=' && b[i] != b'>' && b[i] != b'/' {
            i += 1;
        }
        let name = src[an..i].to_string();
        while i < b.len() && ws(b[i]) {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && ws(b[i]) {
                i += 1;
            }
            let quote = *b.get(i).ok_or("unterminated attribute")?;
            if quote == b'"' || quote == b'\'' {
                let end = find_from(src, i + 1, if quote == b'"' { "\"" } else { "'" })?;
                node.attrs.push((name, decode(&src[i + 1..end])));
                i = end + 1;
            } else {
                let vs = i;
                while i < b.len() && !ws(b[i]) && b[i] != b'>' {
                    i += 1;
                }
                node.attrs.push((name, decode(&src[vs..i])));
            }
        } else if !name.is_empty() {
            node.attrs.push((name, String::new()));
        } else {
            i += 1; // stray character: skip it
        }
    }
}

/// Parses an XML document and returns its root element.
pub fn parse_xml(src: &str) -> Result<Node, String> {
    let src = src.trim_start_matches('\u{feff}');
    let mut stack = vec![Node::default()];
    let mut i = 0;
    while i < src.len() {
        let rest = &src[i..];
        if rest.starts_with('<') {
            if rest.starts_with("<!--") {
                i = find_from(src, i + 4, "-->")? + 3;
            } else if rest.starts_with("<![CDATA[") {
                let end = find_from(src, i + 9, "]]>")?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&src[i + 9..end]);
                }
                i = end + 3;
            } else if rest.starts_with("<?") {
                i = find_from(src, i + 2, "?>")? + 2;
            } else if rest.starts_with("<!") {
                i = find_from(src, i + 2, ">")? + 1;
            } else if rest.starts_with("</") {
                i = find_from(src, i + 2, ">")? + 1;
                if stack.len() > 1 {
                    let node = stack.pop().unwrap_or_default();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    }
                }
            } else {
                let (node, closed, next) = start_tag(src, i + 1)?;
                if closed {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    }
                } else {
                    stack.push(node);
                }
                i = next;
            }
        } else {
            let end = rest.find('<').map_or(src.len(), |p| i + p);
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&decode(&src[i..end]));
            }
            i = end;
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().unwrap_or_default();
        if let Some(parent) = stack.last_mut() {
            parent.children.push(node);
        }
    }
    stack.pop().and_then(|root| root.children.into_iter().next()).ok_or_else(|| "empty document".into())
}

// ---------------------------------------------------------------------------------------------
// Durations

/// ISO 8601 durations as used by MPDs (`PT1H2M3.5S`, `P1DT2H`).
pub fn iso_duration(s: &str) -> Option<f64> {
    let rest = s.trim().strip_prefix('P')?;
    let (date, time) = match rest.split_once('T') {
        Some((d, t)) => (d, t),
        None => (rest, ""),
    };
    let mut total = 0.0;
    for (part, is_time) in [(date, false), (time, true)] {
        let mut num = String::new();
        for c in part.chars() {
            if c.is_ascii_digit() || c == '.' {
                num.push(c);
                continue;
            }
            let v: f64 = num.parse().ok()?;
            num.clear();
            total += v * match (c, is_time) {
                ('Y', false) => 365.0 * 86400.0,
                ('M', false) => 30.0 * 86400.0,
                ('W', false) => 7.0 * 86400.0,
                ('D', false) => 86400.0,
                ('H', true) => 3600.0,
                ('M', true) => 60.0,
                ('S', true) => 1.0,
                _ => return None,
            };
        }
        if !num.is_empty() {
            return None;
        }
    }
    Some(total)
}

// ---------------------------------------------------------------------------------------------
// MPD model

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
    Text,
    Other,
}

/// A segment address: URL plus an optional (offset, length) byte range.
#[derive(Debug, Clone, PartialEq)]
pub struct SegRef {
    pub url: String,
    pub range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Segments {
    /// The whole representation is one file (SegmentBase, or a bare BaseURL).
    Single(String),
    /// Explicit segments (expanded from SegmentTemplate or SegmentList).
    List { init: Option<SegRef>, media: Vec<SegRef> },
}

#[derive(Debug, Clone)]
pub struct Representation {
    /// The manifest's id (templates are already expanded with it; kept for diagnostics).
    #[allow(dead_code)]
    pub id: String,
    pub kind: Kind,
    pub mime: String,
    pub bandwidth: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
    pub lang: String,
    pub label: String,
    /// ContentProtection is declared: the media is encrypted (DRM).
    pub protected: bool,
    pub segments: Segments,
}

impl Representation {
    /// Container extension of the downloaded representation.
    pub fn container(&self) -> &'static str {
        let m = self.mime.to_ascii_lowercase();
        if m.contains("webm") {
            "webm"
        } else if self.kind == Kind::Audio {
            "m4a"
        } else {
            "mp4"
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Mpd {
    /// `type="dynamic"`: a live presentation.
    pub dynamic: bool,
    pub duration: Option<f64>,
    pub representations: Vec<Representation>,
}

impl Mpd {
    pub fn protected(&self) -> bool {
        self.representations.iter().any(|r| r.protected && matches!(r.kind, Kind::Video | Kind::Audio))
    }
    #[cfg(test)]
    pub fn videos(&self) -> impl Iterator<Item = &Representation> {
        self.representations.iter().filter(|r| r.kind == Kind::Video)
    }
    #[cfg(test)]
    pub fn audios(&self) -> impl Iterator<Item = &Representation> {
        self.representations.iter().filter(|r| r.kind == Kind::Audio)
    }
}

#[derive(Debug, Clone, Default)]
struct Template {
    media: Option<String>,
    init: Option<String>,
    start_number: Option<u64>,
    timescale: Option<u64>,
    duration: Option<u64>,
    /// (t, d, r) entries of a SegmentTimeline.
    timeline: Option<Vec<(Option<u64>, u64, i64)>>,
}

impl Template {
    fn overlay(&self, node: Option<&Node>) -> Template {
        let Some(n) = node else { return self.clone() };
        let num = |k: &str| n.attr(k).and_then(|v| v.trim().parse::<u64>().ok());
        Template {
            media: n.attr("media").map(str::to_string).or_else(|| self.media.clone()),
            init: n
                .attr("initialization")
                .map(str::to_string)
                .or_else(|| n.child("Initialization").and_then(|i| i.attr("sourceURL")).map(str::to_string))
                .or_else(|| self.init.clone()),
            start_number: num("startNumber").or(self.start_number),
            timescale: num("timescale").or(self.timescale),
            duration: num("duration").or(self.duration),
            timeline: n
                .child("SegmentTimeline")
                .map(|t| {
                    t.all("S")
                        .map(|s| {
                            let get = |k: &str| s.attr(k).and_then(|v| v.trim().parse::<i64>().ok());
                            (get("t").map(|t| t.max(0) as u64), get("d").unwrap_or(0).max(0) as u64, get("r").unwrap_or(0))
                        })
                        .collect()
                })
                .or_else(|| self.timeline.clone()),
        }
    }
}

/// Expands `$RepresentationID$`, `$Number%05d$`, `$Time$`, `$Bandwidth$` and `$$`.
pub fn fill_template(template: &str, rep_id: &str, number: u64, time: u64, bandwidth: u64) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(p) = rest.find('$') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        let Some(end) = after.find('$') else {
            out.push_str(&rest[p..]);
            return out;
        };
        let ident = &after[..end];
        if ident.is_empty() {
            out.push('$');
        } else {
            let (name, fmt) = match ident.find('%') {
                Some(i) => (&ident[..i], Some(&ident[i..])),
                None => (ident, None),
            };
            let value = match name {
                "RepresentationID" => Some(rep_id.to_string()),
                "Number" => Some(number.to_string()),
                "Time" => Some(time.to_string()),
                "Bandwidth" => Some(bandwidth.to_string()),
                _ => None,
            };
            match value {
                Some(v) => {
                    let width = fmt
                        .and_then(|f| f.strip_prefix('%'))
                        .and_then(|f| f.strip_suffix('d'))
                        .and_then(|w| w.trim_start_matches('0').parse::<usize>().ok());
                    match width {
                        Some(w) if name != "RepresentationID" => out.push_str(&format!("{v:0>w$}")),
                        _ => out.push_str(&v),
                    }
                }
                None => {
                    out.push('$');
                    out.push_str(ident);
                    out.push('$');
                }
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn expand_template(
    t: &Template,
    rep_id: &str,
    bandwidth: u64,
    base: &str,
    period: Option<f64>,
    budget: &mut usize,
) -> Option<Segments> {
    let media = t.media.as_deref()?;
    let limit = MAX_SEGMENTS.min(*budget);
    let timescale = t.timescale.unwrap_or(1).max(1);
    let start = t.start_number.unwrap_or(1);
    let init = t.init.as_deref().map(|i| SegRef {
        url: resolve(base, &fill_template(i, rep_id, start, 0, bandwidth)),
        range: None,
    });
    let mut list = Vec::new();
    let push = |list: &mut Vec<SegRef>, number: u64, time: u64| {
        list.push(SegRef { url: resolve(base, &fill_template(media, rep_id, number, time, bandwidth)), range: None });
    };
    if let Some(timeline) = &t.timeline {
        let mut time = 0u64;
        let mut number = start;
        for (i, &(t0, d, r)) in timeline.iter().enumerate() {
            if let Some(t0) = t0 {
                time = t0;
            }
            if d == 0 {
                continue;
            }
            let repeat = if r >= 0 {
                r as u64
            } else {
                // Repeat until the next entry's start, or the end of the period.
                let end = timeline
                    .get(i + 1)
                    .and_then(|n| n.0)
                    .or_else(|| period.map(|p| (p * timescale as f64).round() as u64))
                    .unwrap_or(time + d);
                (end.saturating_sub(time)).div_ceil(d).saturating_sub(1)
            };
            for _ in 0..=repeat {
                if list.len() >= limit {
                    break;
                }
                push(&mut list, number, time);
                time = time.saturating_add(d);
                number += 1;
            }
            if list.len() >= limit {
                break;
            }
        }
    } else {
        let d = t.duration.filter(|&d| d > 0)?;
        let count = ((period? * timescale as f64) / d as f64).ceil().max(1.0) as u64;
        for k in 0..count.min(limit as u64) {
            push(&mut list, start + k, k.saturating_mul(d));
        }
    }
    *budget -= list.len().min(*budget);
    Some(Segments::List { init, media: list })
}

/// `start-end` (inclusive) to (offset, length).
fn byte_range(v: Option<&str>) -> Option<(u64, u64)> {
    let (a, b) = v?.split_once('-')?;
    let (a, b): (u64, u64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (b >= a).then_some((a, b - a + 1))
}

fn segment_list(list: &Node, base: &str, budget: &mut usize) -> Segments {
    let init = list.child("Initialization").map(|i| SegRef {
        url: i.attr("sourceURL").map_or_else(|| base.to_string(), |u| resolve(base, u)),
        range: byte_range(i.attr("range")),
    });
    let media: Vec<SegRef> = list
        .all("SegmentURL")
        .take(MAX_SEGMENTS.min(*budget))
        .map(|s| SegRef {
            url: s.attr("media").map_or_else(|| base.to_string(), |u| resolve(base, u)),
            range: byte_range(s.attr("mediaRange")),
        })
        .collect();
    *budget -= media.len().min(*budget);
    Segments::List { init, media }
}

fn kind_of(content_type: &str, mime: &str, codecs: &str) -> Kind {
    let (ct, m, c) = (content_type.to_ascii_lowercase(), mime.to_ascii_lowercase(), codecs.to_ascii_lowercase());
    if ct == "video" || m.starts_with("video/") {
        return Kind::Video;
    }
    if ct == "audio" || m.starts_with("audio/") {
        return Kind::Audio;
    }
    if ct == "text" || m.starts_with("text/") || m.contains("ttml") || c.starts_with("stpp") || c.starts_with("wvtt") {
        return Kind::Text;
    }
    if ["avc", "hvc", "hev", "vp0", "vp9", "av01", "dvh"].iter().any(|p| c.starts_with(p)) {
        return Kind::Video;
    }
    if ["mp4a", "opus", "ac-3", "ec-3", "flac", "vorbis"].iter().any(|p| c.starts_with(p)) {
        return Kind::Audio;
    }
    Kind::Other
}

fn base_of(node: &Node, parent: &str) -> String {
    match node.child("BaseURL").map(|b| b.text.trim()).filter(|t| !t.is_empty()) {
        Some(b) => resolve(parent, b),
        None => parent.to_string(),
    }
}

fn frame_rate(v: Option<&str>) -> Option<f64> {
    let v = v?;
    match v.split_once('/') {
        Some((a, b)) => Some(a.trim().parse::<f64>().ok()? / b.trim().parse::<f64>().ok().filter(|d| *d > 0.0)?),
        None => v.trim().parse().ok(),
    }
}

/// Parses a manifest fetched from `url`.
pub fn parse(text: &str, url: &str) -> Result<Mpd, String> {
    let root = parse_xml(text)?;
    if root.name != "MPD" {
        return Err("not a DASH manifest".into());
    }
    let dynamic = root.attr("type").is_some_and(|t| t.eq_ignore_ascii_case("dynamic"));
    let total = root.attr("mediaPresentationDuration").and_then(iso_duration);
    let base = base_of(&root, url);

    // Pick the longest period (multi-period manifests usually split ads from the programme).
    let periods: Vec<&Node> = root.all("Period").collect();
    if periods.is_empty() {
        return Err("the manifest has no periods".into());
    }
    let starts: Vec<Option<f64>> = periods.iter().map(|p| p.attr("start").and_then(iso_duration)).collect();
    let durations: Vec<Option<f64>> = periods
        .iter()
        .enumerate()
        .map(|(i, p)| {
            p.attr("duration").and_then(iso_duration).or_else(|| {
                let start = starts[i].unwrap_or(0.0);
                match starts.get(i + 1) {
                    Some(Some(next)) => Some(next - start),
                    Some(None) => None,
                    None => total.map(|t| t - start),
                }
            })
        })
        .collect();
    let chosen = (0..periods.len())
        .max_by(|&a, &b| {
            let (da, db) = (durations[a].unwrap_or(0.0), durations[b].unwrap_or(0.0));
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal).then(b.cmp(&a))
        })
        .unwrap_or(0);
    let period = periods[chosen];
    let period_duration = durations[chosen].or(if periods.len() == 1 { total } else { None });
    let period_base = base_of(period, &base);
    let period_tpl = Template::default().overlay(period.child("SegmentTemplate"));

    let mut reps = Vec::new();
    let mut budget = MAX_TOTAL_SEGMENTS;
    for set in period.all("AdaptationSet") {
        let set_base = base_of(set, &period_base);
        let set_tpl = period_tpl.overlay(set.child("SegmentTemplate"));
        let set_protected = set.child("ContentProtection").is_some();
        for rep in set.all("Representation") {
            if reps.len() >= MAX_REPRESENTATIONS {
                break;
            }
            let rep_base = base_of(rep, &set_base);
            let get = |k: &str| rep.attr(k).or_else(|| set.attr(k)).unwrap_or("").to_string();
            let mime = get("mimeType");
            let codecs = get("codecs");
            let kind = kind_of(set.attr("contentType").unwrap_or(""), &mime, &codecs);
            if kind == Kind::Other {
                continue;
            }
            let id = rep.attr("id").unwrap_or("").to_string();
            let bandwidth = rep.attr("bandwidth").and_then(|b| b.trim().parse().ok()).unwrap_or(0);
            let tpl_node = rep.child("SegmentTemplate");
            let segments = if tpl_node.is_some() || (set_tpl.media.is_some() && rep.child("SegmentList").is_none()) {
                expand_template(&set_tpl.overlay(tpl_node), &id, bandwidth, &rep_base, period_duration, &mut budget)
            } else if let Some(list) = rep.child("SegmentList").or_else(|| set.child("SegmentList")) {
                Some(segment_list(list, &rep_base, &mut budget))
            } else {
                Some(Segments::Single(rep_base.clone()))
            };
            let Some(segments) = segments else { continue };
            let num = |k: &str| rep.attr(k).or_else(|| set.attr(k)).and_then(|v| v.trim().parse().ok());
            reps.push(Representation {
                id,
                kind,
                mime,
                bandwidth,
                width: num("width"),
                height: num("height"),
                frame_rate: frame_rate(rep.attr("frameRate").or_else(|| set.attr("frameRate"))),
                lang: set.attr("lang").unwrap_or("").to_string(),
                label: set
                    .attr("label")
                    .map(str::to_string)
                    .or_else(|| set.child("Label").map(|l| l.text.trim().to_string()))
                    .unwrap_or_default(),
                protected: set_protected || rep.child("ContentProtection").is_some(),
                segments,
            });
        }
    }
    if reps.is_empty() {
        return Err("the manifest lists no audio or video".into());
    }
    Ok(Mpd { dynamic, duration: period_duration.or(total), representations: reps })
}

#[cfg(test)]
mod tests {
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
}
