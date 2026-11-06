//! Downloads: WebView2 download operations with progress, pause/resume/cancel and history.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Instant;

use serde::Serialize;
use std::result::Result;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, HSTRING, PWSTR};

use super::tab::TabShared;
use super::{config, db, emit, post, with_host};
use crate::store::{now_ms, DownloadRecord};

static NEXT_ID: AtomicI64 = AtomicI64::new(0);

/// Download ids are shared by WebView2 downloads and media jobs.
pub(crate) fn next_id() -> i64 {
    let _ = NEXT_ID.compare_exchange(0, now_ms(), Ordering::SeqCst, Ordering::SeqCst);
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub struct ActiveDownload {
    pub op: ICoreWebView2DownloadOperation,
    pub record: DownloadRecord,
    pub tab: String,
    pub paused: bool,
    pub last_emit: Instant,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEvent {
    #[serde(flatten)]
    pub record: DownloadRecord,
    pub file_name: String,
    pub paused: bool,
    pub can_resume: bool,
    pub tab_id: String,
    pub reason: String,
    /// "file" for WebView2 downloads, "media" for the media downloader's jobs.
    pub kind: &'static str,
    /// Progress detail for media jobs ("Segment 12 of 80", "Recording 3:05").
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// A live recording: it can be stopped (keeping what was recorded) but not paused.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Picks a free file name in `dir`, appending " (n)" like other browsers do.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let p = Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 1..1000 {
        let c = dir.join(format!("{stem} ({n}){ext}"));
        if !c.exists() {
            return c;
        }
    }
    candidate
}

fn state_name(op: &ICoreWebView2DownloadOperation) -> (String, String, bool) {
    unsafe {
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
        let _ = op.State(&mut state);
        let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
        let _ = op.InterruptReason(&mut reason);
        let mut can_resume = BOOL::default();
        let _ = op.CanResume(&mut can_resume);
        let reason_text = match reason {
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NONE => "",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED => "Canceled",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NO_SPACE => "Disk full",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_ACCESS_DENIED => "Access denied",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_TOO_LARGE => "File too large",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_MALICIOUS => "Blocked: malicious file",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_DISCONNECTED => "Network disconnected",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_FAILED => "Network error",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_TIMEOUT => "Network timeout",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_FAILED => "Server error",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_UNAUTHORIZED => "Unauthorized",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_BAD_CONTENT => "File not found",
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_SHUTDOWN => "Browser closed",
            _ => "Failed",
        }
        .to_string();
        let name = if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
            "completed"
        } else if state == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
            if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED {
                "canceled"
            } else {
                "interrupted"
            }
        } else {
            "progress"
        };
        (name.to_string(), reason_text, can_resume.as_bool())
    }
}

fn snapshot(d: &mut ActiveDownload) -> DownloadEvent {
    unsafe {
        let mut received = 0i64;
        let _ = d.op.BytesReceived(&mut received);
        let mut total = 0i64;
        let _ = d.op.TotalBytesToReceive(&mut total);
        let mut path = PWSTR::null();
        let _ = d.op.ResultFilePath(&mut path);
        let path = take_pwstr(path);
        if !path.is_empty() {
            d.record.path = path;
        }
        d.record.received = received;
        d.record.total = total.max(0);
    }
    let (state, reason, can_resume) = state_name(&d.op);
    // A paused download reports "interrupted" but can resume.
    let state = if d.paused && state == "interrupted" { "progress".to_string() } else { state };
    if state != "progress" && d.record.ended.is_none() {
        d.record.ended = Some(now_ms());
    }
    d.record.state = state;
    DownloadEvent {
        file_name: file_name(&d.record.path),
        record: d.record.clone(),
        paused: d.paused,
        can_resume,
        tab_id: d.tab.clone(),
        reason,
        kind: "file",
        detail: String::new(),
        live: false,
    }
}

