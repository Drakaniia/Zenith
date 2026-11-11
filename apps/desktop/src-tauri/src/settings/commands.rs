//! Commands for settings: the app-wide runtime settings and the window's appearance the UI
//! pushes, the settings file, and uploaded start-page backgrounds.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, WebviewWindow};

use super::wallpapers::{valid_wallpaper_data, wallpaper_file};
use crate::app::command::{blocking, on_main, private, refuse_in_incognito, CmdResult};
use crate::host::{self, paths, RuntimeConfig};
use crate::storage;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    /// "system" | "light" | "dark"
    theme: String,
    page_background: [u8; 3],
    /// prefers-color-scheme for web content: "system" | "light" | "dark"
    content_scheme: String,
}

#[tauri::command]
pub async fn apply_settings(
    app: AppHandle,
    window: WebviewWindow,
    config: RuntimeConfig,
    appearance: Appearance,
) -> CmdResult<()> {
    // App-wide settings come from the normal window; an incognito window only styles itself
    // (its settings changes last only as long as it is open).
    if !private(&window) {
        host::apply_config(config);
    } else {
        host::set_extensions_in_incognito(config.extensions_in_incognito);
    }
    let theme = match appearance.theme.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    };
    let _ = window.set_theme(theme);
    let bg = appearance.page_background;
    let scheme = appearance.content_scheme;
    on_main(&app, move || {
        host::set_page_background((bg[0], bg[1], bg[2]));
        host::set_content_color_scheme(&scheme);
    })
    .await
}

#[tauri::command]
pub async fn settings_load() -> CmdResult<Option<String>> {
    blocking(|| storage::read_text(&paths().settings())).await
}

#[derive(Serialize, Clone)]
struct SettingsChanged {
    /// Window that saved; it already has the new values.
    origin: String,
}

/// Incognito windows start from the user's settings; their own changes are not saved. The other
/// normal windows hear about a change through `settings-changed` and reload.
#[tauri::command]
pub async fn settings_save(window: WebviewWindow, data: String) -> CmdResult<()> {
    if private(&window) {
        return Ok(());
    }
    blocking(move || storage::write_atomic(&paths().settings(), data.as_bytes()).map_err(|e| e.to_string())).await??;
    host::emit("settings-changed", SettingsChanged { origin: window.label().to_string() });
    Ok(())
}

/// Stores a start-page background the user uploaded (a downscaled image as a data URL).
#[tauri::command]
pub async fn wallpaper_save(window: WebviewWindow, id: String, data: String) -> CmdResult<()> {
    refuse_in_incognito(&window, "Adding a background")?;
    if !valid_wallpaper_data(&data) {
        return Err("This image can't be used as a background. Try a JPEG, PNG or WebP file.".into());
    }
    let path = wallpaper_file(&id)?;
    blocking(move || storage::write_atomic(&path, data.as_bytes()).map_err(|e| e.to_string())).await?
}

#[tauri::command]
pub async fn wallpaper_load(id: String) -> CmdResult<Option<String>> {
    let path = wallpaper_file(&id)?;
    blocking(move || storage::read_text(&path)).await
}

#[tauri::command]
pub async fn wallpaper_delete(window: WebviewWindow, id: String) -> CmdResult<()> {
    if private(&window) {
        return Ok(());
    }
    let path = wallpaper_file(&id)?;
    blocking(move || match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    })
    .await?
}
