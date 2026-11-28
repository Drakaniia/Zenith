//! Optional external tools for the media downloader (`ffmpeg.exe` and `yt-dlp.exe`):
//! discovery (settings path → Zenith's managed tools directory → `PATH`), one-click HTTPS
//! installation from official releases, and removal.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::http;

const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";

static INSTALLING_FFMPEG: AtomicBool = AtomicBool::new(false);
static INSTALLING_YTDLP: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgress {
    pub tool: &'static str,
    pub stage: String,
    pub received: u64,
    pub total: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub ffmpeg: Option<String>,
    pub ffmpeg_managed: bool,
    pub ytdlp: Option<String>,
    pub ytdlp_managed: bool,
}

pub fn tools_dir() -> PathBuf {
    crate::host::paths().local.join("tools")
}

pub fn managed_path(tool: &str) -> Option<PathBuf> {
    match tool {
        "ffmpeg" => Some(tools_dir().join("ffmpeg.exe")),
        "ytdlp" => Some(tools_dir().join("yt-dlp.exe")),
        _ => None,
    }
}

fn find_in_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(exe)).find(|p| p.is_file())
}

fn resolve_binary(enabled: bool, configured: Option<PathBuf>, managed: PathBuf, exe: &str) -> Option<PathBuf> {
    if !enabled {
        return None;
    }
    if let Some(p) = configured.filter(|p| p.is_file()) {
        return Some(p);
    }
    if managed.is_file() {
        return Some(managed);
    }
    find_in_path(exe)
}

/// FFmpeg from settings, Zenith's managed tools folder, or `PATH`, if enabled.
pub fn find_ffmpeg() -> Option<PathBuf> {
    let cfg = crate::host::config();
    resolve_binary(cfg.use_ffmpeg, cfg.ffmpeg_path, tools_dir().join("ffmpeg.exe"), "ffmpeg.exe")
}

/// yt-dlp from settings, Zenith's managed tools folder, or `PATH`, if enabled.
pub fn find_ytdlp() -> Option<PathBuf> {
    let cfg = crate::host::config();
    resolve_binary(cfg.use_ytdlp, cfg.ytdlp_path, tools_dir().join("yt-dlp.exe"), "yt-dlp.exe")
}

pub fn status() -> Tools {
    let ff_managed = tools_dir().join("ffmpeg.exe").is_file();
    let yt_managed = tools_dir().join("yt-dlp.exe").is_file();
    Tools {
        ffmpeg: find_ffmpeg().map(|p| p.display().to_string()),
        ffmpeg_managed: ff_managed,
        ytdlp: find_ytdlp().map(|p| p.display().to_string()),
        ytdlp_managed: yt_managed,
    }
}

fn emit_progress(tool: &'static str, stage: impl Into<String>, received: u64, total: u64) {
    crate::host::emit(
        "media-tool-progress",
        ToolProgress { tool, stage: stage.into(), received, total },
    );
}

struct InstallGuard(&'static AtomicBool);
impl Drop for InstallGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn claim_install(tool: &str) -> Result<(&'static str, InstallGuard), String> {
    let (name, flag): (&'static str, &'static AtomicBool) = match tool {
        "ffmpeg" => ("ffmpeg", &INSTALLING_FFMPEG),
        "ytdlp" => ("ytdlp", &INSTALLING_YTDLP),
        _ => return Err(format!("Unknown tool: {tool}")),
    };
    if flag.swap(true, Ordering::AcqRel) {
        return Err(format!("{name} is already being installed"));
    }
    Ok((name, InstallGuard(flag)))
}

fn is_pe_executable(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = File::open(path) else { return false };
    let mut magic = [0u8; 2];
    f.read_exact(&mut magic).is_ok() && &magic == b"MZ"
}

fn find_file_recursive(root: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, name, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

fn download_file(tool: &'static str, url: &str, dest: &Path) -> Result<(), String> {
    let policy: http::Policy = Box::new(|u: &str| u.starts_with("https://"));
    let client = http::Client::with_policy("Zenith-Browser/1.0", policy).map_err(|e| e.to_string())?;
    let headers = |_: &str| Vec::new();
    let mut resp = client.get(url, &headers, None, false).map_err(|e| e.to_string())?;
    let total = resp.total.or(resp.content_length).unwrap_or(0);
    let mut file = File::create(dest).map_err(|e| format!("Could not create file: {e}"))?;
    let mut received = 0u64;
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    emit_progress(tool, "Downloading…", 0, total);
    let stop = || false;
    resp.copy_to(&mut file, 0, None, &stop, &mut |n| {
        received += n;
        if last_emit.elapsed() >= Duration::from_millis(180) {
            last_emit = Instant::now();
            emit_progress(tool, "Downloading…", received, total);
        }
    })
    .map_err(|e| e.to_string())?;
    emit_progress(tool, "Downloading…", received, total.max(received));
    Ok(())
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let tar = Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()))
        .join("System32")
        .join("tar.exe");
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = std::process::Command::new(tar)
        .args(["-xf"])
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("Could not run tar.exe: {e}"))?;
    if !status.success() {
        return Err("Failed to extract the FFmpeg archive".into());
    }
    Ok(())
}

