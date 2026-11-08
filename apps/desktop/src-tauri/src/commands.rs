//! Tauri commands for the chrome UI. Only the windows' local chrome webviews (`main`,
//! `incognito-N`) can call these; page webviews are raw WebView2 views without the Tauri bridge.
//! Commands that act on one window take the calling `WebviewWindow`.
//!
//! WebView2 objects are single-threaded, so webview work hops to the UI thread with
//! `run_on_main_thread`; disk and network work runs on blocking worker threads.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::command::{blocking, on_main, on_main_cb, private, refuse_in_incognito, CmdResult};
use crate::host::crx;
use crate::host::extensions::{self, ExtensionInfo};
use crate::host::page_tools;
use crate::host::tab::{self, CreateOptions};
use crate::host::{self, db, LayoutSpec};
use crate::platform::urls;

// ---------------------------------------------------------------------------------------------
// Tabs

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabCreateArgs {
    id: String,
    url: Option<String>,
    #[serde(default)]
    glance: bool,
    #[serde(default)]
    muted: bool,
    zoom: Option<f64>,
    scroll_x: Option<f64>,
    scroll_y: Option<f64>,
    opener_url: Option<String>,
}

#[tauri::command]
pub async fn tab_create(app: AppHandle, window: WebviewWindow, args: TabCreateArgs) -> CmdResult<()> {
    let label = window.label().to_string();
    let url = match &args.url {
        Some(u) => Some(urls::sanitize_navigation(u).ok_or("this address cannot be opened")?),
        None => None,
    };
    let scroll = args.scroll_x.zip(args.scroll_y).filter(|(x, y)| *x > 0.0 || *y > 0.0);
    on_main(&app, move || {
        tab::create(CreateOptions {
            id: args.id,
            window: label,
            url,
            glance: args.glance,
            muted: args.muted,
            zoom: args.zoom,
            scroll,
            opener_url: args.opener_url,
        })
        .map(|_| ())
    })
    .await?
}

#[tauri::command]
pub async fn tab_close(app: AppHandle, id: String) -> CmdResult<()> {
    on_main(&app, move || tab::close(&id)).await
}

#[tauri::command]
pub async fn tab_navigate(app: AppHandle, id: String, url: String) -> CmdResult<()> {
    let url = urls::sanitize_navigation(&url).ok_or("this address cannot be opened")?;
    on_main(&app, move || tab::navigate(&id, &url)).await?
}

#[tauri::command]
pub async fn tab_command(
    app: AppHandle,
    id: String,
    cmd: String,
    arg: Option<serde_json::Value>,
) -> CmdResult<serde_json::Value> {
    if page_tools::handles(&cmd) {
        return on_main_cb(&app, move |done| page_tools::run(&id, &cmd, arg, done)).await?;
    }
    on_main(&app, move || tab::command(&id, &cmd, arg)).await?
}

#[derive(Serialize)]
pub struct Scroll {
    x: f64,
    y: f64,
}

/// Frees a tab's webview and returns its scroll position for restoring later.
#[tauri::command]
pub async fn tab_sleep(app: AppHandle, id: String) -> CmdResult<Option<Scroll>> {
    let scroll = on_main_cb(&app, move |done| tab::sleep(&id, move |s| done(s))).await?;
    Ok(scroll.map(|(x, y)| Scroll { x, y }))
}

/// The natural size of a page's content (sizes extension popups to fit, like Chrome).
#[tauri::command]
pub async fn tab_content_size(app: AppHandle, id: String) -> CmdResult<Option<Scroll>> {
    let size = on_main_cb(&app, move |done| tab::content_size(&id, move |s| done(s))).await?;
    Ok(size.map(|(x, y)| Scroll { x, y }))
}

#[tauri::command]
pub async fn tab_set_glance(app: AppHandle, id: String, glance: bool) -> CmdResult<()> {
    on_main(&app, move || tab::set_glance(&id, glance)).await
}

#[tauri::command]
pub async fn set_layout(app: AppHandle, window: WebviewWindow, spec: LayoutSpec) -> CmdResult<()> {
    let label = window.label().to_string();
    on_main(&app, move || host::set_layout(&label, spec)).await
}

