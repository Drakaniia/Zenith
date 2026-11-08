//! Finishing a job: merging tracks (natively or with FFmpeg), giving the result a unique name in
//! the downloads folder and marking it as downloaded from the internet.

use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::time::Duration;
use std::path::{Path, PathBuf};

use super::fetch::{io_halt, Ctx, Halt};
use super::state::CANCEL;
use super::track::Track;
use super::{Job, Output, PROTECTED};
use crate::media::mp4;

impl Job {
    /// Converts or merges the downloaded tracks and moves the results into `dir`.
    pub fn finish(&self, ctx: &Ctx, dir: &Path, stem: &str) -> Result<Output, Halt> {
        let ffmpeg = ctx.env.ffmpeg();
        let main = &self.tracks[0];
        if main.part_len == 0 && fs::metadata(&main.part).map(|m| m.len()).unwrap_or(0) == 0 {
            return Err(Halt::Failed("Nothing was downloaded".into()));
        }
        let cancelled = || ctx.control.get() == CANCEL;
        let Some(audio) = self.tracks.get(1) else {
            if main.ext == "ts" && !main.mp4 {
                if let Some(ff) = &ffmpeg {
                    ctx.progress.set_stage("Converting to MP4…");
                    let out = ctx.work.join("converted.mp4");
                    let mut args = ffmpeg_input(main);
                    args.extend([os("-map"), os("0:v?"), os("-map"), os("0:a?"), os("-c"), os("copy"), out.as_os_str()]);
                    match run_ffmpeg(ff, &args, &cancelled) {
                        Ok(()) => return place(&out, dir, stem, "mp4").map(|f| Output { files: vec![f], note: String::new() }),
                        Err(_) if cancelled() => return Err(Halt::Canceled),
                        Err(e) => eprintln!("[zenith] FFmpeg conversion failed: {e}"),
                    }
                }
            }
            return place(&main.part, dir, stem, &main.ext).map(|f| Output { files: vec![f], note: String::new() });
        };
        ctx.progress.set_stage("Merging audio and video…");
        if main.mp4 && audio.mp4 {
            let out = ctx.work.join("merged.mp4");
            match mp4::merge(&main.part, &audio.part, &out, &cancelled) {
                Ok(()) => return place(&out, dir, stem, "mp4").map(|f| Output { files: vec![f], note: String::new() }),
                Err(_) if cancelled() => return Err(Halt::Canceled),
                Err(e) if e.contains("DRM") => return Err(Halt::Failed(PROTECTED.into())),
                Err(e) => eprintln!("[zenith] native merge failed: {e}"),
            }
        }
        if let Some(ff) = &ffmpeg {
            let webm = |e: &str| matches!(e, "webm" | "weba" | "opus" | "ogg");
            let ext = if webm(&main.ext) && webm(&audio.ext) {
                "webm"
            } else if webm(&main.ext) || webm(&audio.ext) {
                "mkv"
            } else {
                "mp4"
            };
            let out = ctx.work.join(format!("merged.{ext}"));
            let mut args = ffmpeg_input(main);
            args.extend(ffmpeg_input(audio));
            args.extend([os("-map"), os("0:v:0"), os("-map"), os("1:a:0"), os("-c"), os("copy"), out.as_os_str()]);
            match run_ffmpeg(ff, &args, &cancelled) {
                Ok(()) => return place(&out, dir, stem, ext).map(|f| Output { files: vec![f], note: String::new() }),
                Err(_) if cancelled() => return Err(Halt::Canceled),
                Err(e) => eprintln!("[zenith] FFmpeg merge failed: {e}"),
            }
        }
        // Keep both parts rather than nothing. If the second can't be placed, put the first back
        // so a retry finds both.
        let audio_ext = if audio.mp4 { "m4a" } else { audio.ext.as_str() };
        let v = place(&main.part, dir, stem, &main.ext)?;
        let a = match place(&audio.part, dir, &format!("{stem} (audio)"), audio_ext) {
            Ok(a) => a,
            Err(e) => {
                if fs::rename(&v, &main.part).is_err() && fs::copy(&v, &main.part).is_ok() {
                    let _ = fs::remove_file(&v);
                }
                return Err(e);
            }
        };
        Ok(Output {
            files: vec![v, a],
            note: "Video and audio were saved as separate files (install FFmpeg to have them merged)".into(),
        })
    }
}

/// `-protocol_whitelist file -f <demuxer> -i <part>`: FFmpeg reads the part as the container it
/// is, and never follows addresses inside it (a hostile "segment" could be a playlist).
fn ffmpeg_input(t: &Track) -> Vec<&std::ffi::OsStr> {
    let demuxer = if t.mp4 {
        "mov"
    } else {
        match t.ext.as_str() {
            "ts" => "mpegts",
            "mp4" | "m4a" | "m4v" | "mov" => "mov",
            "webm" | "weba" | "mkv" => "matroska",
            "aac" => "aac",
            "mp3" => "mp3",
            "ogg" | "oga" | "opus" => "ogg",
            _ => "",
        }
    };
    let mut args = vec![os("-protocol_whitelist"), os("file")];
    if !demuxer.is_empty() {
        args.extend([os("-f"), os(demuxer)]);
    }
    args.extend([os("-i"), t.part.as_os_str()]);
    args
}

fn os(s: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(s)
}

/// Runs FFmpeg without a console window; kills it when `cancelled` turns true.
fn run_ffmpeg(ffmpeg: &Path, args: &[&std::ffi::OsStr], cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = std::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y"])
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Drain stderr on a thread so a chatty FFmpeg never blocks on a full pipe.
    let stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut e) = stderr {
            let _ = (&mut e).take(64 * 1024).read_to_string(&mut text);
            let _ = io::copy(&mut e, &mut io::sink());
        }
        text
    });
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let err = reader.join().unwrap_or_default();
                return if status.success() {
                    Ok(())
                } else {
                    Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("FFmpeg failed").trim().to_string())
                };
            }
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("canceled".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Picks a free name in `dir` (`stem.ext`, `stem (1).ext`, ...) and claims it.
pub(super) fn reserve(dir: &Path, stem: &str, ext: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    for n in 0..1000 {
        let name = if n == 0 { format!("{stem}.{ext}") } else { format!("{stem} ({n}).{ext}") };
        let path = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free file name"))
}

/// Moves a finished file into the downloads folder under a free name.
fn place(src: &Path, dir: &Path, stem: &str, ext: &str) -> Result<PathBuf, Halt> {
    let dest = reserve(dir, stem, ext).map_err(io_halt)?;
    if fs::rename(src, &dest).is_err() {
        // Different volume: copy, then remove the original. A failed copy leaves nothing behind.
        if let Err(e) = fs::copy(src, &dest) {
            let _ = fs::remove_file(&dest);
            return Err(io_halt(e));
        }
        let _ = fs::remove_file(src);
    }
    Ok(dest)
}

/// Marks a downloaded file as coming from the internet (the `Zone.Identifier` stream that
/// browsers write), so Windows and other apps treat it with the usual care. Query strings are
/// left out: they often carry access tokens.
pub(super) fn mark_of_the_web(path: &Path, host_url: &str, referrer: &str) {
    let clean = |u: &str| -> String {
        let u = u.split(['?', '#']).next().unwrap_or("");
        u.chars().filter(|c| !c.is_control()).take(2048).collect()
    };
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let body = format!("[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl={}\r\nHostUrl={}\r\n", clean(referrer), clean(host_url));
    // Not every file system has alternate data streams (FAT, exFAT); that is fine.
    let _ = fs::write(PathBuf::from(stream), body);
}
