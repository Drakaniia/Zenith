//! Built-in media downloader: finds the videos and audio that pages play (progressive files,
//! HLS and DASH streams), lists them per tab, and downloads them into the regular downloads list.
//!
//! - `sniff` watches every tab's network responses and reads stream manifests; it also scans the
//!   page (`<video>`/`<audio>`, `og:video`) on demand and serves cookies to downloads.
//! - `TabMedia` (here) is a tab's list of what was found, with HLS media playlists folded into
//!   their master playlist.
//! - `job` downloads an item: plain files with resume, segmented streams in parallel, live HLS
//!   recording, and merging of separate audio and video (natively for fragmented MP4, otherwise
//!   with FFmpeg when it is installed).
//! - DRM-protected media (Encrypted Media Extensions, encrypted HLS, DASH ContentProtection,
//!   Common Encryption) is detected and never downloaded.

pub mod dash;
pub mod detect;
pub mod hls;
pub mod http;
pub mod job;
pub mod mp4;
pub mod sniff;

use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use detect::Sniffed;
use job::{Plan, Request, Source};

/// Items kept per tab (pages that load dozens of clips keep the most recent).
const MAX_ENTRIES: usize = 60;
/// Manifests read per page, and at the same time: a hostile page could otherwise make Zenith
/// read and parse an endless stream of playlists.
const MAX_MANIFEST_READS: u32 = 200;
const MAX_PENDING_MANIFESTS: usize = 8;

/// A quality or audio-language choice offered for a stream.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub height: Option<u32>,
    pub bandwidth: Option<u64>,
    pub audio_only: bool,
}

/// One downloadable item as the media panel shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: u32,
    /// "file" | "hls" | "dash"
    pub kind: &'static str,
    pub url: String,
    pub title: String,
    pub ext: String,
    pub size: Option<u64>,
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub audio_only: bool,
    pub live: bool,
    pub protected: bool,
    /// "network" (seen loading) or "page" (found in the document).
    pub found_by: &'static str,
    pub detected: i64,
    pub variants: Vec<Choice>,
    pub audio_tracks: Vec<Choice>,
    /// Why the item can't be downloaded, or other remarks.
    pub note: String,
}

pub enum Manifest {
    None,
    HlsMaster(hls::Master),
    HlsMedia(hls::MediaPlaylist),
    Dash(dash::Mpd),
}

pub struct Entry {
    pub item: MediaItem,
    pub manifest: Manifest,
    /// Request headers worth replaying (Referer, Origin, User-Agent).
    pub headers: Vec<(String, String)>,
    pub page_url: String,
    /// Playlists of an HLS master, so their own responses fold into it.
    children: Vec<String>,
    /// Children whose details were folded in already (later refreshes are not read again).
    folded: Vec<String>,
}

/// Where an item was found and on which page.
pub struct Found<'a> {
    pub by: &'static str,
    pub headers: Vec<(String, String)>,
    pub page_url: &'a str,
    pub title: &'a str,
}

/// Result of the page scan script (`detect::SCAN_SCRIPT`); untrusted.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Scan {
    pub title: String,
    pub url: String,
    pub blobs: u32,
    pub drm: u32,
    pub items: Vec<ScanItem>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct ScanItem {
    pub url: String,
    pub audio: bool,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub meta: bool,
    #[serde(rename = "type")]
    pub mime: String,
}

#[derive(Default)]
pub struct TabMedia {
    pub entries: Vec<Entry>,
    next_id: u32,
    /// Bumped on every navigation; late async results for an older page are dropped.
    pub epoch: u64,
    /// Recent requests made by `<video>`/`<audio>` elements.
    media_requests: VecDeque<String>,
    /// Manifests being read.
    pub pending: HashSet<String>,
    /// Manifests read since the page loaded.
    manifest_reads: u32,
    /// Players on the page that stream through Media Source (`blob:` sources).
    pub blobs: u32,
    /// Players using Encrypted Media Extensions (DRM).
    pub drm: u32,
}

