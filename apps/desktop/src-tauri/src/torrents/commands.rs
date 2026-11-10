//! Commands for torrents: adding one (magnet link, .torrent file or its bytes), choosing its
//! files and playing a file while it downloads. Pause, resume, cancel and stop go through the
//! downloads page's `download_action`.

use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use super::{Input, TorrentFile};
// Torrents added from an incognito window's UI stay out of the download history.
use crate::app::command::{blocking, private, CmdResult};

fn ensure_enabled() -> CmdResult<()> {
    if super::enabled() {
        Ok(())
    } else {
        Err("Torrent downloads are turned off in Settings > Downloads.".into())
    }
}

/// Adds a magnet link pasted on the downloads page.
#[tauri::command]
pub async fn torrent_add_magnet(window: WebviewWindow, uri: String) -> CmdResult<i64> {
    ensure_enabled()?;
    let uri = uri.trim().to_string();
    if !super::is_magnet(&uri) {
        return Err("Paste a link that starts with magnet:".into());
    }
    let private = private(&window);
    blocking(move || super::add(Input::Magnet(uri), "", private, None)).await?
}

/// Adds a .torrent file dropped on the downloads page (its contents).
#[tauri::command]
pub async fn torrent_add_bytes(window: WebviewWindow, bytes: Vec<u8>) -> CmdResult<i64> {
    ensure_enabled()?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("That file is too large to be a .torrent file.".into());
    }
    let private = private(&window);
    blocking(move || super::add(Input::File(bytes), "", private, None)).await?
}

/// Asks for a .torrent file and adds it. None when the user canceled.
#[tauri::command]
pub async fn torrent_open_file(app: AppHandle, window: WebviewWindow) -> CmdResult<Option<i64>> {
    ensure_enabled()?;
    let private = private(&window);
    blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("Open a torrent")
            .add_filter("Torrent files", &["torrent"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok());
        match path {
            Some(p) => super::add_file(&p.display().to_string(), "", private).map(Some),
            None => Ok(None),
        }
    })
    .await?
}

#[tauri::command]
pub async fn torrent_files(id: i64) -> CmdResult<Vec<TorrentFile>> {
    super::files(id)
}

/// Downloads only the chosen files (by index).
#[tauri::command]
pub async fn torrent_choose_files(id: i64, files: Vec<usize>) -> CmdResult<()> {
    super::choose_files(id, files).await
}

/// A local address that plays a file of the torrent while it downloads.
#[tauri::command]
pub async fn torrent_stream_url(id: i64, file: usize) -> CmdResult<String> {
    super::stream_url(id, file).await
}
