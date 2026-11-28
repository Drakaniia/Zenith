//! Tiered URL probing and downloading for pasted links and fallback tab inspection:
//! 1. Native Rust HTTP probe (direct files, HLS `.m3u8`, DASH `.mpd`, HTML `<video>`/`og:video`/JSON-LD)
//! 2. Optional managed `yt-dlp` fallback for complex video sites (YouTube, TikTok, X, Twitch, etc.).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;

use super::detect::{self, Sniffed};
use super::job::{self, AudioFormat};
use super::{dash, hls, http, sniff, tools, ytdlp, Found, MediaItem, TabMedia};

const MAX_HTML_BYTES: usize = 512 * 1024;
const MAX_MANIFEST_BYTES: usize = 16 << 20;
const MAX_CACHED_PROBES: usize = 24;

static NEXT_PROBE: AtomicU64 = AtomicU64::new(1);

enum CachedProbe {
    Native(TabMedia),
    Ytdlp(MediaItem),
}

static PROBES: LazyLock<Mutex<HashMap<String, CachedProbe>>> = LazyLock::new(Mutex::default);

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UrlProbeResult {
    pub probe_id: String,
    /// "native" | "ytdlp" | "none"
    pub engine: &'static str,
    pub items: Vec<MediaItem>,
    pub ffmpeg: bool,
    pub ytdlp: bool,
    pub hint: String,
}

fn cache_probe(entry: CachedProbe) -> String {
    let id = format!("p{}", NEXT_PROBE.fetch_add(1, Ordering::Relaxed));
    if let Ok(mut map) = PROBES.lock() {
        if map.len() >= MAX_CACHED_PROBES {
            if let Some(oldest) = map.keys().next().cloned() {
                map.remove(&oldest);
            }
        }
        map.insert(id.clone(), entry);
    }
    id
}

fn validate_probe_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 {
        return Err("Paste a valid http:// or https:// video address".into());
    }
    let u = url::Url::parse(trimmed).map_err(|_| "Paste a valid http:// or https:// video address".to_string())?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("Only http:// and https:// addresses can be downloaded".into());
    }
    Ok(u.to_string())
}

/// Extracts `<title>` and candidate media URLs from a small HTML snippet (no regex dependency needed).
pub(crate) fn extract_html_candidates(html: &str, base_url: &str) -> (String, Vec<String>) {
    let base = url::Url::parse(base_url).ok();
    let resolve = |raw: &str| -> Option<String> {
        let cleaned = raw.trim().replace("&amp;", "&");
        if cleaned.is_empty() || cleaned.starts_with("blob:") || cleaned.starts_with("data:") {
            return None;
        }
        let u = base.as_ref().and_then(|b| b.join(&cleaned).ok()).or_else(|| url::Url::parse(&cleaned).ok())?;
        matches!(u.scheme(), "http" | "https").then(|| u.to_string())
    };

    let lower = html.to_ascii_lowercase();
    let title = lower
        .find("<title")
        .and_then(|i| lower[i..].find('>').map(|j| i + j + 1))
        .and_then(|start| lower[start..].find("</title>").map(|end| html[start..start + end].trim().to_string()))
        .unwrap_or_default();

    let mut urls = Vec::new();
    let mut push = |u: String| {
        if !urls.contains(&u) && urls.len() < 12 {
            urls.push(u);
        }
    };

    // 1. OpenGraph / Twitter stream meta tags
    let mut pos = 0;
    while let Some(rel) = lower[pos..].find("<meta") {
        let start = pos + rel;
        let Some(end_rel) = lower[start..].find('>') else { break };
        let tag = &html[start..start + end_rel + 1];
        let tag_l = &lower[start..start + end_rel + 1];
        if tag_l.contains("og:video") || tag_l.contains("twitter:player:stream") || tag_l.contains("og:audio") {
            if let Some(val) = attr_value(tag, "content").as_deref().and_then(&resolve) {
                if detect::guess_from_url(&val, true).is_some() {
                    push(val);
                }
            }
        }
        pos = start + end_rel + 1;
    }

    // 2. <video src=...> / <source src=...> / <audio src=...>
    for needle in ["<video", "<source", "<audio"] {
        let mut p = 0;
        while let Some(rel) = lower[p..].find(needle) {
            let start = p + rel;
            let Some(end_rel) = lower[start..].find('>') else { break };
            let tag = &html[start..start + end_rel + 1];
            if let Some(val) = attr_value(tag, "src").as_deref().and_then(&resolve) {
                push(val);
            }
            p = start + end_rel + 1;
        }
    }

    // 3. JSON-LD "contentUrl":"https://..."
    let mut p = 0;
    while let Some(rel) = html[p..].find("\"contentUrl\"") {
        let start = p + rel + "\"contentUrl\"".len();
        if let Some(colon) = html[start..].find(':') {
            let after = html[start + colon + 1..].trim_start();
            if let Some(rest) = after.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    let raw = rest[..end].replace("\\/", "/");
                    if let Some(val) = resolve(&raw) {
                        if detect::guess_from_url(&val, true).is_some() {
                            push(val);
                        }
                    }
                }
            }
        }
        p = start;
    }

    (title, urls)
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{name}=");
    let idx = lower.find(&needle)? + needle.len();
    let rest = tag[idx..].trim_start();
    let (quote, after) = match rest.as_bytes().first().copied() {
        Some(b'"') => (Some('"'), &rest[1..]),
        Some(b'\'') => (Some('\''), &rest[1..]),
        _ => (None, rest),
    };
    let val = match quote {
        Some(q) => after.split(q).next()?,
        None => after.split(|c: char| c.is_whitespace() || c == '>').next()?,
    };
    Some(val.trim().to_string())
}