pub fn format_bitrate(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.1} Mbps", bps as f64 / 1e6)
    } else {
        format!("{} kbps", (bps + 500) / 1000)
    }
}

/// "1080p60 · 6.2 Mbps", "Audio only · 128 kbps".
pub fn quality_label(height: Option<u32>, bandwidth: u64, fps: Option<f64>, audio_only: bool) -> String {
    let mut parts = Vec::new();
    if audio_only {
        parts.push("Audio only".to_string());
    } else if let Some(h) = height.filter(|h| *h > 0) {
        match fps.filter(|f| *f > 31.0) {
            Some(f) => parts.push(format!("{h}p{}", f.round() as u32)),
            None => parts.push(format!("{h}p")),
        }
    }
    if bandwidth > 0 {
        parts.push(format_bitrate(bandwidth));
    }
    if parts.is_empty() {
        "Default".into()
    } else {
        parts.join(" · ")
    }
}

/// A Windows-safe file name (without extension) from a page or media title.
pub fn sanitize_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let mut s = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > 120 {
        s = s.chars().take(120).collect();
    }
    let s = s.trim_end_matches(['.', ' ']).trim().to_string();
    // Windows reserves device names even with an extension: "nul.tar.mp4" is NUL.
    let upper = s.to_ascii_uppercase();
    let base: Vec<char> = upper.split('.').next().unwrap_or("").trim_end().chars().collect();
    let name: String = base.iter().collect();
    let device = ["CON", "PRN", "AUX", "NUL"].contains(&name.as_str())
        || ((name.starts_with("COM") || name.starts_with("LPT"))
            && base.len() == 4
            && (base[3].is_ascii_digit() || matches!(base[3], '\u{b9}' | '\u{b2}' | '\u{b3}')));
    if s.is_empty() {
        "video".into()
    } else if device {
        format!("_{s}")
    } else {
        s
    }
}

/// A file name stem from the media URL, when the page has no usable title.
fn stem_from_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let last = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let stem = last.rsplit_once('.').map_or(last, |(s, _)| s);
    let decoded = stem.replace("%20", " ");
    let generic = ["", "index", "master", "playlist", "manifest", "video", "chunklist", "stream", "media"];
    if generic.contains(&decoded.to_ascii_lowercase().as_str()) {
        String::new()
    } else {
        decoded
    }
}

pub fn mime_for(ext: &str, audio: bool) -> String {
    match ext {
        "mp4" | "m4v" => "video/mp4",
        "m4a" => "audio/mp4",
        "ts" => "video/mp2t",
        "webm" if audio => "audio/webm",
        "webm" => "video/webm",
        "weba" => "audio/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "aac" => "audio/aac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "ogv" => "video/ogg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        _ if audio => "audio/mpeg",
        _ => "video/mp4",
    }
    .to_string()
}

fn now() -> i64 {
    crate::store::now_ms()
}

fn hls_master_item(m: &hls::Master) -> (Vec<Choice>, Vec<Choice>, Option<(u32, u32)>) {
    let mut seen = HashSet::new();
    let mut variants: Vec<(usize, &hls::Variant)> = m
        .variants
        .iter()
        .enumerate()
        .filter(|(_, v)| seen.insert((v.resolution, v.bandwidth, v.audio_only())))
        .collect();
    variants.sort_by(|(_, a), (_, b)| {
        a.audio_only()
            .cmp(&b.audio_only())
            .then(b.resolution.map(|r| r.1).cmp(&a.resolution.map(|r| r.1)))
            .then(b.bandwidth.cmp(&a.bandwidth))
    });
    let best = variants.iter().find(|(_, v)| !v.audio_only()).and_then(|(_, v)| v.resolution);
    let choices = variants
        .iter()
        .map(|(i, v)| Choice {
            id: format!("v{i}"),
            label: quality_label(v.resolution.map(|r| r.1), v.bandwidth, v.frame_rate, v.audio_only()),
            height: v.resolution.map(|r| r.1),
            bandwidth: Some(v.bandwidth).filter(|b| *b > 0),
            audio_only: v.audio_only(),
        })
        .collect();
    let mut langs = HashSet::new();
    let audio: Vec<Choice> = m
        .renditions
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == "AUDIO" && r.uri.is_some())
        .filter(|(_, r)| langs.insert((r.language.clone(), r.name.clone())))
        .map(|(i, r)| Choice {
            id: format!("a{i}"),
            label: match (r.name.is_empty(), r.language.is_empty()) {
                (false, _) => r.name.clone(),
                (true, false) => r.language.clone(),
                _ => format!("Track {}", i + 1),
            },
            height: None,
            bandwidth: None,
            audio_only: true,
        })
        .collect();
    (choices, if audio.len() > 1 { audio } else { Vec::new() }, best)
}

