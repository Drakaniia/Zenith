//! Commands for ad and tracker blocking: building the engine from the filter lists the user
//! subscribed to, the sites where blocking is off, and statistics.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::command::{blocking, CmdResult};
use crate::blocker;
use crate::host::{self, tab, BLOCKER};
use crate::platform::net;
use crate::storage;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FilterList {
    id: String,
    url: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ListStatus {
    id: String,
    rules: usize,
    updated: Option<i64>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockerStats {
    rules: usize,
    total_blocked: u64,
    lists: Vec<ListStatus>,
}

fn list_file(id: &str) -> PathBuf {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    // Filter lists are cached in the user's profile, incognito windows included.
    host::persistent_paths().filters().join(format!("{safe}.txt"))
}

/// Rebuilds the engine from the built-in list plus the given subscriptions. With `update`, lists
/// are downloaded again; otherwise cached copies are used (downloaded when missing).
#[tauri::command]
pub async fn blocker_configure(lists: Vec<FilterList>, update: bool, disabled_sites: Vec<String>) -> CmdResult<BlockerStats> {
    blocking(move || {
        let mut engine = blocker::Engine::with_builtin();
        let mut statuses = Vec::new();
        for list in &lists {
            let file = list_file(&list.id);
            let mut error = None;
            if update || !file.exists() {
                match net::get(&list.url, 16 * 1024 * 1024, 20_000) {
                    Ok(body) => {
                        let _ = std::fs::create_dir_all(host::persistent_paths().filters());
                        if let Err(e) = storage::write_atomic(&file, &body) {
                            error = Some(e.to_string());
                        }
                    }
                    Err(e) => error = Some(e),
                }
            }
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let rules = if text.is_empty() { 0 } else { engine.add_list(&text) };
            let updated = std::fs::metadata(&file)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64);
            statuses.push(ListStatus { id: list.id.clone(), rules, updated, error });
        }
        let rules = engine.rule_count();
        if let Ok(mut state) = BLOCKER.write() {
            state.engine = engine;
            state.disabled_sites = disabled_sites.into_iter().map(|s| s.to_ascii_lowercase()).collect();
        }
        BlockerStats {
            rules,
            total_blocked: tab::TOTAL_BLOCKED.load(std::sync::atomic::Ordering::Relaxed),
            lists: statuses,
        }
    })
    .await
}

#[tauri::command]
pub async fn blocker_set_disabled_sites(sites: Vec<String>) -> CmdResult<()> {
    if let Ok(mut state) = BLOCKER.write() {
        state.disabled_sites = sites.into_iter().map(|s| s.to_ascii_lowercase()).collect();
    }
    Ok(())
}

#[tauri::command]
pub async fn blocker_stats() -> CmdResult<BlockerStats> {
    let rules = BLOCKER.read().map(|b| b.engine.rule_count()).unwrap_or(0);
    Ok(BlockerStats {
        rules,
        total_blocked: tab::TOTAL_BLOCKED.load(std::sync::atomic::Ordering::Relaxed),
        lists: Vec::new(),
    })
}