fn probe_native(url: &str) -> Option<TabMedia> {
    let ua = sniff::default_user_agent();
    let client = http::Client::with_policy(&ua, job::network_policy(url)).ok()?;
    let no_headers = |_: &str| Vec::new();
    let mut resp = client.get(url, &no_headers, None, true).ok()?;
    let final_url = resp.final_url.clone();
    let ct = resp.content_type.clone();
    let total = resp.total.or(resp.content_length);

    let mut media = TabMedia::default();
    fn found<'a>(page: &'a str, title: &'a str) -> Found<'a> {
        Found {
            by: "network",
            headers: vec![("Referer".into(), page.to_string())],
            page_url: page,
            title,
        }
    }

    if let Some(kind) = detect::classify(&final_url, &ct, true, resp.status as u16, total) {
        match kind {
            Sniffed::File => {
                media.add_file(&final_url, &ct, total, found(&final_url, ""));
                return Some(media);
            }
            Sniffed::Hls => {
                let bytes = resp.bytes(MAX_MANIFEST_BYTES).ok()?;
                let text = String::from_utf8_lossy(&bytes);
                if let Ok(pl) = hls::parse(&text, &final_url) {
                    media.add_hls(&final_url, pl, found(&final_url, ""));
                    return Some(media);
                }
            }
            Sniffed::Dash => {
                let bytes = resp.bytes(MAX_MANIFEST_BYTES).ok()?;
                let text = String::from_utf8_lossy(&bytes);
                if let Ok(mpd) = dash::parse(&text, &final_url) {
                    media.add_dash(&final_url, mpd, found(&final_url, ""));
                    return Some(media);
                }
            }
        }
    }

    // HTML page: scan for embedded video/HLS/DASH URLs natively.
    if detect::mime(&ct).starts_with("text/html") {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 32 * 1024];
        while buf.len() < MAX_HTML_BYTES {
            match resp.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n.min(MAX_HTML_BYTES - buf.len())]),
            }
        }
        let html = String::from_utf8_lossy(&buf);
        let (title, candidates) = extract_html_candidates(&html, &final_url);
        for cand in candidates {
            match detect::guess_from_url(&cand, false) {
                Some(Sniffed::Hls) => {
                    if let Ok((text, resolved)) = client.text(&cand, &no_headers, MAX_MANIFEST_BYTES) {
                        if let Ok(pl) = hls::parse(&text, &resolved) {
                            media.add_hls(&resolved, pl, found(&final_url, &title));
                        }
                    }
                }
                Some(Sniffed::Dash) => {
                    if let Ok((text, resolved)) = client.text(&cand, &no_headers, MAX_MANIFEST_BYTES) {
                        if let Ok(mpd) = dash::parse(&text, &resolved) {
                            media.add_dash(&resolved, mpd, found(&final_url, &title));
                        }
                    }
                }
                Some(Sniffed::File) => {
                    media.add_file(&cand, "", None, found(&final_url, &title));
                }
                None => {}
            }
        }
        if media.count() > 0 {
            return Some(media);
        }
    }
    None
}

