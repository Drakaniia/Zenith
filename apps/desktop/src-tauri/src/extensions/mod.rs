//! Browser extensions for CEF Chrome-style profiles.
//!
//! Extensions are unpacked into `%LOCALAPPDATA%\<id>\extensions\` and tracked in `registry.json`
//! (preserving WebView2-era registry preferences). Enabled extensions are passed to CEF at
//! startup via `--load-extension`; installs, removals and toggles also apply to the running
//! engine through the DevTools `Extensions` domain.

pub mod bridge;
pub mod commands;
pub mod crx;
mod install;
pub mod native_messaging;
pub mod private_profile;
mod updates;

pub use install::{accept, discard, download_from_store, install_folder, offer, refuse, stage_crx, stage_folder, Fetch};
pub use updates::{check_updates, store_source, ExtensionUpdate};

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::host::paths;
use crate::storage::{read_text, write_atomic};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub version: String,
    pub description: String,
    pub options_url: Option<String>,
    pub popup_url: Option<String>,
    pub icon: Option<String>,
    pub builtin: bool,
    pub store_id: Option<String>,
    pub incognito: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct Registry {
    pub folders: HashMap<String, PathBuf>,
    #[serde(default)]
    pub sources: HashMap<String, String>,
    #[serde(default)]
    pub incognito_off: HashSet<String>,
    #[serde(default)]
    pub disabled: HashSet<String>,
}

fn registry_path() -> PathBuf {
    paths().extensions().join("registry.json")
}

pub(crate) fn load_registry() -> Registry {
    read_text(&registry_path())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_registry(r: &Registry) {
    if let Ok(json) = serde_json::to_vec_pretty(r) {
        let _ = write_atomic(&registry_path(), &json);
    }
}

/// Returns unpacked directories of all enabled user extensions for CEF `--load-extension`.
pub fn enabled_extension_dirs() -> Vec<PathBuf> {
    let reg = load_registry();
    let mut dirs: Vec<PathBuf> = reg
        .folders
        .iter()
        .filter(|(id, dir)| !reg.disabled.contains(*id) && dir.join("manifest.json").is_file())
        .map(|(_, dir)| dir.clone())
        .collect();
    dirs.sort();
    dirs
}

pub(crate) struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub options: Option<String>,
    pub popup: Option<String>,
    pub icon: Option<String>,
}

fn localized(dir: &Path, manifest: &serde_json::Value, raw: &str) -> String {
    let Some(key) = raw.strip_prefix("__MSG_").and_then(|k| k.strip_suffix("__")) else {
        return raw.to_string();
    };
    let locale = manifest.get("default_locale").and_then(|v| v.as_str()).unwrap_or("en");
    for loc in [locale, "en", "en_US"] {
        let file = dir.join("_locales").join(loc).join("messages.json");
        if let Some(text) = read_text(&file) {
            if let Ok(messages) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text) {
                let hit = messages
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .and_then(|(_, v)| v.get("message"))
                    .and_then(|m| m.as_str());
                if let Some(m) = hit {
                    return m.to_string();
                }
            }
        }
    }
    raw.to_string()
}

pub(crate) fn read_manifest(dir: &Path) -> Option<Manifest> {
    let text = read_text(&dir.join("manifest.json"))?;
    let text = text.trim_start_matches('\u{feff}');
    let m: serde_json::Value = serde_json::from_str(text).ok()?;
    let s = |v: Option<&serde_json::Value>| v.and_then(|x| x.as_str()).map(str::to_string);
    let name = s(m.get("name")).map(|n| localized(dir, &m, &n)).unwrap_or_else(|| "Extension".into());
    let description = s(m.get("description")).map(|d| localized(dir, &m, &d)).unwrap_or_default();
    let options = s(m.pointer("/options_ui/page")).or_else(|| s(m.get("options_page")));
    let popup = s(m.pointer("/action/default_popup")).or_else(|| s(m.pointer("/browser_action/default_popup")));
    let icon = m.get("icons").and_then(|icons| icons.as_object()).and_then(|icons| {
        let mut sizes: Vec<(u32, String)> = icons
            .iter()
            .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_string())))
            .collect();
        sizes.sort_by_key(|(size, _)| (*size as i64 - 48).abs());
        let (_, rel) = sizes.into_iter().next()?;
        let bytes = fs::read(dir.join(rel.trim_start_matches('/'))).ok()?;
        if bytes.len() > 256 * 1024 {
            return None;
        }
        Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    });
    Some(Manifest {
        name,
        version: s(m.get("version")).unwrap_or_default(),
        description,
        options,
        popup,
        icon,
    })
}

