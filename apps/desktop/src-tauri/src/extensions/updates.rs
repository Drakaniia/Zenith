//! Update checks for extensions installed from the Chrome Web Store.

use serde::Serialize;
use std::result::Result;

use super::{crx, load_registry, read_manifest};

// ---------------------------------------------------------------------------------------------
// Updates from the Chrome Web Store

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionUpdate {
    pub id: String,
    pub store_id: String,
    pub name: String,
    pub current: String,
    pub latest: String,
}

/// Asks the Web Store for newer versions of the store extensions (blocking).
pub fn check_updates() -> Result<Vec<ExtensionUpdate>, String> {
    let registry = load_registry();
    let mut installed = Vec::new();
    for (id, store_id) in &registry.sources {
        let Some(m) = registry.folders.get(id).and_then(|d| read_manifest(d)) else { continue };
        installed.push((id.clone(), store_id.clone(), m.name, m.version));
    }
    if installed.is_empty() {
        return Ok(Vec::new());
    }
    let query: Vec<(String, String)> = installed.iter().map(|(_, s, _, v)| (s.clone(), v.clone())).collect();
    let body = crate::platform::net::get(&crx::update_check_url(&query, &crx::prodversion()), 1024 * 1024, 20_000)
        .map_err(|e| format!("could not reach the Chrome Web Store ({e})"))?;
    let answers = crx::parse_update_response(&String::from_utf8_lossy(&body));
    Ok(installed
        .into_iter()
        .filter_map(|(id, store_id, name, current)| {
            let a = answers.iter().find(|a| a.id == store_id && a.status == "ok")?;
            let latest = a.version.clone()?;
            crx::is_newer(&latest, &current).then_some(ExtensionUpdate { id, store_id, name, current, latest })
        })
        .collect())
}

/// The store id an installed extension came from.
pub fn store_source(id: &str) -> Option<String> {
    load_registry().sources.get(id).cloned()
}