fn publish(id: i64, force: bool) {
    let event = with_host(|h| {
        let d = h.downloads.get_mut(&id)?;
        if !force && d.last_emit.elapsed().as_millis() < 250 {
            return None;
        }
        d.last_emit = Instant::now();
        Some(snapshot(d))
    })
    .flatten();
    if let Some(ev) = event {
        let finished = ev.record.state != "progress";
        let record = ev.record.clone();
        // Downloads from incognito windows are not kept in the download history.
        if (finished || force) && !super::tab_is_private(&ev.tab_id) {
            db().exec(move |c| {
                let _ = crate::store::download_upsert(c, &record);
            });
        }
        emit("download", ev);
        if finished {
            release(id);
        }
    }
}

/// Forgets a finished download and frees a closed tab that was only kept alive for it.
fn release(id: i64) {
    let zombie = with_host(|h| {
        let d = h.downloads.remove(&id)?;
        let still_busy = h.downloads.values().any(|o| o.tab == d.tab);
        if still_busy {
            None
        } else {
            h.zombies.remove(&d.tab)
        }
    })
    .flatten();
    if let Some(tab) = zombie {
        post(move || drop(tab));
    }
}

pub fn attach(core: &ICoreWebView2, shared: Rc<TabShared>) {
    let Ok(c4) = core.cast::<ICoreWebView2_4>() else { return };
    let mut token = 0i64;
    unsafe {
        let _ = c4.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let op = args.DownloadOperation()?;
                // Zenith shows its own downloads UI instead of the default flyout.
                args.SetHandled(true)?;
                let mut path = PWSTR::null();
                args.ResultFilePath(&mut path)?;
                let mut path = take_pwstr(path);
                if let Some(dir) = config().download_dir.filter(|d| d.is_dir()) {
                    let target = unique_path(&dir, &file_name(&path));
                    path = target.display().to_string();
                    args.SetResultFilePath(&HSTRING::from(path.as_str()))?;
                }
                let mut uri = PWSTR::null();
                op.Uri(&mut uri)?;
                let mut mime = PWSTR::null();
                op.MimeType(&mut mime)?;
                let mut total = 0i64;
                op.TotalBytesToReceive(&mut total)?;
                let id = next_id();
                let record = DownloadRecord {
                    id,
                    url: take_pwstr(uri),
                    path,
                    mime: take_pwstr(mime),
                    total: total.max(0),
                    received: 0,
                    state: "progress".into(),
                    started: now_ms(),
                    ended: None,
                };
                let mut t = 0i64;
                op.add_BytesReceivedChanged(
                    &BytesReceivedChangedEventHandler::create(Box::new(move |_, _| {
                        publish(id, false);
                        Ok(())
                    })),
                    &mut t,
                )?;
                op.add_StateChanged(
                    &StateChangedEventHandler::create(Box::new(move |_, _| {
                        publish(id, true);
                        Ok(())
                    })),
                    &mut t,
                )?;
                let tab = shared.id.clone();
                with_host(|h| {
                    h.downloads.insert(
                        id,
                        ActiveDownload { op, record, tab, paused: false, last_emit: Instant::now() },
                    );
                });
                publish(id, true);
                Ok(())
            })),
            &mut token,
        );
    }
}

pub fn action(id: i64, action: &str) -> Result<(), String> {
    if crate::media::job::owns(id) {
        return crate::media::job::action(id, action);
    }
    let op = with_host(|h| {
        let d = h.downloads.get_mut(&id)?;
        match action {
            "pause" => d.paused = true,
            "resume" => d.paused = false,
            _ => {}
        }
        Some(d.op.clone())
    })
    .flatten()
    .ok_or("download is not active")?;
    unsafe {
        match action {
            "pause" => op.Pause(),
            "resume" => op.Resume(),
            "cancel" => op.Cancel(),
            other => return Err(format!("unknown download action {other}")),
        }
    }
    .map_err(|e| e.to_string())?;
    publish(id, true);
    Ok(())
}

/// Downloads currently tracked in memory (running, paused or just finished), media jobs included.
pub fn active() -> Vec<DownloadEvent> {
    let mut list: Vec<DownloadEvent> = with_host(|h| h.downloads.values_mut().map(snapshot).collect()).unwrap_or_default();
    list.extend(crate::media::job::active());
    list
}
