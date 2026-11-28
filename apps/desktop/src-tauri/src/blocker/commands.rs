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

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CachedListMeta {
    id: String,
    len: u64,
    updated: Option<i64>,
    rules: usize,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct EngineCacheMeta {
    builtin_len: usize,
    total_rules: usize,
    lists: Vec<CachedListMeta>,
}

fn list_file(id: &str) -> PathBuf {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    // Filter lists are cached in the user's profile, incognito windows included.
    host::persistent_paths().filters().join(format!("{safe}.txt"))
}

fn cache_paths() -> (PathBuf, PathBuf) {
    let dir = host::persistent_paths().filters();
    (dir.join("engine-v1.bin"), dir.join("engine-v1.json"))
}

fn file_stamp(path: &std::path::Path) -> Option<(u64, Option<i64>)> {
    let meta = std::fs::metadata(path).ok()?;
    let updated = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);
    Some((meta.len(), updated))
}

/// Rebuilds the engine from the built-in list plus the given subscriptions. With `update`, lists
/// are downloaded again; otherwise cached copies are used (downloaded when missing) and a matching
/// binary snapshot (`engine-v1.bin`) skips re-parsing 100k+ ABP text lines on startup.
pub async fn blocker_configure(lists: Vec<FilterList>, update: bool, disabled_sites: Vec<String>) -> CmdResult<BlockerStats> {
    blocking(move || {
        let (bin_path, meta_path) = cache_paths();
        if !update {
            if let Some(cached) = storage::read_text(&meta_path)
                .and_then(|s| serde_json::from_str::<EngineCacheMeta>(&s).ok())
                .filter(|m| m.builtin_len == blocker::BUILTIN_LIST.len() && m.lists.len() == lists.len())
            {
                let stamps_match = lists.iter().zip(&cached.lists).all(|(l, c)| {
                    l.id == c.id
                        && file_stamp(&list_file(&l.id))
                            .is_some_and(|(len, updated)| len > 0 && len == c.len && updated == c.updated)
                });
                if stamps_match {
                    if let Some(engine) = std::fs::read(&bin_path)
                        .ok()
                        .and_then(|b| blocker::Engine::from_serialized(&b, cached.total_rules))
                    {
                        let rules = engine.rule_count();
                        if let Ok(mut state) = BLOCKER.write() {
                            state.engine = engine;
                            state.disabled_sites = disabled_sites.into_iter().map(|s| s.to_ascii_lowercase()).collect();
                        }
                        let statuses = cached
                            .lists
                            .into_iter()
                            .map(|c| ListStatus { id: c.id, rules: c.rules, updated: c.updated, error: None })
                            .collect();
                        return BlockerStats {
                            rules,
                            total_blocked: tab::TOTAL_BLOCKED.load(std::sync::atomic::Ordering::Relaxed),
                            lists: statuses,
                        };
                    }
                }
            }
        }

        let mut builder = blocker::Builder::with_builtin();
        let mut statuses = Vec::new();
        let mut meta_lists = Vec::with_capacity(lists.len());
        let mut all_clean = true;
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
            let rules = if text.is_empty() { 0 } else { builder.add_list(&text) };
            let stamp = file_stamp(&file);
            let updated = stamp.and_then(|(_, u)| u);
            if let (true, Some((len, _))) = (error.is_none(), stamp.filter(|(l, _)| *l > 0)) {
                meta_lists.push(CachedListMeta { id: list.id.clone(), len, updated, rules });
            } else {
                all_clean = false;
            }
            statuses.push(ListStatus { id: list.id.clone(), rules, updated, error });
        }
        let engine = builder.build();
        let rules = engine.rule_count();
        if all_clean && meta_lists.len() == lists.len() {
            let meta = EngineCacheMeta {
                builtin_len: blocker::BUILTIN_LIST.len(),
                total_rules: rules,
                lists: meta_lists,
            };
            if let Ok(meta_json) = serde_json::to_vec(&meta) {
                let bytes = engine.serialize();
                if storage::write_atomic(&bin_path, &bytes).is_ok() {
                    let _ = storage::write_atomic(&meta_path, &meta_json);
                }
            }
        }
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

pub async fn blocker_set_disabled_sites(sites: Vec<String>) -> CmdResult<()> {
    if let Ok(mut state) = BLOCKER.write() {
        state.disabled_sites = sites.into_iter().map(|s| s.to_ascii_lowercase()).collect();
    }
    Ok(())
}

pub async fn blocker_stats() -> CmdResult<BlockerStats> {
    let rules = BLOCKER.read().map(|b| b.engine.rule_count()).unwrap_or(0);
    Ok(BlockerStats {
        rules,
        total_blocked: tab::TOTAL_BLOCKED.load(std::sync::atomic::Ordering::Relaxed),
        lists: Vec::new(),
    })
}
