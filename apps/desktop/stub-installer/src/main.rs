#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOG_NOTIFICATIONS,
    TDCBF_CANCEL_BUTTON, TDCBF_RETRY_BUTTON, TDE_CONTENT, TDE_MAIN_INSTRUCTION,
    TDF_CALLBACK_TIMER, TDF_SHOW_MARQUEE_PROGRESS_BAR, TDF_SHOW_PROGRESS_BAR, TDM_CLICK_BUTTON,
    TDM_ENABLE_BUTTON, TDM_SET_ELEMENT_TEXT, TDM_SET_MARQUEE_PROGRESS_BAR,
    TDM_SET_PROGRESS_BAR_MARQUEE, TDM_SET_PROGRESS_BAR_POS, TDM_SET_PROGRESS_BAR_RANGE,
    TDN_BUTTON_CLICKED, TDN_CREATED, TDN_TIMER, TD_ERROR_ICON,
};
use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, IDCANCEL, IDOK, IDRETRY};

const DEFAULT_ENDPOINT: &str =
    "https://github.com/Drakaniia/Zenith/releases/latest/download/latest.json";
const PREVIEW_ENDPOINT: &str =
    "https://github.com/Drakaniia/Zenith/releases/download/preview/preview.json";
const PUBKEY_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDlEOTU0REY4Njc5NTBGQUQKUldTdEQ1Vm4rRTJWblk4K0Vha3hFR1gxYXZiVmhIZzVpZ3V2NU44NGdCc3Uyajg5NGFpWXdXcFUK";
const TARGET_PLATFORM: &str = "windows-x86_64";
const MAX_FEED_BYTES: u64 = 512 * 1024;
const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;

#[cfg(feature = "preview")]
const APP_TITLE: &str = "Zenith Preview Setup";
#[cfg(not(feature = "preview"))]
const APP_TITLE: &str = "Zenith Setup";

#[derive(Deserialize)]
struct Feed {
    version: String,
    #[serde(default)]
    platforms: HashMap<String, FeedPlatform>,
}

#[derive(Deserialize)]
struct FeedPlatform {
    url: String,
    signature: String,
}

#[derive(Clone)]
struct UiSnapshot {
    generation: u64,
    main_text: String,
    content_text: String,
    progress_permille: Option<u16>,
    cancellable: bool,
    done: bool,
    error: Option<String>,
}

struct SharedState {
    ui: Mutex<UiSnapshot>,
    cancelled: AtomicBool,
    cancellable: AtomicBool,
    has_temp_file: AtomicBool,
    cleanup_done: AtomicBool,
    last_seen_gen: Mutex<u64>,
    is_marquee: Mutex<bool>,
    cancel_disabled: Mutex<bool>,
}

impl SharedState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            ui: Mutex::new(UiSnapshot {
                generation: 1,
                main_text: "Installing Zenith".into(),
                content_text: "Checking for the latest version...".into(),
                progress_permille: None,
                cancellable: true,
                done: false,
                error: None,
            }),
            cancelled: AtomicBool::new(false),
            cancellable: AtomicBool::new(true),
            has_temp_file: AtomicBool::new(false),
            cleanup_done: AtomicBool::new(false),
            last_seen_gen: Mutex::new(0),
            is_marquee: Mutex::new(true),
            cancel_disabled: Mutex::new(false),
        })
    }

    fn update(&self, main: impl Into<String>, content: impl Into<String>, progress: Option<u16>) {
        if let Ok(mut ui) = self.ui.lock() {
            ui.generation += 1;
            ui.main_text = main.into();
            ui.content_text = content.into();
            ui.progress_permille = progress;
        }
    }

    fn disable_cancel(&self) {
        self.cancellable.store(false, Ordering::Relaxed);
        if let Ok(mut ui) = self.ui.lock() {
            ui.generation += 1;
            ui.cancellable = false;
        }
    }

    fn finish(&self) {
        if let Ok(mut ui) = self.ui.lock() {
            ui.generation += 1;
            ui.done = true;
        }
    }

    fn fail(&self, err: String) {
        if let Ok(mut ui) = self.ui.lock() {
            ui.generation += 1;
            ui.error = Some(err);
        }
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn feed_endpoint() -> &'static str {
    if cfg!(feature = "preview") {
        PREVIEW_ENDPOINT
    } else {
        DEFAULT_ENDPOINT
    }
}

