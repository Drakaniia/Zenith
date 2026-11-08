//! Recording a live HLS stream until it ends, fails for good or the user stops it.

use std::fs;
use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::fetch::{append, fetch_segment, io_halt, Ctx, Halt};
use super::state::{CANCEL, STOP};
use super::track::{download, media_playlist, Track};
use super::{Source, LIVE_FAILURES, PROTECTED};
use crate::media::dash::SegRef;
use crate::media::hls;
use crate::media::http;

/// Records a live HLS track until the user stops it or the stream ends. Stopping, the end of the
/// broadcast and a stream that keeps failing after something was recorded all keep the file.
pub(super) fn record(ctx: &Ctx, t: &mut Track) -> Result<(), Halt> {
    t.recover(&ctx.progress.tracks[t.index]);
    if t.complete {
        return Ok(());
    }
    match record_until_end(ctx, t) {
        Err(Halt::Stopped) => Ok(()),
        other => other,
    }
}

fn record_until_end(ctx: &Ctx, t: &mut Track) -> Result<(), Halt> {
    let Source::Hls(url) = t.source.clone() else { return download(ctx, t) };
    let p = &ctx.progress.tracks[t.index];
    let mut out = t.open_part()?;
    let (mut playlist_failures, mut segment_failures) = (0u32, 0u32);
    let wait = |d: Duration| ctx.live_wait.unwrap_or(d);
    // What to return when the stream stays unreachable: keep a recording that has data.
    let ended = |t: &Track, error: String| if t.appended > 0 { Ok(()) } else { Err(Halt::Failed(error)) };
    loop {
        match ctx.control.get() {
            CANCEL => return Err(Halt::Canceled),
            STOP => return Ok(()),
            _ if ctx.aborted() => return Err(Halt::Failed(String::new())),
            _ => {}
        }
        let pl = match media_playlist(ctx, &url) {
            Ok(pl) => {
                playlist_failures = 0;
                pl
            }
            Err(Halt::Failed(e)) if e == PROTECTED || ctx.aborted() => return Err(Halt::Failed(e)),
            Err(Halt::Failed(e)) => {
                playlist_failures += 1;
                if playlist_failures >= LIVE_FAILURES {
                    return ended(t, e);
                }
                ctx.control.sleep(wait(Duration::from_secs(2)));
                continue;
            }
            Err(h) => return Err(h),
        };
        if !t.init_done {
            t.ext = pl.container().to_string();
            t.mp4 = pl.init.is_some() || t.ext == "mp4";
            if let Some(init) = &pl.init {
                let init = SegRef { url: init.uri.clone(), range: init.range };
                t.write_init(ctx, &mut out, &init)?;
            }
            t.init_done = true;
        }
        let newest = pl.segments.last().map(|s| s.sequence);
        let fresh: Vec<&hls::Segment> = match t.last_seq {
            None if pl.ended => pl.segments.iter().collect(),
            // Start near the live edge, like a player.
            None => pl.segments.iter().skip(pl.segments.len().saturating_sub(3)).collect(),
            // The numbering restarted (encoder restart, or no EXT-X-MEDIA-SEQUENCE in a sliding
            // window): anything not recorded recently is new.
            Some(last) if newest.is_some_and(|n| n < last) => {
                pl.segments.iter().filter(|s| !t.recent.contains(&s.uri)).collect()
            }
            Some(last) => pl.segments.iter().filter(|s| s.sequence > last).collect(),
        };
        for seg in fresh {
            if ctx.stop_now() {
                break;
            }
            let path = ctx.work.join(format!("t{}-live-{}.seg", t.index, seg.sequence));
            match fetch_segment(ctx, p, &SegRef { url: seg.uri.clone(), range: seg.range }, &path) {
                Ok(_) => {
                    let len = append(&mut out, &path).map_err(io_halt)?;
                    let _ = fs::remove_file(&path);
                    t.part_len += len;
                    t.appended += 1;
                    p.appended.fetch_add(len, Ordering::Relaxed);
                    p.done.fetch_add(1, Ordering::Relaxed);
                    p.recorded_ms.fetch_add((seg.duration.max(0.0) * 1000.0) as u64, Ordering::Relaxed);
                    segment_failures = 0;
                }
                // The disk is full or the folder is gone: say so instead of recording nothing.
                Err(Some(e @ http::Error::Io(_))) => return Err(Halt::Failed(e.to_string())),
                // One segment can fall out of the live window; a run of failures (expired
                // tokens, the broadcast taken down) ends the recording.
                Err(Some(e)) => {
                    segment_failures += 1;
                    if segment_failures >= LIVE_FAILURES {
                        let _ = out.flush();
                        return ended(t, e.to_string());
                    }
                }
                Err(None) => break,
            }
            t.last_seq = Some(seg.sequence);
            t.recent.push_back(seg.uri.clone());
            if t.recent.len() > 64 {
                t.recent.pop_front();
            }
        }
        let _ = out.flush();
        if pl.ended {
            return Ok(());
        }
        let refresh = if pl.target_duration.is_finite() { (pl.target_duration / 2.0).clamp(1.0, 10.0) } else { 2.0 };
        ctx.control.sleep(wait(Duration::from_secs_f64(refresh)));
    }
}
