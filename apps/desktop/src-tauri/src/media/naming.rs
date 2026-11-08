//! Quality labels, file names and MIME types for listed media.

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
pub(super) fn stem_from_url(url: &str) -> String {
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