fn parse_https_url(raw: &str) -> Result<(String, u16, String), String> {
    let rest = raw
        .strip_prefix("https://")
        .ok_or_else(|| format!("Only HTTPS URLs are allowed: {raw}"))?;
    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err(format!("Missing host in URL: {raw}"));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() => {
            let port: u16 = p.parse().map_err(|_| format!("Invalid port in URL: {raw}"))?;
            (h.to_string(), port)
        }
        _ => (authority.to_string(), 443),
    };
    Ok((host, port, path.to_string()))
}

fn parse_feed_json(bytes: &[u8]) -> Result<(String, FeedPlatform), String> {
    let feed: Feed = serde_json::from_slice(bytes).map_err(|e| format!("Invalid release feed: {e}"))?;
    let plat = feed
        .platforms
        .into_iter()
        .find(|(k, _)| k == TARGET_PLATFORM)
        .map(|(_, v)| v)
        .ok_or_else(|| format!("No {TARGET_PLATFORM} package in release feed"))?;
    Ok((feed.version, plat))
}

fn parse_pubkey() -> Result<PublicKey, String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(PUBKEY_B64)
        .map_err(|e| format!("Bad embedded public key encoding: {e}"))?;
    let text = String::from_utf8(decoded).map_err(|e| format!("Bad public key UTF-8: {e}"))?;
    PublicKey::decode(&text).map_err(|e| format!("Bad public key: {e}"))
}

fn verify_signature(path: &Path, sig_b64: &str) -> Result<(), String> {
    let pk = parse_pubkey()?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(sig_b64.trim())
        .map_err(|e| format!("Bad signature encoding: {e}"))?;
    let sig_text = String::from_utf8(decoded).map_err(|e| format!("Bad signature UTF-8: {e}"))?;
    let sig = Signature::decode(&sig_text).map_err(|e| format!("Bad signature: {e}"))?;
    let mut verifier = pk
        .verify_stream(&sig)
        .map_err(|e| format!("Signature header mismatch: {e}"))?;
    let mut file = File::open(path).map_err(|e| format!("Open downloaded installer: {e}"))?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("Read installer: {e}"))?;
        if n == 0 {
            break;
        }
        verifier.update(&buf[..n]);
    }
    verifier
        .finalize()
        .map_err(|e| format!("Signature verification failed: {e}"))
}

struct WinHttpHandle(*mut c_void);
impl Drop for WinHttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn http_download<W: Write>(
    url: &str,
    max_bytes: u64,
    cancelled: &AtomicBool,
    mut sink: W,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let (host, port, path) = parse_https_url(url)?;
    let session = WinHttpHandle(unsafe {
        WinHttpOpen(
            w!("ZenithSetup/1.0"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            None,
            None,
            0,
        )
    });
    if session.0.is_null() {
        return Err("Failed to initialize Windows HTTP session.".into());
    }
    unsafe {
        let _ = WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 60_000);
    }

    let connect = WinHttpHandle(unsafe {
        WinHttpConnect(session.0, &HSTRING::from(host.as_str()), port, 0)
    });
    if connect.0.is_null() {
        return Err(format!("Failed to connect to {host}."));
    }

    let request = WinHttpHandle(unsafe {
        WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            &HSTRING::from(path.as_str()),
            None,
            None,
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err("Failed to open HTTP request.".into());
    }

    unsafe {
        WinHttpSendRequest(request.0, None, None, 0, 0, 0)
            .map_err(|e| format!("Network request failed: {e}"))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())
            .map_err(|e| format!("No response from server: {e}"))?;
    }

    let mut status: u32 = 0;
    let mut status_len = std::mem::size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            None,
            Some(&mut status as *mut u32 as *mut c_void),
            &mut status_len,
            std::ptr::null_mut(),
        )
        .map_err(|e| format!("Failed to read HTTP status: {e}"))?;
    }
    if !(200..=299).contains(&status) {
        return Err(format!("Server returned HTTP {status}."));
    }

    let mut content_len: u32 = 0;
    let mut cl_len = std::mem::size_of::<u32>() as u32;
    let total = unsafe {
        if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
            None,
            Some(&mut content_len as *mut u32 as *mut c_void),
            &mut cl_len,
            std::ptr::null_mut(),
        )
        .is_ok()
            && content_len > 0
        {
            Some(content_len as u64)
        } else {
            None
        }
    };

    let mut downloaded: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Cancelled by user.".into());
        }
        let mut available: u32 = 0;
        unsafe {
            WinHttpQueryDataAvailable(request.0, &mut available)
                .map_err(|e| format!("Download interrupted: {e}"))?;
        }
        if available == 0 {
            break;
        }
        let to_read = (available as usize).min(buf.len());
        let mut read: u32 = 0;
        unsafe {
            WinHttpReadData(
                request.0,
                buf.as_mut_ptr() as *mut c_void,
                to_read as u32,
                &mut read,
            )
            .map_err(|e| format!("Download read failed: {e}"))?;
        }
        if read == 0 {
            break;
        }
        downloaded += read as u64;
        if downloaded > max_bytes {
            return Err("Downloaded file exceeds maximum allowed size.".into());
        }
        sink.write_all(&buf[..read as usize])
            .map_err(|e| format!("Failed writing temporary file: {e}"))?;
        on_progress(downloaded, total);
    }
    sink.flush().map_err(|e| format!("Failed flushing temporary file: {e}"))?;
    Ok(())
}

