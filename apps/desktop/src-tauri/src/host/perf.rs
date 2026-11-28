//! `ZENITH_PERF` layout trace: one line per `set_layout` in `<local>/perf-layout.log`.
//! Columns: ms since first call, gap since previous call, IPC-to-main-thread queue, native
//! apply time, inner browser HWND resizes in that apply, first pane x/w (CSS px).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

struct Trace {
    file: File,
    first: Instant,
    last: Instant,
}

static TRACE: OnceLock<Option<Mutex<Trace>>> = OnceLock::new();
static RESIZES: AtomicU32 = AtomicU32::new(0);

fn trace() -> Option<&'static Mutex<Trace>> {
    TRACE
        .get_or_init(|| {
            std::env::var_os("ZENITH_PERF")?;
            let path = super::paths().local.join("perf-layout.log");
            let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(path).ok()?;
            let _ = writeln!(file, "t_ms\tgap_ms\tqueue_ms\tapply_ms\tresizes\tpane");
            let now = Instant::now();
            Some(Mutex::new(Trace { file, first: now, last: now }))
        })
        .as_ref()
}

pub fn enabled() -> bool {
    trace().is_some()
}

pub fn note_resize() {
    RESIZES.fetch_add(1, Ordering::Relaxed);
}

pub fn record(queued: Instant, started: Instant, pane: Option<(f64, f64)>) {
    let done = Instant::now();
    let Some(t) = trace() else { return };
    let Ok(mut t) = t.lock() else { return };
    let ms = |a: Instant, b: Instant| b.saturating_duration_since(a).as_secs_f64() * 1000.0;
    let line = format!(
        "{:.1}\t{:.1}\t{:.2}\t{:.2}\t{}\t{}",
        ms(t.first, queued),
        ms(t.last, queued),
        ms(queued, started),
        ms(started, done),
        RESIZES.swap(0, Ordering::Relaxed),
        pane.map_or(String::from("-"), |(x, w)| format!("{x:.0},{w:.0}")),
    );
    t.last = queued;
    let _ = writeln!(t.file, "{line}");
}
