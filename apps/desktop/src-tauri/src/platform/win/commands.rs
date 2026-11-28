//! Commands for Windows integration: Zenith as the default browser, and memory use.

use crate::app::command::{blocking, CmdResult};
use crate::platform::win;

pub async fn default_browser_status() -> CmdResult<win::DefaultBrowserStatus> {
    blocking(win::default_browser_status).await
}

/// Registers Zenith with Windows and (by default) opens the Default apps page so the user can
/// pick it; Windows does not let apps make themselves the default silently.
pub async fn default_browser_register(open_settings: Option<bool>) -> CmdResult<()> {
    blocking(move || {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        win::register_browser(&exe)?;
        if open_settings.unwrap_or(true) {
            win::open_default_apps_settings()?;
        }
        Ok(())
    })
    .await?
}

pub async fn default_browser_unregister() -> CmdResult<()> {
    blocking(win::unregister_browser).await
}

pub async fn memory_stats() -> CmdResult<win::MemoryStats> {
    blocking(win::memory_stats).await
}

pub async fn system_memory_load() -> CmdResult<u32> {
    let load = win::system_memory_load();
    if load >= crate::host::tab::warm::MEMORY_PRESSURE_LOAD_PCT {
        let _ = crate::app::runtime::post_ui(move || crate::host::tab::release_if_pressured(load));
    }
    Ok(load)
}
