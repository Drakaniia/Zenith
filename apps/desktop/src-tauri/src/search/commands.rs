//! The search-suggestions command.

use crate::app::command::{blocking, CmdResult};

pub async fn search_suggest(engine: String, query: String) -> CmdResult<Vec<String>> {
    let query = query.trim().to_string();
    if query.is_empty() || query.len() > 200 {
        return Ok(Vec::new());
    }
    blocking(move || super::suggestions(&engine, &query)).await?
}

/// Fetches an instant-answer API (exchange rates, weather) for the address bar; returns its JSON text.
pub async fn search_instant_fetch(url: String) -> CmdResult<String> {
    blocking(move || super::instant_fetch(&url)).await?
}
