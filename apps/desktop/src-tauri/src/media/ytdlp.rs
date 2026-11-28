//! Optional `yt-dlp` backend: probes complex video pages and executes downloads inside Zenith's
//! regular downloads list with live progress, cancellation, audio extraction, and DRM refusal.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::job::{self, AudioFormat, Control, Halt, Output, Progress, CANCEL, PROTECTED};
use super::{mime_for, quality_label, sanitize_stem, Choice, MediaItem};

const PROBE_TIMEOUT: Duration = Duration::from_secs(35);
const MAX_JSON_BYTES: u64 = 16 << 20;

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
struct YtInfo {
    title: String,
    duration: Option<f64>,
    is_live: Option<bool>,
    ext: Option<String>,
    webpage_url: Option<String>,
    #[serde(alias = "_has_drm")]
    has_drm: Option<serde_json::Value>,
    formats: Vec<YtFormat>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
struct YtFormat {
    format_id: String,
    ext: String,
    vcodec: Option<String>,
    acodec: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
    tbr: Option<f64>,
    abr: Option<f64>,
    filesize: Option<u64>,
    filesize_approx: Option<u64>,
    language: Option<String>,
    has_drm: Option<serde_json::Value>,
}

fn is_drm(val: &Option<serde_json::Value>) -> bool {
    match val {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(s)) => !s.is_empty() && !s.eq_ignore_ascii_case("false"),
        _ => false,
    }
}

