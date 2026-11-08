//! The search-suggestions command.

use crate::app::command::{blocking, CmdResult};

#[tauri::command]
pub async fn search_suggest(engine: String, query: String) -> CmdResult<Vec<String>> {
    let query = query.trim().to_string();
    if query.is_empty() || query.len() > 200 {
        return Ok(Vec::new());
    }
    blocking(move || super::suggestions(&engine, &query)).await?
}
