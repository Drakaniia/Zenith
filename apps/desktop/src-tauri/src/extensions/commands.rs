//! Commands for browser extensions: listing them, installing them (unpacked folder, `.crx`
//! package, Chrome Web Store), the install prompt, updates, enabling and removing.

use std::path::PathBuf;

use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use super::{crx, ExtensionInfo};
use crate::app::command::{blocking, on_main_cb, private, refuse_in_incognito, CmdResult};
use crate::extensions;

#[tauri::command]
pub async fn ext_list(app: AppHandle) -> CmdResult<Vec<ExtensionInfo>> {
    on_main_cb(&app, |done| extensions::list(move |r| done(r))).await?
}

async fn install_staged(app: &AppHandle, dir: PathBuf, store_id: Option<String>) -> CmdResult<ExtensionInfo> {
    let ext = on_main_cb(app, move |done| extensions::install_folder(dir, store_id, move |r| done(r))).await?;
    extensions::private_profile::sync();
    ext
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
    let ext = on_main_cb(&app, move |done| extensions::accept(token, move |r| done(r))).await?;
    extensions::private_profile::sync();
    ext
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
    let r = on_main_cb(&app, move |done| extensions::remove(id, move |r| done(r))).await?;
    extensions::private_profile::sync();
    r
}

#[tauri::command]
pub async fn ext_set_incognito(window: WebviewWindow, id: String, allowed: bool) -> CmdResult<()> {
    refuse_in_incognito(&window, "Changing where an extension runs")?;
    blocking(move || extensions::set_incognito_allowed(&id, allowed)).await??;
    extensions::private_profile::sync();
    Ok(())
}

#[tauri::command]
pub async fn ext_set_enabled(app: AppHandle, id: String, enabled: bool) -> CmdResult<()> {
    let r = on_main_cb(&app, move |done| extensions::set_enabled(id, enabled, move |r| done(r))).await?;
    extensions::private_profile::sync();
    r
}
