//! App-level commands: a window's UI starting up, quitting and closing windows, the UI's own
//! DevTools, file and folder pickers, and the shell (open a file, show it in its folder, the
//! clipboard).

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use super::command::{blocking, on_main, private, CmdResult};
use crate::host::{self, paths};
use crate::{store, win};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupInfo {
    /// Running on a separate profile (ZENITH_PROFILE_DIR), e.g. an automated test instance.
    test_profile: bool,
    /// The browser profile this process runs (see profiles.rs).
    profile: crate::profile_commands::ProfileInfo,
    /// An incognito window (InPrivate pages, nothing saved).
    incognito: bool,
    startup_urls: Vec<String>,
    windows11: bool,
    webview_version: String,
    version: String,
    data_dir: String,
    default_download_dir: String,
}

#[tauri::command]
pub async fn ui_ready(app: AppHandle, window: WebviewWindow) -> CmdResult<StartupInfo> {
    let incognito = private(&window);
    let label = window.label().to_string();
    let test_profile = host::profile_override().is_some() && !incognito;
    let shown = window.clone();
    on_main(&app, move || {
        // Shown outside the host borrow: showing the window sends WM_SIZE, which relayouts tabs.
        let _ = shown.show();
        if shown.label() == host::MAIN && super::window::RESTORE_MAXIMIZED.swap(false, std::sync::atomic::Ordering::SeqCst) {
            let _ = shown.maximize();
        }
        // A separate-profile (test) instance must not steal focus from the user's window.
        if !test_profile {
            let _ = shown.set_focus();
        }
    })
    .await?;
    // Time from process start until the chrome UI is shown (read by scripts/measure.ps1).
    if std::env::var_os("ZENITH_PERF").is_some() {
        if let Some(started) = host::STARTED.get() {
            let json = format!("{{\"startupMs\":{}}}", started.elapsed().as_millis());
            let _ = store::write_atomic(&paths().local.join("perf.json"), json.as_bytes());
        }
    }
    let startup_urls = super::startup::take_startup_urls(&label);
    let profile = blocking(crate::profile_commands::current_info).await?;
    Ok(StartupInfo {
        test_profile,
        profile,
        incognito,
        startup_urls,
        windows11: win::is_windows_11(),
        webview_version: wry::webview_version().unwrap_or_default(),
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: paths().roaming.display().to_string(),
        default_download_dir: app
            .path()
            .download_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn app_quit(app: AppHandle) -> CmdResult<()> {
    super::request_quit(&app);
    Ok(())
}

/// Closes the calling window (after its UI saved what it keeps). Closing the last window quits.
#[tauri::command]
pub async fn window_close(app: AppHandle, window: WebviewWindow) -> CmdResult<()> {
    super::close_window(&app, window.label());
    Ok(())
}

/// Firefox's Browser Toolbox and Browser Console: DevTools for this window's own UI. Release
/// builds turn the UI's DevTools off; asking for them explicitly turns them on for the window.
#[tauri::command]
pub async fn chrome_devtools(app: AppHandle, window: WebviewWindow) -> CmdResult<()> {
    on_main(&app, move || {
        window
            .with_webview(|pw| unsafe {
                use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings;
                let Ok(core) = pw.controller().CoreWebView2() else { return };
                if let Ok(settings) = core.Settings() {
                    let settings: ICoreWebView2Settings = settings;
                    let _ = settings.SetAreDevToolsEnabled(true);
                }
                let _ = core.OpenDevToolsWindow();
            })
            .map_err(|e| e.to_string())
    })
    .await?
}

/// Ctrl+O: a file to open in a tab.
#[tauri::command]
pub async fn pick_file(app: AppHandle) -> CmdResult<Option<String>> {
    blocking(move || {
        app.dialog()
            .file()
            .set_title("Open file")
            .add_filter("Web pages and documents", &["html", "htm", "xhtml", "shtml", "svg", "pdf", "txt", "xml", "json", "png", "jpg", "jpeg", "gif", "webp", "avif", "mp4", "webm", "mp3"])
            .add_filter("All files", &["*"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
}

#[tauri::command]
pub async fn open_path(path: String) -> CmdResult<()> {
    if !std::path::Path::new(&path).exists() {
        return Err("the file no longer exists".into());
    }
    blocking(move || win::shell_open(&path)).await?
}

#[tauri::command]
pub async fn show_in_folder(path: String) -> CmdResult<()> {
    blocking(move || win::show_in_folder(&path)).await?
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle, title: Option<String>) -> CmdResult<Option<String>> {
    blocking(move || {
        let mut dialog = app.dialog().file();
        if let Some(t) = title {
            dialog = dialog.set_title(t);
        }
        dialog
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
}

#[tauri::command]
pub async fn clipboard_write(text: String) -> CmdResult<()> {
    blocking(move || win::set_clipboard_text(&text)).await?
}
