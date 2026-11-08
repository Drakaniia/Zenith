//! Commands for the history page and the address bar's history matches.

use super::HistoryEntry;
use crate::app::command::CmdResult;
use crate::host::db;

#[tauri::command]
pub async fn history_search(text: String, limit: Option<i64>, offset: Option<i64>) -> CmdResult<Vec<HistoryEntry>> {
    let (limit, offset) = (limit.unwrap_or(50).clamp(1, 500), offset.unwrap_or(0).max(0));
    db().call(move |c| super::history_search(c, &text, limit, offset)).await
}

#[tauri::command]
pub async fn history_top(limit: Option<i64>) -> CmdResult<Vec<HistoryEntry>> {
    let limit = limit.unwrap_or(40).clamp(1, 200);
    db().call(move |c| super::history_top(c, limit)).await
}

#[tauri::command]
pub async fn history_delete(ids: Vec<i64>) -> CmdResult<usize> {
    db().call(move |c| super::history_delete(c, &ids)).await
}

#[tauri::command]
pub async fn history_clear(since_ms: Option<i64>) -> CmdResult<usize> {
    db().call(move |c| super::history_clear(c, since_ms)).await
}

#[tauri::command]
pub async fn history_prune(days: i64) -> CmdResult<()> {
    db().call(move |c| super::history_prune(c, days, 100_000)).await
}
