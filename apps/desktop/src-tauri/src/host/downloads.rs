//! Downloads via CEF `DownloadHandler`: progress, pause/resume/cancel, CRX takeover,
//! duplicate detection, custom download folder, fast-download probe, and SQLite history.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use cef::*;
use serde::Serialize;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use super::cef_app::{cef_str, opt_cef_str};
use super::tab::TabShared;
use super::{db, emit, eval_js, post, with_host};
use crate::downloads::DownloadRecord;
use crate::storage::now_ms;

static NEXT_ID: AtomicI64 = AtomicI64::new(0);

pub(crate) fn next_id() -> i64 {
    let _ = NEXT_ID.compare_exchange(0, now_ms(), Ordering::SeqCst, Ordering::SeqCst);
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub struct ActiveDownload {
    pub cef_id: u32,
    pub record: DownloadRecord,
    pub tab: String,
    pub paused: bool,
    pub can_resume: bool,
    pub reason: String,
    pub last_emit: Instant,
    pub origin: Option<Origin>,
    pub callback: Option<DownloadItemCallback>,
    /// An action asked for before CEF handed over the item callback; applied on the first update.
    pub queued: Option<String>,
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
    pub kind: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torrent: Option<crate::torrents::TorrentView>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fast: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Origin {
    pub x: f64,
    pub y: f64,
    pub window: String,
}

pub(crate) fn pointer_in(label: &str) -> Option<Origin> {
    with_host(|h| {
        let hwnd = h.wins.get(label)?.hwnd;
        let scale = h.scale(label);
        let mut p = POINT::default();
        unsafe {
            GetCursorPos(&mut p).ok()?;
            if !ScreenToClient(hwnd, &mut p).as_bool() {
                return None;
            }
        }
        Some(Origin { x: p.x as f64 / scale, y: p.y as f64 / scale, window: label.to_string() })
    })
    .flatten()
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

pub(crate) fn unique_path(dir: &Path, name: &str) -> PathBuf {
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

fn is_crx(uri: &str, path: &str, mime: &str) -> bool {
    let ends = |s: &str| s.to_ascii_lowercase().ends_with(".crx");
    let uri_path = uri.split(['?', '#']).next().unwrap_or("");
    mime.eq_ignore_ascii_case("application/x-chrome-extension")
        || ends(path)
        || (uri.starts_with("http") && ends(uri_path))
}

const READ_BACK_SCRIPT: &str = r#"(async (u) => {
  const b = new Uint8Array(await (await fetch(u)).arrayBuffer());
  let s = '';
  for (let i = 0; i < b.length; i += 32768) s += String.fromCharCode.apply(null, b.subarray(i, i + 32768));
  return btoa(s);
})"#;

fn take_over_crx(browser: Option<&Browser>, shared: &TabShared, uri: String, name: String) {
    use crate::extensions::{offer, refuse, Fetch};
    let page = shared.url();
    let source = crate::platform::urls::host_of(&page)
        .filter(|h| !h.is_empty())
        .unwrap_or("this page")
        .to_string();
    let tab = shared.id();
    if shared.private {
        return refuse(
            &tab,
            source,
            "Extensions can't be installed from an incognito window. Open the page in a normal window.",
        );
    }
    if uri.starts_with("https://") || uri.starts_with("http://") {
        let allow_private = crate::platform::urls::host_of(&page)
            .is_some_and(crate::media::detect::is_private_host);
        let user_agent = crate::media::sniff::default_user_agent();
        return offer(
            tab,
            source,
            Fetch::Url { url: uri, referer: page, user_agent, allow_private, hint: name },
        );
    }
    let Some(browser) = browser.cloned() else {
        return refuse(&tab, source, "The page's extension package could not be read.");
    };
    let expr = format!("{READ_BACK_SCRIPT}({})", serde_json::Value::String(uri));
    post(move || {
        eval_js(&browser, &expr, true, false, move |res| {
            use base64::Engine as _;
            let bytes = res
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok());
            match bytes {
                Some(bytes) => offer(tab, source, Fetch::Bytes { bytes, hint: name }),
                None => refuse(&tab, source, "The page's extension package could not be read."),
            }
        });
    });
}

fn snapshot(d: &ActiveDownload) -> DownloadEvent {
    DownloadEvent {
        file_name: file_name(&d.record.path),
        record: d.record.clone(),
        paused: d.paused,
        can_resume: d.can_resume,
        tab_id: d.tab.clone(),
        reason: d.reason.clone(),
        kind: "file",
        detail: String::new(),
        live: false,
        origin: None,
        torrent: None,
        fast: false,
    }
}

fn publish(id: i64, force: bool) {
    let event = with_host(|h| {
        let d = h.downloads.get_mut(&id)?;
        if !force && d.last_emit.elapsed().as_millis() < 250 {
            return None;
        }
        d.last_emit = Instant::now();
        let origin = d.origin.take();
        Some(DownloadEvent { origin, ..snapshot(d) })
    })
    .flatten();
    if let Some(ev) = event {
        let finished = ev.record.state != "progress";
        if ev.record.state == "completed" {
            crate::downloads::remember_finished(ev.record.id, &ev.record.path);
            if crate::torrents::is_torrent_file(&ev.record.path, &ev.record.mime)
                && crate::torrents::enabled()
            {
                let private = super::tab_is_private(&ev.tab_id);
                if let Err(e) = crate::torrents::add_file(&ev.record.path, &ev.tab_id, private) {
                    super::emit_tab(&ev.tab_id, "torrent-error", e);
                }
            }
        }
        let record = ev.record.clone();
        if (finished || force) && !super::tab_is_private(&ev.tab_id) {
            db().exec(move |c| {
                let _ = crate::downloads::download_upsert(c, &record);
            });
        }
        emit("download", ev);
        if finished {
            with_host(|h| {
                h.downloads.remove(&id);
            });
        }
    }
}

pub(crate) struct Started {
    pub cef_id: u32,
    pub uri: String,
    pub path: String,
    pub mime: String,
    pub total: i64,
    pub origin: Option<Origin>,
}

pub(crate) fn register_download(s: Started, shared: &TabShared) -> i64 {
    let id = next_id();
    let record = DownloadRecord {
        id,
        url: s.uri,
        path: s.path,
        mime: s.mime,
        total: s.total.max(0),
        received: 0,
        state: "progress".into(),
        started: now_ms(),
        ended: None,
    };
    let tab = shared.id();
    with_host(|h| {
        h.downloads.insert(
            id,
            ActiveDownload {
                cef_id: s.cef_id,
                record,
                tab,
                paused: false,
                can_resume: false,
                reason: String::new(),
                last_emit: Instant::now(),
                origin: s.origin,
                callback: None,
                queued: None,
            },
        );
    });
    publish(id, true);
    id
}
/// Opens the PDF viewer in the tab that started the download.
fn load_viewer(browser: Option<&Browser>, url: &str) {
    if let Some(frame) = browser.and_then(|b| b.main_frame()) {
        frame.load_url(Some(&CefString::from(url)));
    }
}

wrap_download_handler! {
    struct TabDownloadHandler {
        shared: Arc<TabShared>,
    }

    impl DownloadHandler {
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> ::std::os::raw::c_int {
            1
        }

        fn on_before_download(
            &self,
            browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            let (Some(item), Some(cb)) = (download_item, callback) else {
                return 0;
            };
            self.shared.restore_page_after_download();
            let uri = cef_str(item.url());
            let mime = cef_str(item.mime_type());
            let name = {
                let s = opt_cef_str(suggested_name);
                if s.is_empty() { "download".into() } else { s }
            };

            if is_crx(&uri, &name, &mime) {
                take_over_crx(browser.as_deref(), &self.shared, uri, name);
                return 1; // Do not call cb.cont -> download is cancelled.
            }

            if crate::pdf::is_viewer_blob(&uri) {
                // Save As from the PDF viewer: the user picks where the edited file goes.
                let target = unique_path(&crate::media::job::download_dir(), &name);
                cb.cont(Some(&CefString::from(target.display().to_string().as_str())), 1);
                return 1;
            }
            if crate::pdf::take_request(&uri) {
                let tmp = crate::pdf::start_capture(item.id(), uri, name);
                cb.cont(Some(&CefString::from(tmp.display().to_string().as_str())), 0);
                return 1;
            }

            if !self.shared.private {
                if let Some(existing) = crate::downloads::already_downloaded(&uri) {
                    let tab_id = self.shared.id();
                    super::emit_tab(
                        &tab_id,
                        "download-exists",
                        crate::downloads::already_message(&existing),
                    );
                    return 1;
                }
            }

            let dir = crate::media::job::download_dir();
            let _ = std::fs::create_dir_all(&dir);
            let target = unique_path(&dir, &name);
            let path = target.display().to_string();
            let cef_id = item.id();
            let total = item.total_bytes();
            let win_label = self.shared.window();
            let origin = pointer_in(&win_label);

            let started = Started { cef_id, uri: uri.clone(), path: path.clone(), mime, total, origin };
            if let Some(risk) = crate::security::classify_download(&name, &uri) {
                let req_id = crate::security::NEXT_SECURITY_REQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let tab = self.shared.id();
                let pending = crate::security::PendingDownloadConfirm::new(tab.clone(), self.shared.clone(), started, cb.clone());
                if with_host(move |h| { h.pending_downloads.insert(req_id, pending); }).is_some() {
                    super::emit_tab(&tab, "download-warning", serde_json::json!({
                        "requestId": req_id,
                        "tabId": tab,
                        "fileName": name,
                        "url": uri,
                        "reason": risk.reason,
                    }));
                }
                return 1;
            }
            if super::fast_download::wanted(&self.shared, &started) {
                super::fast_download::offer(cb.clone(), self.shared.clone(), started);
                return 1;
            }

            let _ = register_download(started, &self.shared);
            cb.cont(Some(&CefString::from(path.as_str())), 0);
            1
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            callback: Option<&mut DownloadItemCallback>,
        ) {
            let Some(item) = download_item else { return };
            let cef_id = item.id();
            if crate::pdf::is_capture(cef_id) {
                let done = item.is_complete() != 0;
                if done || item.is_canceled() != 0 || item.is_interrupted() != 0 {
                    if let Some(viewer) = crate::pdf::finish_capture(cef_id, done) {
                        load_viewer(_browser.as_deref(), &viewer);
                    }
                }
                return;
            }
            let received = item.received_bytes();
            let total = item.total_bytes();
            let full_path = cef_str(item.full_path());
            let complete = item.is_complete() != 0;
            let canceled = item.is_canceled() != 0;
            let interrupted = item.is_interrupted() != 0;
            let paused = item.is_paused() != 0;
            let cb_clone = callback.map(|c| c.clone());

            let force = complete || canceled || interrupted;
            let mut queued = None;
            let zenith_id = with_host(|h| {
                let (&id, d) = h.downloads.iter_mut().find(|(_, d)| d.cef_id == cef_id)?;
                if cb_clone.is_some() {
                    queued = d.queued.take();
                }
                d.record.received = received;
                if total > 0 {
                    d.record.total = total;
                }
                if !full_path.is_empty() {
                    d.record.path = full_path;
                }
                d.paused = paused;
                d.can_resume = paused || interrupted;
                d.callback = cb_clone;
                if complete {
                    d.record.state = "completed".into();
                    d.record.ended = Some(now_ms());
                    d.reason.clear();
                } else if canceled {
                    d.record.state = "canceled".into();
                    d.record.ended = Some(now_ms());
                    d.reason = "Canceled".into();
                } else if interrupted {
                    d.record.state = "interrupted".into();
                    d.record.ended = Some(now_ms());
                    d.reason = "Download interrupted".into();
                } else {
                    d.record.state = "progress".into();
                }
                Some(id)
            })
            .flatten();
            if let (Some(id), Some(a)) = (zenith_id, queued) {
                let _ = action(id, &a);
            }
            if let Some(id) = zenith_id {
                publish(id, force);
            }
        }
    }
}

pub(super) fn create_download_handler(shared: Arc<TabShared>) -> DownloadHandler {
    TabDownloadHandler::new(shared)
}

pub fn action(id: i64, action: &str) -> Result<(), String> {
    if crate::media::job::owns(id) {
        return crate::media::job::action(id, action);
    }
    if crate::torrents::owns(id) {
        return crate::torrents::action(id, action);
    }
    if !matches!(action, "pause" | "resume" | "cancel") {
        return Err(format!("unknown download action {action}"));
    }
    let cb = with_host(|h| {
        let d = h.downloads.get_mut(&id)?;
        match action {
            "pause" => d.paused = true,
            "resume" => d.paused = false,
            _ => {}
        }
        if d.callback.is_none() {
            d.queued = Some(action.to_string());
        }
        Some(d.callback.clone())
    })
    .flatten()
    .ok_or("download is not active")?;
    let Some(cb) = cb else {
        publish(id, true);
        return Ok(());
    };
    match action {
        "pause" => cb.pause(),
        "resume" => cb.resume(),
        _ => cb.cancel(),
    }
    publish(id, true);
    Ok(())
}

pub fn active() -> Vec<DownloadEvent> {
    let mut list: Vec<DownloadEvent> = with_host(|h| h.downloads.values().map(snapshot).collect()).unwrap_or_default();
    list.extend(crate::media::job::active());
    list.extend(crate::torrents::active());
    list
}
