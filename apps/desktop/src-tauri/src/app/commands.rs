//! App-level commands: window UI startup, quitting/closing windows, chrome DevTools, file and
//! folder pickers, shell operations, and clipboard access.

use serde::Serialize;

use super::command::{blocking, on_main, private, CmdResult};
use super::runtime::{AppHandle, WindowHandle};
use crate::host::{self, paths};
use crate::platform::win;
use crate::storage;

pub const CEF_ENGINE_VERSION: &str = "CEF 156.0.31 (Chromium 156.0.8078.100)";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupInfo {
    test_profile: bool,
    profile: crate::profiles::commands::ProfileInfo,
    incognito: bool,
    startup_urls: Vec<String>,
    webview_version: String,
    version: String,
    data_dir: String,
    default_download_dir: String,
    migration_notice: bool,
}

pub async fn ui_ready(app: AppHandle, window: WindowHandle) -> CmdResult<StartupInfo> {
    let incognito = private(&window);
    let label = window.label().to_string();
    let test_profile = host::profile_override().is_some() && !incognito;
    let shown = window.clone();
    on_main(&app, move || {
        let _ = shown.show();
        if shown.label() == host::MAIN
            && super::window::RESTORE_MAXIMIZED.swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            let _ = shown.maximize();
        }
        if !test_profile {
            let _ = shown.set_focus();
        }
    })
    .await?;
    if std::env::var_os("ZENITH_PERF").is_some() {
        if let Some(started) = host::STARTED.get() {
            let json = format!("{{\"startupMs\":{}}}", started.elapsed().as_millis());
            let _ = storage::write_atomic(&paths().local.join("perf.json"), json.as_bytes());
        }
    }
    let startup_urls = super::startup::take_startup_urls(&label);
    let profile = blocking(crate::profiles::commands::current_info).await?;
    Ok(StartupInfo {
        test_profile,
        profile,
        incognito,
        startup_urls,
        webview_version: CEF_ENGINE_VERSION.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: paths().roaming.display().to_string(),
        default_download_dir: app
            .download_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        migration_notice: !incognito && storage::migration::migration_notice_pending(paths()),
    })
}

pub async fn migration_notice_dismiss() -> CmdResult<()> {
    let p = paths().clone();
    blocking(move || storage::migration::dismiss_migration_notice(&p)).await?
}

pub async fn app_quit(app: AppHandle) -> CmdResult<()> {
    super::request_quit(&app);
    Ok(())
}

pub async fn window_close(app: AppHandle, window: WindowHandle) -> CmdResult<()> {
    super::close_window(&app, window.label());
    Ok(())
}

pub async fn chrome_devtools(app: AppHandle, window: WindowHandle) -> CmdResult<()> {
    let label = window.label().to_string();
    on_main(&app, move || host::open_chrome_devtools(&label)).await?
}

pub async fn pick_file(_app: AppHandle) -> CmdResult<Option<String>> {
    blocking(move || {
        win::dialog::pick_file(
            "Open file",
            &[
                (
                    "Web pages and documents",
                    &[
                        "html", "htm", "xhtml", "shtml", "svg", "pdf", "txt", "xml", "json", "png",
                        "jpg", "jpeg", "gif", "webp", "avif", "mp4", "webm", "mp3",
                    ],
                ),
                ("All files", &["*"]),
            ],
        )
        .map(|p| p.display().to_string())
    })
    .await
}

pub async fn open_path(path: String) -> CmdResult<()> {
    if !std::path::Path::new(&path).exists() {
        return Err("the file no longer exists".into());
    }
    blocking(move || win::shell_open(&path)).await?
}

pub async fn show_in_folder(path: String) -> CmdResult<()> {
    blocking(move || win::show_in_folder(&path)).await?
}

pub async fn pick_folder(_app: AppHandle, title: Option<String>) -> CmdResult<Option<String>> {
    blocking(move || win::dialog::pick_folder(title.as_deref()).map(|p| p.display().to_string())).await
}

pub async fn clipboard_write(text: String) -> CmdResult<()> {
    blocking(move || win::set_clipboard_text(&text)).await?
}

pub async fn clipboard_read() -> CmdResult<String> {
    blocking(win::get_clipboard_text).await?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub cef_version: &'static str,
    pub chromium_version: &'static str,
    pub sandbox_enabled: bool,
    pub custom_cef_build: bool,
    pub proprietary_codecs: bool,
    pub widevine_enabled: bool,
    pub patches: &'static [&'static str],
}

pub async fn engine_info() -> CmdResult<EngineInfo> {
    let custom = option_env!("ZENITH_CUSTOM_CEF").is_some();
    Ok(EngineInfo {
        cef_version: "156.0.31+ge603d97",
        chromium_version: "156.0.8078.100",
        sandbox_enabled: true,
        custom_cef_build: custom,
        proprietary_codecs: custom,
        widevine_enabled: custom,
        patches: if custom {
            &[
                "0001-zenith-audio-state-changed",
                "0002-zenith-widevine-cdm-registration",
                "0003-zenith-extensions-tabs-windows-bridge",
                "0004-zenith-disable-vendor-background-traffic",
            ]
        } else {
            &[]
        },
    })
}