fn run_install_worker(state: Arc<SharedState>) {
    let result = (|| -> Result<(), String> {
        state.update("Installing Zenith", "Checking for the latest version...", None);
        let mut feed_bytes = Vec::new();
        http_download(
            feed_endpoint(),
            MAX_FEED_BYTES,
            &state.cancelled,
            &mut feed_bytes,
            |_, _| {},
        )?;
        let (version, plat) = parse_feed_json(&feed_bytes)?;

        let tmp_path: PathBuf = std::env::temp_dir().join(format!(
            "zenith-setup-{}-{}.exe",
            std::process::id(),
            version.replace(|c: char| !c.is_ascii_alphanumeric() && c != '.', "_")
        ));

        let dl_result = (|| -> Result<(), String> {
            let file = File::create(&tmp_path)
                .map_err(|e| format!("Could not create temporary file: {e}"))?;
            state.has_temp_file.store(true, Ordering::Relaxed);
            let main_msg = format!("Downloading Zenith {version}...");
            state.update(&main_msg, "Connecting to download server...", Some(0));

            let mut last_key = (u16::MAX, u64::MAX);
            http_download(
                &plat.url,
                MAX_PACKAGE_BYTES,
                &state.cancelled,
                file,
                |done, total| {
                    let done_mb = done as f64 / (1024.0 * 1024.0);
                    let (permille, content) = match total {
                        Some(t) if t > 0 => {
                            let p = ((done.saturating_mul(1000)) / t).min(1000) as u16;
                            let total_mb = t as f64 / (1024.0 * 1024.0);
                            (Some(p), format!("{done_mb:.1} MB of {total_mb:.1} MB"))
                        }
                        _ => (None, format!("{done_mb:.1} MB downloaded")),
                    };
                    let key = (permille.unwrap_or(0) / 5, done / (256 * 1024));
                    if key != last_key {
                        last_key = key;
                        state.update(&main_msg, content, permille);
                    }
                },
            )?;

            state.update(
                format!("Verifying Zenith {version}..."),
                "Checking cryptographic signature...",
                None,
            );
            verify_signature(&tmp_path, &plat.signature)?;

            if state.cancelled.load(Ordering::Relaxed) {
                return Err("Cancelled by user.".into());
            }
            state.disable_cancel();
            state.update(
                format!("Installing Zenith {version}..."),
                "Setting up Zenith on your computer...",
                None,
            );
            let status = std::process::Command::new(&tmp_path)
                .arg("/S")
                .status()
                .map_err(|e| format!("Failed to launch installer: {e}"))?;
            if !status.success() {
                return Err(format!("Installer exited with status {status}."));
            }
            Ok(())
        })();

        let _ = fs::remove_file(&tmp_path);
        state.cleanup_done.store(true, Ordering::Relaxed);
        dl_result
    })();

    match result {
        Ok(()) => state.finish(),
        Err(e) => {
            if !state.cancelled.load(Ordering::Relaxed) {
                state.fail(e);
            }
        }
    }
}