#[tauri::command]
pub async fn focus_view(app: AppHandle, window: WebviewWindow, target: Option<String>) -> CmdResult<()> {
    let label = window.label().to_string();
    on_main(&app, move || host::focus_view(&label, target)).await
}

#[tauri::command]
pub async fn find_in_page(
    app: AppHandle,
    id: String,
    op: String,
    text: Option<String>,
    case_sensitive: Option<bool>,
    whole_word: Option<bool>,
) -> CmdResult<()> {
    on_main(&app, move || {
        tab::find(
            &id,
            &op,
            text.as_deref().unwrap_or(""),
            case_sensitive.unwrap_or(false),
            whole_word.unwrap_or(false),
        )
    })
    .await?
}

#[tauri::command]
pub async fn permission_respond(app: AppHandle, request_id: u32, allow: bool, remember: bool) -> CmdResult<()> {
    on_main(&app, move || tab::respond_permission(request_id, allow, remember)).await
}

#[tauri::command]
pub async fn clear_browsing_data(
    app: AppHandle,
    window: WebviewWindow,
    kinds: Vec<String>,
    since_ms: Option<i64>,
    history: bool,
) -> CmdResult<()> {
    let private = private(&window);
    if history && !private {
        db().call(move |c| crate::history::history_clear(c, since_ms)).await?;
    }
    if kinds.is_empty() {
        return Ok(());
    }
    on_main_cb(&app, move |done| host::clear_browsing_data(private, &kinds, since_ms, move |r| done(r))).await?
}

// ---------------------------------------------------------------------------------------------
// Extensions

#[tauri::command]
pub async fn ext_list(app: AppHandle) -> CmdResult<Vec<ExtensionInfo>> {
    on_main_cb(&app, |done| extensions::list(move |r| done(r))).await?
}

async fn install_staged(app: &AppHandle, dir: PathBuf, store_id: Option<String>) -> CmdResult<ExtensionInfo> {
    on_main_cb(app, move |done| extensions::install_folder(dir, store_id, move |r| done(r))).await?
}

#[tauri::command]
pub async fn ext_install_folder(app: AppHandle, window: WebviewWindow, path: Option<String>) -> CmdResult<Option<ExtensionInfo>> {
    refuse_in_incognito(&window, "Installing extensions")?;
    let picked = match path {
        Some(p) => Some(PathBuf::from(p)),
        None => {
            let app2 = app.clone();
            blocking(move || {
                app2.dialog()
                    .file()
                    .set_title("Choose an unpacked extension folder (with manifest.json)")
                    .blocking_pick_folder()
                    .and_then(|p| p.into_path().ok())
            })
            .await?
        }
    };
    let Some(source) = picked else { return Ok(None) };
    let staged = blocking(move || extensions::stage_folder(&source)).await??;
    install_staged(&app, staged, None).await.map(Some)
}