pub(crate) fn info_for(id: String, name: String, enabled: bool, registry: &Registry) -> ExtensionInfo {
    let manifest = registry.folders.get(&id).and_then(|dir| read_manifest(dir));
    let page = |p: &Option<String>| {
        p.as_ref()
            .map(|p| format!("chrome-extension://{id}/{}", p.trim_start_matches('/')))
    };
    ExtensionInfo {
        name: if name.is_empty() {
            manifest.as_ref().map(|m| m.name.clone()).unwrap_or_default()
        } else {
            name
        },
        enabled,
        version: manifest.as_ref().map(|m| m.version.clone()).unwrap_or_default(),
        description: manifest.as_ref().map(|m| m.description.clone()).unwrap_or_default(),
        options_url: manifest.as_ref().and_then(|m| page(&m.options)),
        popup_url: manifest.as_ref().and_then(|m| page(&m.popup)),
        icon: manifest.and_then(|m| m.icon),
        builtin: !registry.folders.contains_key(&id),
        store_id: registry.sources.get(&id).cloned(),
        incognito: !registry.incognito_off.contains(&id),
        id,
    }
}

pub fn list(done: impl FnOnce(Result<Vec<ExtensionInfo>, String>) + 'static) {
    let registry = load_registry();
    let mut out: Vec<ExtensionInfo> = registry
        .folders
        .keys()
        .map(|id| {
            let enabled = !registry.disabled.contains(id);
            info_for(id.clone(), String::new(), enabled, &registry)
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    done(Ok(out));
}

/// Loads an unpacked folder into the running engine, then calls `done` either way: a failure
/// only means the extension starts with the next launch.
fn load_live(dir: &Path, done: impl FnOnce() + 'static) {
    let params = serde_json::json!({ "path": dir }).to_string();
    crate::host::browser_cdp("Extensions.loadUnpacked", &params, move |r| {
        if let Err(e) = r {
            eprintln!("[zenith] live extension load failed: {e}");
        }
        done();
    });
}

/// Unloads an extension from the running engine, then calls `done` either way.
fn unload_live(id: &str, done: impl FnOnce() + 'static) {
    let params = serde_json::json!({ "id": id }).to_string();
    crate::host::browser_cdp("Extensions.uninstall", &params, move |_| done());
}

fn ensure_user_extension(id: &str) -> Result<(), String> {
    if load_registry().folders.contains_key(id) {
        Ok(())
    } else {
        Err("this extension is built in and cannot be changed".into())
    }
}

pub fn remove(id: String, done: impl FnOnce(Result<(), String>) + 'static) {
    if let Err(e) = ensure_user_extension(&id) {
        return done(Err(e));
    }
    let mut registry = load_registry();
    let dir = registry.folders.remove(&id);
    registry.sources.remove(&id);
    registry.disabled.remove(&id);
    registry.incognito_off.remove(&id);
    save_registry(&registry);
    // Unload first: the engine still reads the folder while the extension runs.
    unload_live(&id, move || {
        if let Some(dir) = dir {
            let _ = fs::remove_dir_all(dir);
        }
        done(Ok(()));
    });
}

pub fn set_incognito_allowed(id: &str, allowed: bool) -> Result<(), String> {
    ensure_user_extension(id)?;
    let mut registry = load_registry();
    if allowed {
        registry.incognito_off.remove(id);
    } else {
        registry.incognito_off.insert(id.to_string());
    }
    save_registry(&registry);
    Ok(())
}

pub fn set_enabled(id: String, enabled: bool, done: impl FnOnce(Result<(), String>) + 'static) {
    if let Err(e) = ensure_user_extension(&id) {
        return done(Err(e));
    }
    let mut registry = load_registry();
    let dir = registry.folders.get(&id).cloned();
    if enabled {
        registry.disabled.remove(&id);
    } else {
        registry.disabled.insert(id.clone());
    }
    save_registry(&registry);
    match dir {
        Some(dir) if enabled => load_live(&dir, move || done(Ok(()))),
        _ if !enabled => unload_live(&id, move || done(Ok(()))),
        _ => done(Ok(())),
    }
}