/// Probes any URL using Zenith's tiered engine: native Rust first, optional `yt-dlp` fallback.
/// When `prefer_ytdlp` is true (e.g. "Inspect with yt-dlp" on a tab), tries `yt-dlp` directly.
pub fn probe_url(raw_url: &str, prefer_ytdlp: bool) -> Result<UrlProbeResult, String> {
    let url = validate_probe_url(raw_url)?;
    let ffmpeg = tools::find_ffmpeg().is_some();
    let ytdlp_bin = tools::find_ytdlp();
    let ytdlp_ok = ytdlp_bin.is_some();

    if !prefer_ytdlp {
        if let Some(native) = probe_native(&url) {
            let items = native.items();
            if !items.is_empty() {
                let probe_id = cache_probe(CachedProbe::Native(native));
                return Ok(UrlProbeResult {
                    probe_id,
                    engine: "native",
                    items,
                    ffmpeg,
                    ytdlp: ytdlp_ok,
                    hint: String::new(),
                });
            }
        }
    }

    if let Some(bin) = ytdlp_bin {
        let item = ytdlp::probe(&bin, &url)?;
        let items = vec![item.clone()];
        let probe_id = cache_probe(CachedProbe::Ytdlp(item));
        return Ok(UrlProbeResult {
            probe_id,
            engine: "ytdlp",
            items,
            ffmpeg,
            ytdlp: true,
            hint: String::new(),
        });
    }

    Ok(UrlProbeResult {
        probe_id: String::new(),
        engine: "none",
        items: Vec::new(),
        ffmpeg,
        ytdlp: false,
        hint: "No direct video stream found on this link. Install yt-dlp in Settings → Downloads to download from YouTube, TikTok, X, Twitch and 1,000+ sites.".into(),
    })
}

/// Starts downloading an item previously returned by `probe_url`.
pub fn download_probed(
    probe_id: &str,
    item_id: u32,
    variant: Option<String>,
    audio: Option<String>,
    audio_format: Option<String>,
    tab_id: Option<String>,
) -> Result<i64, String> {
    let fmt = audio_format.as_deref().and_then(AudioFormat::parse);
    let tab = tab_id.unwrap_or_default();
    let map = PROBES.lock().map_err(|_| "probe cache unavailable")?;
    let entry = map.get(probe_id).ok_or("This inspected link expired; inspect it again")?;
    match entry {
        CachedProbe::Native(media) => {
            let ua = sniff::default_user_agent();
            let req = media.request(&tab, item_id, variant.as_deref(), audio.as_deref(), fmt, &ua)?;
            if req.audio_format.is_some() && !req.source_is_audio && tools::find_ffmpeg().is_none() {
                return Err("Install FFmpeg in Settings → Downloads to extract audio from this video".into());
            }
            job::start(req)
        }
        CachedProbe::Ytdlp(item) => {
            let bin = tools::find_ytdlp().ok_or("yt-dlp is no longer available; install it in Settings → Downloads")?;
            let ff = tools::find_ffmpeg();
            ytdlp::start_download(bin, ff, tab, item, variant.as_deref(), fmt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_html_video_og_and_jsonld_candidates() {
        let html = r#"<html><head>
            <title>Launch Event</title>
            <meta property="og:video:secure_url" content="https://cdn.example.com/hls/master.m3u8">
            <script type="application/ld+json">{"@type":"VideoObject","contentUrl":"https:\/\/cdn.example.com\/clip.mp4"}</script>
        </head><body><video src="/media/intro.mp4"></video></body></html>"#;
        let (title, urls) = extract_html_candidates(html, "https://site.example.com/watch");
        assert_eq!(title, "Launch Event");
        assert!(urls.contains(&"https://cdn.example.com/hls/master.m3u8".to_string()));
        assert!(urls.contains(&"https://site.example.com/media/intro.mp4".to_string()));
        assert!(urls.contains(&"https://cdn.example.com/clip.mp4".to_string()));
    }

    #[test]
    fn validates_probe_url_schemes() {
        assert!(validate_probe_url("https://example.com/v.mp4").is_ok());
        assert!(validate_probe_url("javascript:alert(1)").is_err());
        assert!(validate_probe_url("not a url").is_err());
    }
}