fn has_codec(c: &Option<String>) -> bool {
    c.as_deref().is_some_and(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
}

/// Validates a user/page URL before passing it to `yt-dlp`.
pub fn validate_url(raw: &str) -> Result<String, String> {
    let u = raw.trim();
    if u.is_empty() || u.len() > 4096 {
        return Err("Enter a valid http:// or https:// video address".into());
    }
    let parsed = url::Url::parse(u).map_err(|_| "Enter a valid http:// or https:// video address".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only http:// and https:// video links are supported".into());
    }
    if parsed.host_str().is_none_or(super::detect::is_private_host) {
        return Err("Local and private network links can't be probed by yt-dlp".into());
    }
    Ok(parsed.to_string())
}

/// Parses `yt-dlp --dump-single-json` output into a Zenith `MediaItem`.
pub fn parse_probe_json(json: &str, url: &str) -> Result<MediaItem, String> {
    let info: YtInfo = serde_json::from_str(json).map_err(|e| format!("Could not read yt-dlp output: {e}"))?;
    let all_drm = is_drm(&info.has_drm)
        || (!info.formats.is_empty() && info.formats.iter().all(|f| is_drm(&f.has_drm)));

    // Group playable video formats by height, keeping the best candidate per height.
    let mut by_height: BTreeMap<u32, &YtFormat> = BTreeMap::new();
    let mut best_audio: Option<&YtFormat> = None;
    let mut best_size: Option<u64> = None;
    for f in &info.formats {
        if is_drm(&f.has_drm) {
            continue;
        }
        let has_v = has_codec(&f.vcodec);
        let has_a = has_codec(&f.acodec);
        if has_v {
            if let Some(h) = f.height.filter(|h| *h > 0) {
                let score = (f.fps.unwrap_or(0.0) as u64, f.tbr.unwrap_or(0.0) as u64);
                let replace = by_height
                    .get(&h)
                    .map(|cur| score > (cur.fps.unwrap_or(0.0) as u64, cur.tbr.unwrap_or(0.0) as u64))
                    .unwrap_or(true);
                if replace {
                    by_height.insert(h, f);
                }
            }
        } else if has_a {
            let abr = f.abr.or(f.tbr).unwrap_or(0.0);
            if best_audio.is_none_or(|cur| abr > cur.abr.or(cur.tbr).unwrap_or(0.0)) {
                best_audio = Some(f);
            }
        }
    }

    let mut variants: Vec<Choice> = Vec::new();
    let mut top_w = None;
    let mut top_h = None;
    for (&h, f) in by_height.iter().rev() {
        if top_h.is_none() {
            top_h = Some(h);
            top_w = f.width;
            best_size = f.filesize.or(f.filesize_approx);
        }
        let bw = f.tbr.map(|k| (k * 1000.0) as u64).unwrap_or(0);
        variants.push(Choice {
            id: format!("yt:h{h}"),
            label: quality_label(Some(h), bw, f.fps, false),
            height: Some(h),
            bandwidth: Some(bw).filter(|b| *b > 0),
            audio_only: false,
        });
    }
    if variants.is_empty() && !all_drm {
        if let Some(a) = best_audio {
            let bw = a.abr.or(a.tbr).map(|k| (k * 1000.0) as u64).unwrap_or(0);
            variants.push(Choice {
                id: "yt:audio".into(),
                label: quality_label(None, bw, None, true),
                height: None,
                bandwidth: Some(bw).filter(|b| *b > 0),
                audio_only: true,
            });
            best_size = a.filesize.or(a.filesize_approx);
        } else {
            variants.push(Choice {
                id: "yt:best".into(),
                label: "Best quality".into(),
                height: None,
                bandwidth: None,
                audio_only: false,
            });
        }
    }

    let audio_only = !variants.is_empty() && variants.iter().all(|v| v.audio_only);
    let live = info.is_live.unwrap_or(false);
    Ok(MediaItem {
        id: 1,
        kind: "ytdlp",
        url: info.webpage_url.filter(|u| !u.is_empty()).unwrap_or_else(|| url.to_string()),
        title: info.title.trim().to_string(),
        ext: if audio_only { "m4a".into() } else { info.ext.unwrap_or_else(|| "mp4".into()) },
        size: best_size,
        duration: info.duration.filter(|d| d.is_finite() && *d > 0.0),
        width: top_w,
        height: top_h,
        audio_only,
        live,
        protected: all_drm,
        found_by: "ytdlp",
        detected: crate::storage::now_ms(),
        variants,
        audio_tracks: Vec::new(),
        note: if all_drm { PROTECTED.into() } else { String::new() },
    })
}

/// Runs `yt-dlp --dump-single-json` on a validated URL.
pub fn probe(ytdlp: &Path, raw_url: &str) -> Result<MediaItem, String> {
    let url = validate_url(raw_url)?;
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = Command::new(ytdlp)
        .args([
            "--dump-single-json",
            "--no-playlist",
            "--no-warnings",
            "--skip-download",
            "--socket-timeout",
            "15",
            "--",
            &url,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start yt-dlp: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(mut s) = stdout {
            let _ = (&mut s).take(MAX_JSON_BYTES).read_to_string(&mut buf);
        }
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(mut s) = stderr {
            let _ = (&mut s).take(32 * 1024).read_to_string(&mut buf);
        }
        buf
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if start.elapsed() > PROBE_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("yt-dlp timed out while inspecting this link".into());
                }
                std::thread::sleep(Duration::from_millis(60));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    if !status.success() {
        let msg = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("yt-dlp could not find a downloadable video on this link")
            .trim();
        if msg.to_ascii_lowercase().contains("drm") {
            return Err(PROTECTED.into());
        }
        return Err(msg.to_string());
    }
    parse_probe_json(&out, &url)
}

/// Builds the `-f` selector for `yt-dlp` from a Zenith variant choice (`yt:h1080`, `yt:audio`, etc.).
pub(crate) fn format_selector(variant: Option<&str>, has_ffmpeg: bool) -> String {
    if let Some(v) = variant {
        if v == "yt:audio" {
            return "bestaudio[ext=m4a]/bestaudio".into();
        }
        if let Some(h) = v.strip_prefix("yt:h").and_then(|n| n.parse::<u32>().ok()) {
            return if has_ffmpeg {
                format!("bestvideo[height<={h}]+bestaudio/best[height<={h}]/best")
            } else {
                format!("best[height<={h}]/best")
            };
        }
    }
    if has_ffmpeg {
        "bestvideo+bestaudio/best".into()
    } else {
        "best".into()
    }
}

/// Parses a `ZENITH|<downloaded>|<total>|<estimate>|<pct>` progress line from `yt-dlp`.
pub(crate) fn parse_progress_line(line: &str) -> Option<(u64, u64)> {
    let rest = line.strip_prefix("ZENITH|")?;
    let mut parts = rest.split('|');
    let downloaded: u64 = parts.next()?.trim().parse().ok()?;
    let total = parts
        .next()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .or_else(|| parts.next().and_then(|s| s.trim().parse::<u64>().ok()))
        .unwrap_or(0);
    Some((downloaded, total))
}

/// Starts a `yt-dlp` download job tracked inside Zenith's downloads list.
pub fn start_download(
    ytdlp: PathBuf,
    ffmpeg: Option<PathBuf>,
    tab: String,
    item: &MediaItem,
    variant: Option<&str>,
    audio_format: Option<AudioFormat>,
) -> Result<i64, String> {
    if item.protected {
        return Err(PROTECTED.into());
    }
    let url = validate_url(&item.url)?;
    let dir = job::download_dir();
    let stem = sanitize_stem(if item.title.is_empty() { "video" } else { &item.title });
    let ext = audio_format.map(|f| f.ext()).unwrap_or(if item.audio_only { "m4a" } else { "mp4" });
    let provisional = dir.join(format!("{stem}.{ext}"));
    let mime = mime_for(ext, audio_format.is_some() || item.audio_only);
    let has_ffmpeg = ffmpeg.is_some();
    let selector = format_selector(variant, has_ffmpeg);
    let live = item.live;

    job::start_external(tab, url.clone(), provisional, mime, live, move |control: Arc<Control>, progress: Arc<Progress>| {
        run_ytdlp(&ytdlp, ffmpeg.as_deref(), &dir, &url, &selector, audio_format, &control, &progress)
    })
}

#[allow(clippy::too_many_arguments)]
fn run_ytdlp(
    ytdlp: &Path,
    ffmpeg: Option<&Path>,
    dir: &Path,
    url: &str,
    selector: &str,
    audio_format: Option<AudioFormat>,
    control: &Control,
    progress: &Progress,
) -> Result<Output, Halt> {
    let _ = std::fs::create_dir_all(dir);
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut cmd = Command::new(ytdlp);
    cmd.args([
        "--newline",
        "--no-playlist",
        "--no-warnings",
        "--no-overwrites",
        "--progress",
        "--progress-template",
        "download:ZENITH|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress._percent_str)s",
        "--print",
        "after_move:ZENITH_FILE:%(filepath)s",
        "-o",
        "%(title).120B.%(ext)s",
    ])
    .arg("-P")
    .arg(dir);

    if let Some(ff) = ffmpeg {
        cmd.arg("--ffmpeg-location").arg(ff);
    }

    if let Some(fmt) = audio_format {
        if ffmpeg.is_some() {
            cmd.args(["-x", "--audio-format", fmt.ext(), "--audio-quality", "0"]);
        } else {
            cmd.args(["-f", "bestaudio[ext=m4a]/bestaudio"]);
        }
    } else {
        cmd.args(["-f", selector]);
        if ffmpeg.is_some() {
            cmd.args(["--merge-output-format", "mp4"]);
        }
    }

    cmd.args(["--", url])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    progress.set_stage("Starting yt-dlp…");
    let mut child = cmd.spawn().map_err(|e| Halt::Failed(format!("Could not start yt-dlp: {e}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let err_reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut s) = stderr {
            let _ = (&mut s).take(32 * 1024).read_to_string(&mut text);
        }
        text
    });

    let final_path = Arc::new(std::sync::Mutex::new(None::<PathBuf>));
    let final_clone = final_path.clone();
    let prog_track = Arc::new(std::sync::Mutex::new(()));
    let _ = prog_track;

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let out_reader = std::thread::spawn(move || {
        if let Some(s) = stdout {
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        }
    });

    let mut last_stage_cleared = false;
    loop {
        while let Ok(line) = rx.try_recv() {
            let trimmed = line.trim();
            if let Some((done, total)) = parse_progress_line(trimmed) {
                if !last_stage_cleared {
                    progress.set_stage("");
                    last_stage_cleared = true;
                }
                progress.tracks[0].bytes.store(done, Ordering::Relaxed);
                progress.tracks[0].total.store(total, Ordering::Relaxed);
            } else if let Some(path) = trimmed.strip_prefix("ZENITH_FILE:") {
                let p = PathBuf::from(path.trim());
                if !p.as_os_str().is_empty() {
                    if let Ok(mut g) = final_clone.lock() {
                        *g = Some(p);
                    }
                }
            } else if trimmed.starts_with("[Merger]") || trimmed.starts_with("[ExtractAudio]") {
                progress.set_stage(if trimmed.starts_with("[ExtractAudio]") {
                    "Extracting audio…"
                } else {
                    "Merging audio and video…"
                });
            }
        }

        if control.get() == CANCEL {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Halt::Canceled);
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = out_reader.join();
                while let Ok(line) = rx.try_recv() {
                    if let Some(path) = line.trim().strip_prefix("ZENITH_FILE:") {
                        let p = PathBuf::from(path.trim());
                        if !p.as_os_str().is_empty() {
                            if let Ok(mut g) = final_clone.lock() {
                                *g = Some(p);
                            }
                        }
                    }
                }
                let err = err_reader.join().unwrap_or_default();
                if !status.success() {
                    let msg = err
                        .lines()
                        .rev()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("yt-dlp download failed")
                        .trim();
                    return Err(Halt::Failed(msg.to_string()));
                }
                let file = final_path
                    .lock()
                    .ok()
                    .and_then(|g| g.clone())
                    .filter(|p| p.is_file())
                    .ok_or_else(|| Halt::Failed("yt-dlp finished without reporting an output file".into()))?;
                let note = if audio_format.is_some() && ffmpeg.is_none() {
                    "Saved native audio (install FFmpeg in Settings for MP3/Opus conversion)".into()
                } else {
                    String::new()
                };
                return Ok(Output { files: vec![file], note });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(80)),
            Err(e) => return Err(Halt::Failed(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_urls_strictly() {
        assert!(validate_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ").is_ok());
        assert!(validate_url("file:///C:/secret.txt").is_err());
        assert!(validate_url("--exec calc.exe").is_err());
        assert!(validate_url("http://127.0.0.1:8080/v.mp4").is_err());
    }

    #[test]
    fn parses_ytdlp_json_and_drm() {
        let json = r#"{
            "title": "Sample Video",
            "duration": 125.5,
            "webpage_url": "https://example.com/watch?v=1",
            "formats": [
                {"format_id": "137", "ext": "mp4", "vcodec": "avc1", "acodec": "none", "width": 1920, "height": 1080, "fps": 60, "tbr": 4200.0, "filesize": 50000000},
                {"format_id": "136", "ext": "mp4", "vcodec": "avc1", "acodec": "none", "width": 1280, "height": 720, "fps": 30, "tbr": 1800.0},
                {"format_id": "140", "ext": "m4a", "vcodec": "none", "acodec": "mp4a", "abr": 128.0}
            ]
        }"#;
        let item = parse_probe_json(json, "https://example.com/watch?v=1").unwrap();
        assert_eq!(item.title, "Sample Video");
        assert_eq!(item.height, Some(1080));
        assert_eq!(item.variants.len(), 2);
        assert_eq!(item.variants[0].id, "yt:h1080");
        assert_eq!(item.variants[1].id, "yt:h720");
        assert!(!item.protected);

        let drm_json = r#"{"title": "Locked", "_has_drm": true, "formats": []}"#;
        let drm = parse_probe_json(drm_json, "https://example.com/drm").unwrap();
        assert!(drm.protected);
    }

    #[test]
    fn progress_and_format_selectors_work() {
        assert_eq!(parse_progress_line("ZENITH|1024|4096|NA|25.0%"), Some((1024, 4096)));
        assert_eq!(parse_progress_line("ZENITH|512|NA|2048|25.0%"), Some((512, 2048)));
        assert_eq!(parse_progress_line("[download] 10%"), None);
        assert_eq!(format_selector(Some("yt:h1080"), true), "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best");
        assert_eq!(format_selector(Some("yt:h720"), false), "best[height<=720]/best");
        assert_eq!(format_selector(Some("yt:audio"), false), "bestaudio[ext=m4a]/bestaudio");
    }
}