#[tauri::command]
pub async fn ext_install_crx(app: AppHandle, window: WebviewWindow) -> CmdResult<Option<ExtensionInfo>> {
    refuse_in_incognito(&window, "Installing extensions")?;
    let app2 = app.clone();
    let picked = blocking(move || {
        app2.dialog()
            .file()
            .set_title("Choose a Chrome extension package")
            .add_filter("Chrome extension", &["crx"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
    })
    .await?;
    let Some(file) = picked else { return Ok(None) };
    // Store packages saved by other browsers or mirror sites are named after their id.
    let hint = file.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let staged = blocking(move || {
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        extensions::stage_crx(&bytes)
    })
    .await??;
    let store_id = staged.crx_id.filter(|id| crx::store_id(&hint).as_ref() == Some(id));
    install_staged(&app, staged.dir, store_id).await.map(Some)
}

/// Installs from a Chrome Web Store link or extension id.
#[tauri::command]
pub async fn ext_install_store(app: AppHandle, window: WebviewWindow, input: String) -> CmdResult<ExtensionInfo> {
    refuse_in_incognito(&window, "Installing extensions")?;
    let id = crx::store_id(&input).ok_or("paste a Chrome Web Store link or a 32-letter extension id")?;
    install_from_store(&app, id).await
}

async fn install_from_store(app: &AppHandle, id: String) -> CmdResult<ExtensionInfo> {
    let store_id = id.clone();
    let staged = blocking(move || extensions::stage_crx(&extensions::download_from_store(&id)?)).await??;
    install_staged(app, staged.dir, Some(store_id)).await
}

/// Installs an extension the user accepted in the install prompt.
#[tauri::command]
pub async fn ext_offer_accept(app: AppHandle, window: WebviewWindow, token: u64) -> CmdResult<ExtensionInfo> {
    if private(&window) {
        extensions::discard(token);
        return Err("Installing extensions is not available in an incognito window".into());
    }
    on_main_cb(&app, move |done| extensions::accept(token, move |r| done(r))).await?
}

#[tauri::command]
pub async fn ext_offer_discard(token: u64) -> CmdResult<()> {
    blocking(move || extensions::discard(token)).await
}

/// Newer versions of the extensions installed from the Chrome Web Store.
#[tauri::command]
pub async fn ext_check_updates() -> CmdResult<Vec<extensions::ExtensionUpdate>> {
    blocking(extensions::check_updates).await?
}

/// Reinstalls a store extension from the Web Store's current version.
#[tauri::command]
pub async fn ext_update(app: AppHandle, window: WebviewWindow, id: String) -> CmdResult<ExtensionInfo> {
    refuse_in_incognito(&window, "Updating extensions")?;
    let store_id = extensions::store_source(&id).ok_or("this extension was not installed from the Chrome Web Store")?;
    install_from_store(&app, store_id).await
}

#[tauri::command]
pub async fn ext_remove(app: AppHandle, id: String) -> CmdResult<()> {
    on_main_cb(&app, move |done| extensions::remove(id, move |r| done(r))).await?
}

#[tauri::command]
pub async fn ext_set_enabled(app: AppHandle, id: String, enabled: bool) -> CmdResult<()> {
    on_main_cb(&app, move |done| extensions::set_enabled(id, enabled, move |r| done(r))).await?
}

// ---------------------------------------------------------------------------------------------
// Platform

#[tauri::command]
pub async fn debug_cdp(app: AppHandle, method: String, params: Option<String>) -> CmdResult<String> {
    let params = params.unwrap_or_else(|| "{}".into());
    on_main_cb(&app, move |done| host::chrome_cdp(&method, &params, move |r| done(r))).await?
}

#[tauri::command]
pub async fn debug_layout(app: AppHandle, window: WebviewWindow) -> CmdResult<Option<host::DebugLayout>> {
    let label = window.label().to_string();
    on_main(&app, move || host::debug_layout(&label)).await
}

// ---------------------------------------------------------------------------------------------
// Updates

#[tauri::command]
pub async fn update_status() -> CmdResult<crate::updates::Status> {
    Ok(crate::updates::status())
}

/// Checks now (and downloads a newer version in the background).
#[tauri::command]
pub async fn update_check(app: AppHandle, window: WebviewWindow) -> CmdResult<crate::updates::Status> {
    // Updating restarts Zenith, which would end the incognito session.
    refuse_in_incognito(&window, "Updating")?;
    Ok(crate::updates::check(app).await)
}

/// Quits into the downloaded update; the installer reopens Zenith. Call after the session is saved.
#[tauri::command]
pub async fn update_restart(app: AppHandle, window: WebviewWindow) -> CmdResult<()> {
    refuse_in_incognito(&window, "Updating")?;
    if host::window_labels().iter().any(|l| host::is_private_window(l)) {
        return Err("close the incognito windows first: restarting would end their session".into());
    }
    if !crate::updates::ready() {
        return Err("no update has been downloaded yet".into());
    }
    // The installer closes every Zenith process; other profiles would lose their unsaved state.
    if crate::profiles::others_running() {
        return Err("close your other Zenith profiles first: the update restarts all of them".into());
    }
    crate::updates::restart_after_quit();
    crate::app::request_quit(&app);
    Ok(())
}
