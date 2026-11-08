//! Commands for importing bookmarks and history from other browsers.

use serde::Serialize;
use tauri::WebviewWindow;

use crate::app::command::{blocking, private, CmdResult};
use crate::bookmarks::commands::bookmarks_changed;
use crate::host::{bookmarks_db, db};
use crate::{bookmarks, importer};

/// Browser profiles on this PC with bookmarks or history to import.
#[tauri::command]
pub async fn import_sources() -> CmdResult<Vec<importer::ImportSource>> {
    blocking(importer::detect).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    /// New bookmarks (ones already present are skipped).
    bookmarks: usize,
    /// History entries merged.
    history: usize,
    errors: Vec<String>,
}

/// Imports bookmarks and/or history from a profile found by `import_sources`. History older
/// than `history_days` (0: any age) is left out, since retention would drop it anyway.
#[tauri::command]
pub async fn import_run(
    window: WebviewWindow,
    source: String,
    bookmarks: bool,
    history: bool,
    history_days: i64,
) -> CmdResult<ImportResult> {
    if private(&window) {
        return Err("Incognito windows can't import data.".into());
    }
    let data = blocking(move || importer::read(&source, bookmarks, history, history_days)).await??;
    let marks = data.bookmarks;
    let added = if marks.is_empty() { 0 } else { bookmarks_db().call(move |c| bookmarks::import_paths(c, &marks)).await? };
    if added > 0 {
        bookmarks_changed(&window);
    }
    let visits: Vec<_> = data.history.into_iter().map(|v| (v.url, v.title, v.visit_count, v.last_visit)).collect();
    let merged = if visits.is_empty() { 0 } else { db().call(move |c| crate::history::history_import(c, &visits)).await? };
    Ok(ImportResult { bookmarks: added, history: merged, errors: data.errors })
}