unsafe extern "system" fn td_callback(
    hwnd: HWND,
    msg: TASKDIALOG_NOTIFICATIONS,
    wparam: WPARAM,
    _lparam: LPARAM,
    lp_ref_data: isize,
) -> windows::core::HRESULT {
    let state = &*(lp_ref_data as *const SharedState);
    if msg == TDN_CREATED {
        let _ = SendMessageW(
            hwnd,
            TDM_SET_PROGRESS_BAR_MARQUEE.0 as u32,
            Some(WPARAM(1)),
            Some(LPARAM(30)),
        );
        let ptr = lp_ref_data as *const SharedState;
        Arc::increment_strong_count(ptr);
        let worker_state = Arc::from_raw(ptr);
        thread::spawn(move || run_install_worker(worker_state));
    } else if msg == TDN_BUTTON_CLICKED {
        if wparam.0 as i32 == IDCANCEL.0 {
            if !state.cancellable.load(Ordering::Relaxed) {
                return windows::core::HRESULT(1); // S_FALSE: keep dialog open while NSIS installs
            }
            state.cancelled.store(true, Ordering::Relaxed);
        }
    } else if msg == TDN_TIMER {
        let snapshot = state.ui.lock().ok().map(|u| u.clone());
        if let Some(ui) = snapshot {
            let mut last = state.last_seen_gen.lock().unwrap();
            if ui.generation != *last {
                *last = ui.generation;
                if ui.done || ui.error.is_some() {
                    let _ = SendMessageW(
                        hwnd,
                        TDM_CLICK_BUTTON.0 as u32,
                        Some(WPARAM(IDOK.0 as usize)),
                        Some(LPARAM(0)),
                    );
                    return windows::core::HRESULT(0);
                }
                if !ui.cancellable {
                    let mut disabled = state.cancel_disabled.lock().unwrap();
                    if !*disabled {
                        *disabled = true;
                        let _ = SendMessageW(
                            hwnd,
                            TDM_ENABLE_BUTTON.0 as u32,
                            Some(WPARAM(IDCANCEL.0 as usize)),
                            Some(LPARAM(0)),
                        );
                    }
                }
                let main_w = to_wide(&ui.main_text);
                let content_w = to_wide(&ui.content_text);
                let _ = SendMessageW(
                    hwnd,
                    TDM_SET_ELEMENT_TEXT.0 as u32,
                    Some(WPARAM(TDE_MAIN_INSTRUCTION.0 as usize)),
                    Some(LPARAM(main_w.as_ptr() as isize)),
                );
                let _ = SendMessageW(
                    hwnd,
                    TDM_SET_ELEMENT_TEXT.0 as u32,
                    Some(WPARAM(TDE_CONTENT.0 as usize)),
                    Some(LPARAM(content_w.as_ptr() as isize)),
                );
                let mut marquee = state.is_marquee.lock().unwrap();
                match ui.progress_permille {
                    Some(pos) => {
                        if *marquee {
                            *marquee = false;
                            let _ = SendMessageW(
                                hwnd,
                                TDM_SET_MARQUEE_PROGRESS_BAR.0 as u32,
                                Some(WPARAM(0)),
                                Some(LPARAM(0)),
                            );
                            let range = ((1000u32) << 16) as isize;
                            let _ = SendMessageW(
                                hwnd,
                                TDM_SET_PROGRESS_BAR_RANGE.0 as u32,
                                Some(WPARAM(0)),
                                Some(LPARAM(range)),
                            );
                        }
                        let _ = SendMessageW(
                            hwnd,
                            TDM_SET_PROGRESS_BAR_POS.0 as u32,
                            Some(WPARAM(pos as usize)),
                            Some(LPARAM(0)),
                        );
                    }
                    None => {
                        if !*marquee {
                            *marquee = true;
                            let _ = SendMessageW(
                                hwnd,
                                TDM_SET_MARQUEE_PROGRESS_BAR.0 as u32,
                                Some(WPARAM(1)),
                                Some(LPARAM(0)),
                            );
                            let _ = SendMessageW(
                                hwnd,
                                TDM_SET_PROGRESS_BAR_MARQUEE.0 as u32,
                                Some(WPARAM(1)),
                                Some(LPARAM(30)),
                            );
                        }
                    }
                }
            }
        }
    }
    windows::core::HRESULT(0)
}

