//! MPEG-DASH manifests (MPD). A small tolerant XML reader plus the subset of the MPD model that
//! on-demand players use: BaseURL chains, SegmentTemplate (number or timeline addressing),
//! SegmentList and SegmentBase, with inheritance from Period and AdaptationSet.

use super::hls::resolve;

/// Upper bounds for what one manifest may expand to (a hostile manifest must not make us
/// allocate millions of URLs): per representation, in total, and representations.
const MAX_SEGMENTS: usize = 200_000;
const MAX_TOTAL_SEGMENTS: usize = 400_000;
const MAX_REPRESENTATIONS: usize = 100;

mod xml;
#[cfg(test)]
mod tests;

use xml::{parse_xml, Node};

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
