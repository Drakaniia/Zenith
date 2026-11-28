//! Commands for browser extensions: listing them, installing them (unpacked folder, `.crx`
//! package, Chrome Web Store), the install prompt, updates, enabling and removing.

use std::path::PathBuf;

use crate::app::runtime::{AppHandle, WindowHandle};
use crate::platform::win::dialog;

use super::{crx, ExtensionInfo};
use crate::app::command::{blocking, on_main_cb, private, refuse_in_incognito, CmdResult};
use crate::extensions;

pub async fn ext_list(app: AppHandle) -> CmdResult<Vec<ExtensionInfo>> {
    on_main_cb(&app, |done| extensions::list(move |r| done(r))).await?
}

async fn install_staged(app: &AppHandle, dir: PathBuf, store_id: Option<String>) -> CmdResult<ExtensionInfo> {
    let ext = on_main_cb(app, move |done| extensions::install_folder(dir, store_id, move |r| done(r))).await?;
    extensions::private_profile::sync();
    ext
}

pub async fn ext_install_folder(app: AppHandle, window: WindowHandle, path: Option<String>) -> CmdResult<Option<ExtensionInfo>> {
    refuse_in_incognito(&window, "Installing extensions")?;
    let picked = match path {
        Some(p) => Some(PathBuf::from(p)),
        None => {
            blocking(move || dialog::pick_folder(Some("Choose an unpacked extension folder (with manifest.json)"))).await?
        }
    };
    let Some(source) = picked else { return Ok(None) };
    let staged = blocking(move || extensions::stage_folder(&source)).await??;
    install_staged(&app, staged, None).await.map(Some)
}

pub async fn ext_install_crx(app: AppHandle, window: WindowHandle) -> CmdResult<Option<ExtensionInfo>> {
    refuse_in_incognito(&window, "Installing extensions")?;
    let picked = blocking(move || {
        dialog::pick_file("Choose a Chrome extension package", &[("Chrome extension", &["crx"])])
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
pub async fn ext_install_store(app: AppHandle, window: WindowHandle, input: String) -> CmdResult<ExtensionInfo> {
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
pub async fn ext_offer_accept(app: AppHandle, window: WindowHandle, token: u64) -> CmdResult<ExtensionInfo> {
    if private(&window) {
        extensions::discard(token);
        return Err("Installing extensions is not available in an incognito window".into());
    }
    let ext = on_main_cb(&app, move |done| extensions::accept(token, move |r| done(r))).await?;
    extensions::private_profile::sync();
    ext
}

pub async fn ext_offer_discard(token: u64) -> CmdResult<()> {
    blocking(move || extensions::discard(token)).await
}

/// Newer versions of the extensions installed from the Chrome Web Store.
pub async fn ext_check_updates() -> CmdResult<Vec<extensions::ExtensionUpdate>> {
    blocking(extensions::check_updates).await?
}

/// Reinstalls a store extension from the Web Store's current version.
pub async fn ext_update(app: AppHandle, window: WindowHandle, id: String) -> CmdResult<ExtensionInfo> {
    refuse_in_incognito(&window, "Updating extensions")?;
    let store_id = extensions::store_source(&id).ok_or("this extension was not installed from the Chrome Web Store")?;
    install_from_store(&app, store_id).await
}

pub async fn ext_remove(app: AppHandle, id: String) -> CmdResult<()> {
    let r = on_main_cb(&app, move |done| extensions::remove(id, move |r| done(r))).await?;
    extensions::private_profile::sync();
    r
}

pub async fn ext_set_incognito(window: WindowHandle, id: String, allowed: bool) -> CmdResult<()> {
    refuse_in_incognito(&window, "Changing where an extension runs")?;
    blocking(move || extensions::set_incognito_allowed(&id, allowed)).await??;
    extensions::private_profile::sync();
    Ok(())
}

pub async fn ext_set_enabled(app: AppHandle, id: String, enabled: bool) -> CmdResult<()> {
    let r = on_main_cb(&app, move |done| extensions::set_enabled(id, enabled, move |r| done(r))).await?;
    extensions::private_profile::sync();
    r
}

pub async fn ext_sync_state(snapshot: extensions::bridge::WindowSnapshot) -> CmdResult<()> {
    if let Ok(mut b) = extensions::bridge::BRIDGE.write() {
        b.sync_window(snapshot);
    }
    Ok(())
}

pub async fn ext_badges() -> CmdResult<Vec<extensions::bridge::BadgeEvent>> {
    let b = extensions::bridge::BRIDGE.read().map_err(|_| "extension bridge lock poisoned".to_string())?;
    Ok(b.all_badges())
}

pub async fn ext_set_badge(
    extension_id: String,
    tab_id: Option<String>,
    badge: extensions::bridge::ActionBadge,
) -> CmdResult<extensions::bridge::BadgeEvent> {
    let ev = extensions::bridge::BRIDGE
        .write()
        .map_err(|_| "extension bridge lock poisoned".to_string())?
        .set_badge(extension_id, tab_id, badge);
    crate::host::emit("ext-badge", ev.clone());
    Ok(ev)
}

pub async fn ext_query_tabs(
    window: WindowHandle,
    query: extensions::bridge::TabQuery,
) -> CmdResult<Vec<extensions::bridge::ChromeTab>> {
    let b = extensions::bridge::BRIDGE.read().map_err(|_| "extension bridge lock poisoned".to_string())?;
    Ok(b.query_tabs(
        Some(window.label()),
        private(&window),
        true,
        &std::collections::HashSet::new(),
        &query,
    ))
}

pub async fn ext_native_hosts() -> CmdResult<Vec<extensions::native_messaging::NativeHostInfo>> {
    blocking(extensions::native_messaging::list_hosts).await
}

pub async fn ext_native_host_consent(window: WindowHandle, host_name: String, allowed: bool) -> CmdResult<()> {
    refuse_in_incognito(&window, "Changing native messaging permissions")?;
    blocking(move || extensions::native_messaging::set_host_consent(&host_name, allowed)).await?
}

pub async fn ext_trigger_action(
    app: AppHandle,
    id: String,
    tab_id: Option<String>,
) -> CmdResult<extensions::runtime::ActionOutcome> {
    on_main_cb(&app, move |done| {
        extensions::runtime::trigger_action(id, tab_id, move |r| done(r))
    })
    .await?
}