fn show_error_dialog(err: &str) -> bool {
    let title_w = to_wide(APP_TITLE);
    let main_w = to_wide("Unable to install Zenith");
    let content_w = to_wide(err);
    let hinst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    let cfg = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        hInstance: hinst.into(),
        dwCommonButtons: TDCBF_RETRY_BUTTON | TDCBF_CANCEL_BUTTON,
        pszWindowTitle: PCWSTR(title_w.as_ptr()),
        Anonymous1: TASKDIALOGCONFIG_0 {
            pszMainIcon: TD_ERROR_ICON,
        },
        pszMainInstruction: PCWSTR(main_w.as_ptr()),
        pszContent: PCWSTR(content_w.as_ptr()),
        ..Default::default()
    };
    let mut btn = 0i32;
    let _ = unsafe { TaskDialogIndirect(&cfg, Some(&mut btn), None, None) };
    btn == IDRETRY.0
}

fn main() {
    loop {
        let state = SharedState::new();
        let state_ptr = Arc::into_raw(Arc::clone(&state));
        let title_w = to_wide(APP_TITLE);
        let main_w = to_wide("Installing Zenith");
        let content_w = to_wide("Checking for the latest version...");
        let hinst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
        let cfg = TASKDIALOGCONFIG {
            cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
            hInstance: hinst.into(),
            dwFlags: TDF_SHOW_PROGRESS_BAR | TDF_SHOW_MARQUEE_PROGRESS_BAR | TDF_CALLBACK_TIMER,
            dwCommonButtons: TDCBF_CANCEL_BUTTON,
            pszWindowTitle: PCWSTR(title_w.as_ptr()),
            Anonymous1: TASKDIALOGCONFIG_0 {
                pszMainIcon: PCWSTR(1usize as *const u16),
            },
            pszMainInstruction: PCWSTR(main_w.as_ptr()),
            pszContent: PCWSTR(content_w.as_ptr()),
            pfCallback: Some(td_callback),
            lpCallbackData: state_ptr as isize,
            ..Default::default()
        };

        let mut btn = 0i32;
        let _ = unsafe { TaskDialogIndirect(&cfg, Some(&mut btn), None, None) };
        unsafe { drop(Arc::from_raw(state_ptr)) };

        if state.cancelled.load(Ordering::Relaxed) || btn == IDCANCEL.0 {
            if state.has_temp_file.load(Ordering::Relaxed) {
                for _ in 0..40 {
                    if state.cleanup_done.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(50));
                }
            }
            break;
        }
        let err = state.ui.lock().ok().and_then(|u| u.error.clone());
        match err {
            Some(msg) if show_error_dialog(&msg) => continue,
            _ => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_urls_and_rejects_http() {
        assert_eq!(
            parse_https_url("https://github.com/Drakaniia/Zenith/releases/latest/download/latest.json").unwrap(),
            ("github.com".into(), 443, "/Drakaniia/Zenith/releases/latest/download/latest.json".into())
        );
        assert_eq!(
            parse_https_url("https://example.com:8443/pkg.exe?a=1").unwrap(),
            ("example.com".into(), 8443, "/pkg.exe?a=1".into())
        );
        assert!(parse_https_url("http://github.com/foo").is_err());
    }

    #[test]
    fn parses_feed_and_embedded_pubkey() {
        assert!(parse_pubkey().is_ok());
        let json = br#"{"version":"0.2.0","platforms":{"windows-x86_64":{"url":"https://example.com/a.exe","signature":"abc"}}}"#;
        let (ver, plat) = parse_feed_json(json).unwrap();
        assert_eq!(ver, "0.2.0");
        assert_eq!(plat.url, "https://example.com/a.exe");
        assert_eq!(plat.signature, "abc");

        let missing = br#"{"version":"0.2.0","platforms":{}}"#;
        assert!(parse_feed_json(missing).is_err());
    }

    #[test]
    fn rejects_invalid_signature() {
        let tmp = std::env::temp_dir().join(format!("zenith-setup-test-{}.bin", std::process::id()));
        fs::write(&tmp, b"not a signed installer").unwrap();
        let res = verify_signature(&tmp, "not-base64!!!");
        let _ = fs::remove_file(&tmp);
        assert!(res.is_err());
    }
}
