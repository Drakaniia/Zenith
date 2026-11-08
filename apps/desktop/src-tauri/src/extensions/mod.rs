//! Browser extensions through WebView2's (limited) Chromium extension support.
//!
//! Unpacked extensions are copied into Zenith's own data folder before being added, because
//! WebView2 removes an extension when its source folder changes. `.crx` packages and Chrome Web
//! Store links are unpacked with Windows' built-in `tar.exe` (bsdtar reads zip), so no archive
//! library ships in the binary. Which extension APIs work is decided by WebView2.
//!
//! Packages keep their Chrome id: the developer key from the `.crx` header is written into the
//! unpacked manifest (`key`), exactly as Chrome derives the id, so extensions that rely on their
//! own id keep working and the Web Store can be asked for updates.
//!
//! Extensions found in a page (a `.crx` download, the Web Store's "Add to Chrome" button) are
//! not installed straight away: they are staged under `extensions/pending` and offered to the
//! user with their permissions (`ext-install` event), then installed or discarded.

pub mod commands;
pub mod crx;
mod install;
mod updates;

pub use install::{accept, discard, download_from_store, install_folder, offer, refuse, stage_crx, stage_folder, Fetch};
pub use updates::{check_updates, store_source, ExtensionUpdate};

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::result::Result;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, PWSTR};

use crate::host::{paths, with_host};
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
    /// Shipped inside WebView2 (e.g. the PDF viewer): listed, but never changed by Zenith.
    pub builtin: bool,
    /// Chrome Web Store id, for extensions that came from the store (updates, store link).
    pub store_id: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct Registry {
    /// extension id -> unpacked folder
    folders: HashMap<String, PathBuf>,
    /// extension id -> Chrome Web Store id
    #[serde(default)]
    sources: HashMap<String, String>,
}

fn registry_path() -> PathBuf {
    paths().extensions().join("registry.json")
}

fn load_registry() -> Registry {
    read_text(&registry_path())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_registry(r: &Registry) {
    if let Ok(json) = serde_json::to_vec_pretty(r) {
        let _ = write_atomic(&registry_path(), &json);
    }
}

fn profile() -> Result<ICoreWebView2Profile7, String> {
    with_host(|h| {
        let core = h.wins.values().next()?.chrome_core.clone();
        Some(unsafe {
            core.cast::<ICoreWebView2_13>()
                .and_then(|c| c.Profile())
                .and_then(|p| p.cast::<ICoreWebView2Profile7>())
        })
    })
    .flatten()
    .ok_or("browser is not ready")?
    .map_err(|e| format!("extensions are not supported by this WebView2 runtime ({e})"))
}

struct Manifest {
    name: String,
    version: String,
    description: String,
    options: Option<String>,
    popup: Option<String>,
    icon: Option<String>,
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

fn read_manifest(dir: &Path) -> Option<Manifest> {
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

fn info_for(id: String, name: String, enabled: bool, registry: &Registry) -> ExtensionInfo {
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
        id,
    }
}

fn ext_fields(ext: &ICoreWebView2BrowserExtension) -> (String, String, bool) {
    unsafe {
        let mut id = PWSTR::null();
        let _ = ext.Id(&mut id);
        let mut name = PWSTR::null();
        let _ = ext.Name(&mut name);
        let mut enabled = BOOL::default();
        let _ = ext.IsEnabled(&mut enabled);
        (take_pwstr(id), take_pwstr(name), enabled.as_bool())
    }
}

fn with_extensions(done: impl FnOnce(Result<Vec<ICoreWebView2BrowserExtension>, String>) + 'static) {
    let profile = match profile() {
        Ok(p) => p,
        Err(e) => return done(Err(e)),
    };
    let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |result, list| {
        let mut out = Vec::new();
        if let (Ok(()), Some(list)) = (result.as_ref(), list) {
            let mut count = 0u32;
            unsafe {
                let _ = list.Count(&mut count);
                for i in 0..count {
                    if let Ok(ext) = list.GetValueAtIndex(i) {
                        out.push(ext);
                    }
                }
            }
            done(Ok(out));
        } else {
            done(Err(result.err().map(|e| e.to_string()).unwrap_or_else(|| "no extension list".into())));
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) = profile.GetBrowserExtensions(&handler) {
            eprintln!("[zenith] GetBrowserExtensions failed: {e}");
        }
    }
}

pub fn list(done: impl FnOnce(Result<Vec<ExtensionInfo>, String>) + 'static) {
    with_extensions(move |r| {
        done(r.map(|exts| {
            let registry = load_registry();
            exts.iter()
                .map(|e| {
                    let (id, name, enabled) = ext_fields(e);
                    info_for(id, name, enabled, &registry)
                })
                .collect()
        }))
    });
}

fn ensure_user_extension(id: &str) -> Result<(), String> {
    if load_registry().folders.contains_key(id) {
        Ok(())
    } else {
        Err("this extension is part of WebView2 and cannot be changed".into())
    }
}

pub fn remove(id: String, done: impl FnOnce(Result<(), String>) + 'static) {
    if let Err(e) = ensure_user_extension(&id) {
        return done(Err(e));
    }
    with_extensions(move |r| match r {
        Err(e) => done(Err(e)),
        Ok(exts) => {
            let Some(ext) = exts.into_iter().find(|e| ext_fields(e).0 == id) else {
                return done(Err("extension not found".into()));
            };
            let handler = BrowserExtensionRemoveCompletedHandler::create(Box::new(move |result| {
                let mut registry = load_registry();
                if let Some(dir) = registry.folders.remove(&id) {
                    let _ = fs::remove_dir_all(dir);
                }
                registry.sources.remove(&id);
                save_registry(&registry);
                done(result.map_err(|e| e.to_string()));
                Ok(())
            }));
            unsafe {
                let _ = ext.Remove(&handler);
            }
        }
    });
}

pub fn set_enabled(id: String, enabled: bool, done: impl FnOnce(Result<(), String>) + 'static) {
    if let Err(e) = ensure_user_extension(&id) {
        return done(Err(e));
    }
    with_extensions(move |r| match r {
        Err(e) => done(Err(e)),
        Ok(exts) => {
            let Some(ext) = exts.into_iter().find(|e| ext_fields(e).0 == id) else {
                return done(Err("extension not found".into()));
            };
            let handler = BrowserExtensionEnableCompletedHandler::create(Box::new(move |result| {
                done(result.map_err(|e| e.to_string()));
                Ok(())
            }));
            unsafe {
                let _ = ext.Enable(enabled, &handler);
            }
        }
    });
}