fn dash_choices(mpd: &dash::Mpd) -> (Vec<Choice>, Vec<Choice>, Option<(u32, u32)>) {
    let reps = &mpd.representations;
    let mut videos: Vec<usize> = (0..reps.len()).filter(|&i| reps[i].kind == dash::Kind::Video && !reps[i].protected).collect();
    videos.sort_by(|&a, &b| reps[b].height.cmp(&reps[a].height).then(reps[b].bandwidth.cmp(&reps[a].bandwidth)));
    // Best audio representation per language.
    let mut audios: Vec<usize> = Vec::new();
    for i in (0..reps.len()).filter(|&i| reps[i].kind == dash::Kind::Audio && !reps[i].protected) {
        match audios.iter().position(|&j| reps[j].lang == reps[i].lang) {
            Some(k) if reps[audios[k]].bandwidth < reps[i].bandwidth => audios[k] = i,
            Some(_) => {}
            None => audios.push(i),
        }
    }
    let best = videos.first().and_then(|&i| Some((reps[i].width?, reps[i].height?)));
    let mut variants: Vec<Choice> = videos
        .iter()
        .map(|&i| Choice {
            id: format!("r{i}"),
            label: quality_label(reps[i].height, reps[i].bandwidth, reps[i].frame_rate, false),
            height: reps[i].height,
            bandwidth: Some(reps[i].bandwidth).filter(|b| *b > 0),
            audio_only: false,
        })
        .collect();
    variants.extend(audios.iter().map(|&i| {
        let lang = if reps[i].lang.is_empty() { String::new() } else { format!(" · {}", reps[i].lang) };
        Choice {
            id: format!("r{i}"),
            label: format!("Audio only{lang} · {}", format_bitrate(reps[i].bandwidth)),
            height: None,
            bandwidth: Some(reps[i].bandwidth).filter(|b| *b > 0),
            audio_only: true,
        }
    }));
    let tracks = if audios.len() > 1 {
        audios
            .iter()
            .map(|&i| Choice {
                id: format!("r{i}"),
                label: [reps[i].label.as_str(), reps[i].lang.as_str()]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .unwrap_or("Audio")
                    .to_string(),
                height: None,
                bandwidth: Some(reps[i].bandwidth),
                audio_only: true,
            })
            .collect()
    } else {
        Vec::new()
    };
    (variants, tracks, best)
}

fn fold_media_info(item: &mut MediaItem, p: &hls::MediaPlaylist) -> bool {
    let before = (item.duration, item.live, item.protected);
    item.live = p.live();
    item.duration = if item.live { None } else { Some(p.duration()).filter(|d| *d > 0.0) };
    item.protected |= p.encrypted;
    before != (item.duration, item.live, item.protected)
}

impl TabMedia {
    /// A new page: forget everything.
    pub fn reset(&mut self) {
        self.entries.clear();
        self.media_requests.clear();
        self.pending.clear();
        self.manifest_reads = 0;
        self.blobs = 0;
        self.drm = 0;
        self.epoch += 1;
    }

    /// Claims a manifest read, unless too many are running or this page already had its share.
    pub fn begin_manifest(&mut self, url: &str) -> bool {
        if self.pending.len() >= MAX_PENDING_MANIFESTS || self.manifest_reads >= MAX_MANIFEST_READS || self.pending.contains(url) {
            return false;
        }
        self.manifest_reads += 1;
        self.pending.insert(url.to_string());
        true
    }

