//! The command that opens an incognito window.

use crate::app::command::CmdResult;
use crate::platform::urls;

/// Opens a new incognito window in this process, optionally with web addresses to load.
#[tauri::command]
pub async fn incognito_open(urls: Option<Vec<String>>) -> CmdResult<()> {
    let urls: Vec<String> = urls
        .unwrap_or_default()
        .iter()
        .filter_map(|u| urls::sanitize_navigation(u))
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        .collect();
    super::open_window(urls);
    Ok(())
}
