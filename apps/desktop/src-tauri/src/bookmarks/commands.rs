//! Commands for bookmarks. The tree itself is in `bookmarks`. Every window can change it,
//! incognito ones included, as in other browsers; the other windows hear about a change through
//! `bookmarks-changed` and reload.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::command::{blocking, private, CmdResult};
use crate::bookmarks;
use crate::host::{self, bookmarks_db, paths};
use crate::platform::net;
use crate::storage;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BookmarksChanged {
    /// Window that made the change; it already has the new state.
    origin: String,
}

pub(crate) fn bookmarks_changed(window: &WebviewWindow) {
    host::emit("bookmarks-changed", BookmarksChanged { origin: window.label().to_string() });
}

#[tauri::command]
pub async fn bookmarks_list() -> CmdResult<Vec<bookmarks::Node>> {
    bookmarks_db().call(|c| bookmarks::list(c)).await
}

#[tauri::command]
pub async fn bookmarks_add(window: WebviewWindow, parent: i64, index: Option<i64>, nodes: Vec<bookmarks::NewNode>) -> CmdResult<Vec<i64>> {
    let ids = bookmarks_db().call(move |c| bookmarks::add(c, parent, index, &nodes)).await?;
    bookmarks_changed(&window);
    Ok(ids)
}

#[tauri::command]
pub async fn bookmark_update(window: WebviewWindow, id: i64, patch: bookmarks::NodePatch) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::update(c, id, &patch)).await?;
    bookmarks_changed(&window);
    Ok(())
}

#[tauri::command]
pub async fn bookmark_move(window: WebviewWindow, id: i64, parent: i64, index: i64) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::move_node(c, id, parent, index)).await?;
    bookmarks_changed(&window);
    Ok(())
}

#[tauri::command]
pub async fn bookmarks_reorder(window: WebviewWindow, parent: i64, order: Vec<i64>) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::reorder(c, parent, &order)).await?;
    bookmarks_changed(&window);
    Ok(())
}

#[tauri::command]
pub async fn bookmarks_remove(window: WebviewWindow, ids: Vec<i64>) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::remove(c, &ids)).await?;
    bookmarks_changed(&window);
    Ok(())
}

#[tauri::command]
pub async fn bookmarks_remove_url(window: WebviewWindow, url: String) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::remove_url(c, &url)).await?;
    bookmarks_changed(&window);
    Ok(())
}

/// Largest bookmarks file read (an export or a backup is far smaller).
const MAX_BOOKMARK_FILE: u64 = 64 * 1024 * 1024;

fn read_bookmark_file(path: PathBuf) -> Result<String, String> {
    let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if len > MAX_BOOKMARK_FILE {
        return Err("That file is too large to be a bookmarks file.".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    // Browsers write UTF-8; anything else is read as well as it can be.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Where automatic backups and the copies made before a restore are kept.
fn bookmark_backups_dir() -> PathBuf {
    paths().roaming.join("bookmarkbackups")
}

/// Saves the bookmarks as a Netscape HTML file that Firefox, Chrome and Edge can import.
#[tauri::command]
pub async fn bookmarks_export(app: AppHandle) -> CmdResult<Option<String>> {
    let html = bookmarks_db().call(|c| bookmarks::export_html(c)).await?;
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Export bookmarks")
            .set_file_name("zenith-bookmarks.html")
            .add_filter("Bookmarks (HTML)", &["html", "htm"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        storage::write_atomic(&path, html.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await?
}

/// Adds the bookmarks of a Netscape HTML file; returns how many are new.
#[tauri::command]
pub async fn bookmarks_import(window: WebviewWindow, app: AppHandle) -> CmdResult<usize> {
    let html = blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Import bookmarks")
            .add_filter("Bookmarks (HTML)", &["html", "htm"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        read_bookmark_file(path.into_path().map_err(|e| e.to_string())?).map(Some)
    })
    .await??;
    let Some(html) = html else { return Ok(0) };
    let added = bookmarks_db().call(move |c| bookmarks::import_html(c, &html)).await?;
    if added > 0 {
        bookmarks_changed(&window);
    }
    Ok(added)
}

/// Saves every bookmark, folder, tag and keyword as a JSON backup file.
#[tauri::command]
pub async fn bookmarks_backup(app: AppHandle) -> CmdResult<Option<String>> {
    let backup = bookmarks_db().call(|c| bookmarks::backup_json(c)).await?;
    let name = format!("zenith-bookmarks-{}.json", bookmarks::date_string(storage::now_ms()));
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Back up bookmarks")
            .set_file_name(name)
            .add_filter("Bookmarks backup (JSON)", &["json"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        storage::write_atomic(&path, backup.json.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await?
}

/// Replaces every bookmark with the ones in a backup file; None when the dialog is cancelled.
/// A copy of the current bookmarks is kept first.
#[tauri::command]
pub async fn bookmarks_restore(window: WebviewWindow, app: AppHandle) -> CmdResult<Option<usize>> {
    if private(&window) {
        return Err("Incognito windows can't restore bookmarks.".into());
    }
    let json = blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Restore bookmarks")
            .add_filter("Bookmarks backup (JSON)", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        read_bookmark_file(path.into_path().map_err(|e| e.to_string())?).map(Some)
    })
    .await??;
    let Some(json) = json else { return Ok(None) };
    let dir = bookmark_backups_dir();
    let restored = bookmarks_db().call(move |c| bookmarks::restore(c, Some(&dir), &json, storage::now_ms())).await?;
    bookmarks_changed(&window);
    Ok(Some(restored))
}

/// The automatic backups and pre-restore copies, newest first.
#[tauri::command]
pub async fn bookmark_backups() -> CmdResult<Vec<bookmarks::BackupInfo>> {
    let dir = bookmark_backups_dir();
    blocking(move || bookmarks::list_backups(&dir)).await
}

/// Restores one of those backups by name (never by path).
#[tauri::command]
pub async fn bookmarks_restore_backup(window: WebviewWindow, name: String) -> CmdResult<usize> {
    if private(&window) {
        return Err("Incognito windows can't restore bookmarks.".into());
    }
    let dir = bookmark_backups_dir();
    let json = {
        let dir = dir.clone();
        blocking(move || bookmarks::read_backup(&dir, &name)).await??
    };
    let restored = bookmarks_db().call(move |c| bookmarks::restore(c, Some(&dir), &json, storage::now_ms())).await?;
    bookmarks_changed(&window);
    Ok(restored)
}

/// The icon of a bookmarked site, as a data URL: its `/favicon.ico`, fetched once for bookmarks that
/// came from an import (or were made before the site was visited). Private network addresses
/// are never asked. None when the site has none.
#[tauri::command]
pub async fn favicon_fetch(url: String) -> CmdResult<Option<String>> {
    let Some(icon) = bookmarks::favicon_url(&url) else { return Ok(None) };
    // A redirect could lead to a private address that the check above never saw: none are followed.
    blocking(move || {
        net::get_no_redirects(&icon, bookmarks::FAVICON_MAX_BYTES, 5_000).ok().and_then(|b| bookmarks::favicon_data_url(&b))
    })
    .await
}

/// Writes today's automatic backup unless there is one already (the main window calls this once
/// at startup).
#[tauri::command]
pub async fn bookmarks_auto_backup(window: WebviewWindow) -> CmdResult<()> {
    if private(&window) {
        return Ok(());
    }
    let dir = bookmark_backups_dir();
    bookmarks_db().call(move |c| bookmarks::auto_backup(c, &dir, storage::now_ms())).await
}