    pub fn note_media_request(&mut self, url: &str) {
        if url.len() > 4096 || self.media_requests.iter().any(|u| u == url) {
            return;
        }
        self.media_requests.push_back(url.to_string());
        if self.media_requests.len() > 64 {
            self.media_requests.pop_front();
        }
    }

    pub fn is_media_request(&self, url: &str) -> bool {
        self.media_requests.iter().any(|u| u == url)
    }

    fn find_mut(&mut self, url: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.item.url == url)
    }

    /// Already listed, part of a listed master, or being read.
    pub fn knows(&self, url: &str) -> bool {
        self.pending.contains(url) || self.entries.iter().any(|e| e.item.url == url || e.children.iter().any(|c| c == url))
    }

    /// Whether a manifest response is worth reading: not being read, not listed, and not a
    /// variant playlist already folded into its master. A variant seen for the first time is
    /// read even though its master is listed: it carries the duration and live/encrypted state.
    pub fn wants_manifest(&self, url: &str) -> bool {
        !self.pending.contains(url) && !self.entries.iter().any(|e| e.item.url == url || e.folded.iter().any(|c| c == url))
    }

    /// Items shown in the panel.
    pub fn count(&self) -> u32 {
        self.entries.len() as u32
    }

    fn push(&mut self, mut item: MediaItem, manifest: Manifest, found: Found, children: Vec<String>) {
        if self.entries.len() >= MAX_ENTRIES {
            let drop = self.entries.iter().position(|e| matches!(e.manifest, Manifest::None)).unwrap_or(0);
            self.entries.remove(drop);
        }
        self.next_id += 1;
        item.id = self.next_id;
        item.found_by = found.by;
        item.detected = now();
        if item.title.is_empty() {
            item.title = found.title.to_string();
        }
        self.entries.push(Entry {
            item,
            manifest,
            headers: found.headers,
            page_url: found.page_url.to_string(),
            children,
            folded: Vec::new(),
        });
    }

    fn blank(kind: &'static str, url: &str) -> MediaItem {
        MediaItem {
            id: 0,
            kind,
            url: url.to_string(),
            title: String::new(),
            ext: String::new(),
            size: None,
            duration: None,
            width: None,
            height: None,
            audio_only: false,
            live: false,
            protected: false,
            found_by: "network",
            detected: 0,
            variants: Vec::new(),
            audio_tracks: Vec::new(),
            note: String::new(),
        }
    }

    /// Adds a progressive file. Returns true when the list changed.
    pub fn add_file(&mut self, url: &str, content_type: &str, size: Option<u64>, found: Found) -> bool {
        if let Some(e) = self.find_mut(url) {
            if size.is_some() && e.item.size != size {
                e.item.size = size;
                return true;
            }
            return false;
        }
        if self.knows(url) {
            return false;
        }
        let (ext, audio) = detect::file_ext(content_type, url);
        let mut item = Self::blank("file", url);
        item.ext = ext;
        item.audio_only = audio;
        item.size = size;
        self.push(item, Manifest::None, found, Vec::new());
        true
    }

    /// Adds a parsed HLS playlist; media playlists of a known master enrich it instead.
    pub fn add_hls(&mut self, url: &str, playlist: hls::Playlist, found: Found) -> bool {
        match playlist {
            hls::Playlist::Master(m) => {
                if self.find_mut(url).is_some() {
                    return false;
                }
                let children: Vec<String> = m.playlist_uris().map(str::to_string).collect();
                let mut absorbed = Vec::new();
                self.entries.retain(|e| match &e.manifest {
                    Manifest::HlsMedia(p) if children.contains(&e.item.url) => {
                        absorbed.push((e.item.url.clone(), p.clone()));
                        false
                    }
                    _ => true,
                });
                let (variants, audio_tracks, best) = hls_master_item(&m);
                let mut item = Self::blank("hls", url);
                item.ext = "mp4".into();
                item.width = best.map(|b| b.0);
                item.height = best.map(|b| b.1);
                item.audio_only = !m.variants.is_empty() && m.variants.iter().all(|v| v.audio_only());
                item.protected = m.encrypted;
                item.variants = variants;
                item.audio_tracks = audio_tracks;
                for (_, p) in &absorbed {
                    fold_media_info(&mut item, p);
                }
                self.push(item, Manifest::HlsMaster(m), found, children);
                if let Some(e) = self.entries.last_mut() {
                    e.folded = absorbed.into_iter().map(|(u, _)| u).collect();
                }
                true
            }
            hls::Playlist::Media(p) => {
                if let Some(master) = self.entries.iter_mut().find(|e| e.children.iter().any(|c| c == url)) {
                    if !master.folded.iter().any(|c| c == url) {
                        master.folded.push(url.to_string());
                    }
                    return fold_media_info(&mut master.item, &p);
                }
                if let Some(e) = self.find_mut(url) {
                    let changed = fold_media_info(&mut e.item, &p);
                    e.manifest = Manifest::HlsMedia(p);
                    return changed;
                }
                let mut item = Self::blank("hls", url);
                item.ext = p.container().to_string();
                item.audio_only = item.ext == "aac" || item.ext == "mp3";
                fold_media_info(&mut item, &p);
                self.push(item, Manifest::HlsMedia(p), found, Vec::new());
                true
            }
        }
    }

    /// Adds a parsed DASH manifest.
    pub fn add_dash(&mut self, url: &str, mpd: dash::Mpd, found: Found) -> bool {
        if self.find_mut(url).is_some() {
            return false;
        }
        let (variants, audio_tracks, best) = dash_choices(&mpd);
        let mut item = Self::blank("dash", url);
        item.ext = "mp4".into();
        item.width = best.map(|b| b.0);
        item.height = best.map(|b| b.1);
        item.duration = mpd.duration.filter(|d| *d > 0.0);
        item.live = mpd.dynamic;
        item.audio_only = !variants.is_empty() && variants.iter().all(|v| v.audio_only);
        item.protected = variants.is_empty() && mpd.protected();
        if mpd.dynamic {
            item.note = "Live DASH streams can't be recorded".into();
        }
        item.variants = variants;
        item.audio_tracks = audio_tracks;
        self.push(item, Manifest::Dash(mpd), found, Vec::new());
        true
    }

    /// Merges a page scan. Returns whether the list changed and the manifests to read.
    pub fn apply_scan(&mut self, scan: &Scan, page_url: &str, title: &str) -> (bool, Vec<(String, Sniffed)>) {
        let mut changed = (self.blobs, self.drm) != (scan.blobs, scan.drm);
        self.blobs = scan.blobs.min(1000);
        self.drm = scan.drm.min(1000);
        let mut fetch = Vec::new();
        for it in scan.items.iter().take(60) {
            let url = it.url.trim();
            if !(url.starts_with("http://") || url.starts_with("https://")) || url.len() > 4096 {
                continue;
            }
            if let Some(e) = self.find_mut(url) {
                if e.item.duration.is_none() && it.duration.is_finite() && it.duration > 0.0 {
                    e.item.duration = Some(it.duration);
                    changed = true;
                }
                if e.item.height.is_none() && it.height > 0 {
                    e.item.width = Some(it.width);
                    e.item.height = Some(it.height);
                    changed = true;
                }
                continue;
            }
            if self.knows(url) {
                continue;
            }
            let headers = vec![("Referer".to_string(), page_url.to_string())];
            match detect::guess_from_url(url, it.meta) {
                Some(Sniffed::File) => {
                    let ct = if !it.mime.is_empty() {
                        it.mime.clone()
                    } else if it.audio {
                        "audio/x-unknown".into()
                    } else {
                        String::new()
                    };
                    if self.add_file(url, &ct, None, Found { by: "page", headers, page_url, title }) {
                        if let Some(e) = self.find_mut(url) {
                            e.item.duration = Some(it.duration).filter(|d| d.is_finite() && *d > 0.0);
                            e.item.width = Some(it.width).filter(|w| *w > 0);
                            e.item.height = Some(it.height).filter(|h| *h > 0);
                        }
                        changed = true;
                    }
                }
                Some(kind) => fetch.push((url.to_string(), kind)),
                None => {}
            }
        }
        (changed, fetch)
    }

    /// Items for the panel: downloadable first, then the longest and largest.
    pub fn items(&self) -> Vec<MediaItem> {
        let mut items: Vec<MediaItem> = self.entries.iter().map(|e| e.item.clone()).collect();
        items.sort_by(|a, b| {
            a.protected
                .cmp(&b.protected)
                .then(b.duration.unwrap_or(0.0).total_cmp(&a.duration.unwrap_or(0.0)))
                .then(b.size.cmp(&a.size))
                .then(b.detected.cmp(&a.detected))
        });
        items
    }

    /// Turns a listed item and the user's choices into a download request.
    pub fn request(&self, tab: &str, item_id: u32, variant: Option<&str>, audio: Option<&str>, user_agent: &str) -> Result<Request, String> {
        let e = self.entries.iter().find(|e| e.item.id == item_id).ok_or("This item is no longer on the page; reload the list")?;
        if e.item.protected {
            return Err(job::PROTECTED.into());
        }
        let title = Some(sanitize_stem(&e.item.title)).filter(|t| t != "video" || e.item.title.trim() == "video");
        let stem = title.unwrap_or_else(|| sanitize_stem(&stem_from_url(&e.item.url)));
        let index = |id: Option<&str>, prefix: char| id.and_then(|s| s.strip_prefix(prefix)).and_then(|n| n.parse::<usize>().ok());
        let (plan, suffix, ext, audio_only) = match &e.manifest {
            Manifest::None => (
                Plan { main: Source::File { url: e.item.url.clone(), ext: e.item.ext.clone() }, audio: None, live: false },
                String::new(),
                e.item.ext.clone(),
                e.item.audio_only,
            ),
            Manifest::HlsMedia(p) => (
                Plan { main: Source::Hls(e.item.url.clone()), audio: None, live: p.live() },
                String::new(),
                p.container().to_string(),
                e.item.audio_only,
            ),
            Manifest::HlsMaster(m) => {
                let v = index(variant, 'v')
                    .and_then(|i| m.variants.get(i))
                    .or_else(|| m.variants.iter().filter(|v| !v.audio_only()).max_by_key(|v| v.bandwidth))
                    .or_else(|| m.variants.iter().max_by_key(|v| v.bandwidth))
                    .ok_or("The stream lists no qualities")?;
                let group = m.audio_renditions(v.audio.as_deref());
                let wanted = index(audio, 'a').and_then(|i| m.renditions.get(i));
                let rendition = wanted
                    .and_then(|w| group.iter().find(|r| r.language == w.language && r.name == w.name).copied())
                    .or_else(|| m.default_audio(v));
                let audio_src = if v.audio_only() { None } else { rendition.and_then(|r| r.uri.clone()).map(Source::Hls) };
                let suffix = if v.audio_only() {
                    "audio".to_string()
                } else {
                    v.resolution.map(|r| format!("{}p", r.1)).unwrap_or_default()
                };
                (
                    Plan { main: Source::Hls(v.uri.clone()), audio: audio_src, live: e.item.live },
                    suffix,
                    if v.audio_only() { "m4a".to_string() } else { "mp4".to_string() },
                    v.audio_only(),
                )
            }
            Manifest::Dash(mpd) => {
                if mpd.dynamic {
                    return Err("Live DASH streams can't be recorded".into());
                }
                let reps = &mpd.representations;
                let usable = |i: &usize| matches!(reps[*i].kind, dash::Kind::Video | dash::Kind::Audio) && !reps[*i].protected;
                let main = index(variant, 'r')
                    .filter(|i| *i < reps.len() && usable(i))
                    .or_else(|| {
                        (0..reps.len())
                            .filter(|i| usable(i) && reps[*i].kind == dash::Kind::Video)
                            .max_by_key(|&i| (reps[i].height, reps[i].bandwidth))
                    })
                    .or_else(|| (0..reps.len()).filter(usable).max_by_key(|&i| reps[i].bandwidth))
                    .ok_or(job::PROTECTED)?;
                let rep = &reps[main];
                let audio_rep = if rep.kind == dash::Kind::Video {
                    index(audio, 'r')
                        .filter(|i| *i < reps.len() && usable(i) && reps[*i].kind == dash::Kind::Audio)
                        .or_else(|| {
                            let first_lang = reps.iter().find(|r| r.kind == dash::Kind::Audio && !r.protected).map(|r| r.lang.clone());
                            (0..reps.len())
                                .filter(|i| usable(i) && reps[*i].kind == dash::Kind::Audio && Some(&reps[*i].lang) == first_lang.as_ref())
                                .max_by_key(|&i| reps[i].bandwidth)
                        })
                } else {
                    None
                };
                let source = |r: &dash::Representation| match &r.segments {
                    dash::Segments::Single(url) => Source::File { url: url.clone(), ext: r.container().to_string() },
                    dash::Segments::List { init, media } => {
                        Source::Segments { init: init.clone(), media: media.clone(), ext: r.container().to_string() }
                    }
                };
                let is_audio = rep.kind == dash::Kind::Audio;
                (
                    Plan { main: source(rep), audio: audio_rep.map(|i| source(&reps[i])), live: false },
                    if is_audio { "audio".into() } else { rep.height.map(|h| format!("{h}p")).unwrap_or_default() },
                    rep.container().to_string(),
                    is_audio,
                )
            }
        };
        let stem = if suffix.is_empty() { stem } else { sanitize_stem(&format!("{stem} ({suffix})")) };
        let mut headers: Vec<(String, String)> = e.headers.iter().filter(|(k, _)| k != "User-Agent").cloned().collect();
        if !headers.iter().any(|(k, _)| k == "Referer") && !e.page_url.is_empty() {
            headers.push(("Referer".into(), e.page_url.clone()));
        }
        let ua = e.headers.iter().find(|(k, _)| k == "User-Agent").map(|(_, v)| v.clone()).unwrap_or_else(|| user_agent.to_string());
        Ok(Request {
            tab: tab.to_string(),
            stem,
            mime: mime_for(&ext, audio_only),
            ext,
            source_url: e.item.url.clone(),
            page_url: e.page_url.clone(),
            headers,
            user_agent: ua,
            plan,
        })
    }
}