fn replace_file(src: &Path, dest: &Path) -> Result<(), String> {
    let _ = fs::remove_file(dest);
    if fs::rename(src, dest).is_err() {
        fs::copy(src, dest).map_err(|e| format!("Could not move binary into place: {e}"))?;
        let _ = fs::remove_file(src);
    }
    Ok(())
}

/// Downloads and installs a managed copy of `"ffmpeg"` or `"ytdlp"` into Zenith's tools directory.
pub fn install(tool: &str) -> Result<String, String> {
    let (name, _guard) = claim_install(tool)?;
    let dir = tools_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let result = match name {
        "ytdlp" => {
            let tmp = dir.join("yt-dlp.exe.part");
            let dest = dir.join("yt-dlp.exe");
            let r = download_file("ytdlp", YTDLP_URL, &tmp).and_then(|()| {
                if !is_pe_executable(&tmp) {
                    return Err("Downloaded yt-dlp file is not a valid Windows executable".into());
                }
                replace_file(&tmp, &dest)?;
                Ok(dest.display().to_string())
            });
            let _ = fs::remove_file(&tmp);
            r
        }
        "ffmpeg" => {
            let zip = dir.join("ffmpeg.zip.part");
            let stage = dir.join("ffmpeg-extract");
            let dest = dir.join("ffmpeg.exe");
            let _ = fs::remove_dir_all(&stage);
            let r = download_file("ffmpeg", FFMPEG_URL, &zip).and_then(|()| {
                emit_progress("ffmpeg", "Extracting FFmpeg…", 0, 0);
                extract_zip(&zip, &stage)?;
                let found = find_file_recursive(&stage, "ffmpeg.exe", 5)
                    .ok_or_else(|| "ffmpeg.exe was not found inside the downloaded archive".to_string())?;
                if !is_pe_executable(&found) {
                    return Err("Extracted ffmpeg.exe is not a valid Windows executable".into());
                }
                replace_file(&found, &dest)?;
                Ok(dest.display().to_string())
            });
            let _ = fs::remove_file(&zip);
            let _ = fs::remove_dir_all(&stage);
            r
        }
        _ => unreachable!(),
    };
    emit_progress(name, "", 0, 0);
    result
}

/// Removes Zenith's managed copy of `"ffmpeg"` or `"ytdlp"`.
pub fn remove(tool: &str) -> Result<(), String> {
    let path = managed_path(tool).ok_or_else(|| format!("Unknown tool: {tool}"))?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Could not remove {tool}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("zenith-tools-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn resolve_prefers_configured_then_managed() {
        let dir = temp_dir("resolve");
        let custom = dir.join("custom.exe");
        let managed = dir.join("managed.exe");
        fs::write(&custom, b"MZcustom").unwrap();
        fs::write(&managed, b"MZmanaged").unwrap();

        assert_eq!(resolve_binary(false, Some(custom.clone()), managed.clone(), "nope-xyz.exe"), None);
        assert_eq!(resolve_binary(true, Some(custom.clone()), managed.clone(), "nope-xyz.exe"), Some(custom));
        assert_eq!(resolve_binary(true, None, managed.clone(), "nope-xyz.exe"), Some(managed));
        assert_eq!(resolve_binary(true, Some(dir.join("missing.exe")), dir.join("missing2.exe"), "nope-xyz.exe"), None);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pe_magic_and_recursive_search_work() {
        let dir = temp_dir("search");
        let nested = dir.join("ffmpeg-7.1").join("bin");
        fs::create_dir_all(&nested).unwrap();
        let exe = nested.join("ffmpeg.exe");
        fs::write(&exe, b"MZ\x90\x00").unwrap();
        let bad = nested.join("readme.txt");
        fs::write(&bad, b"hello").unwrap();

        assert!(is_pe_executable(&exe));
        assert!(!is_pe_executable(&bad));
        assert_eq!(find_file_recursive(&dir, "ffmpeg.exe", 4), Some(exe));
        assert_eq!(find_file_recursive(&dir, "ffmpeg.exe", 1), None);

        let _ = fs::remove_dir_all(&dir);
    }
}