/// What the media panel receives.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub items: Vec<MediaItem>,
    pub blobs: u32,
    pub drm: u32,
    pub ffmpeg: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub ffmpeg: Option<String>,
}

pub fn tools() -> Tools {
    Tools { ffmpeg: job::find_ffmpeg().map(|p| p.display().to_string()) }
}

fn report(tab_id: &str) -> Result<Report, String> {
    let shared = crate::host::with_host(|h| h.tabs.get(tab_id).map(|t| t.shared.clone()))
        .flatten()
        .ok_or("This tab is not loaded")?;
    let media = shared.media.borrow();
    Ok(Report { items: media.items(), blobs: media.blobs, drm: media.drm, ffmpeg: job::find_ffmpeg().is_some() })
}

/// Lists a tab's media; with `scan`, the page is searched first. Runs on the UI thread.
pub fn list(tab_id: &str, scan: bool, done: impl FnOnce(Result<Report, String>) + 'static) {
    if !scan {
        done(report(tab_id));
        return;
    }
    let id = tab_id.to_string();
    sniff::scan(tab_id, move || done(report(&id)));
}

/// Starts downloading an item. Runs on the UI thread.
pub fn download(tab_id: &str, item_id: u32, variant: Option<String>, audio: Option<String>) -> Result<i64, String> {
    let (shared, core) = crate::host::with_host(|h| h.tabs.get(tab_id).map(|t| (t.shared.clone(), t.core())))
        .flatten()
        .ok_or("This tab is not loaded")?;
    let ua = sniff::user_agent(&core);
    let request = shared.media.borrow().request(tab_id, item_id, variant.as_deref(), audio.as_deref(), &ua)?;
    job::start(request)
}

#[cfg(test)]
mod tests {
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
}
