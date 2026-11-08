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

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::result::Result;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, HSTRING, PWSTR};

use super::crx;
use super::{emit_tab, paths, with_host};
use crate::store::{read_text, write_atomic};

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

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "extension".into() } else { s.chars().take(40).collect() }
}

/// Copies an unpacked extension into Zenith's data folder (prepare step; safe off the UI thread).
pub fn stage_folder(source: &Path) -> Result<PathBuf, String> {
    let manifest = read_manifest(source).ok_or("the folder has no valid manifest.json")?;
    let dest = paths()
        .extensions()
        .join(format!("{}-{}", slug(&manifest.name), crate::store::now_ms()));
    copy_dir(source, &dest).map_err(|e| format!("could not copy the extension: {e}"))?;
    Ok(dest)
}

/// An unpacked package, not yet registered with WebView2.
pub struct Staged {
    pub dir: PathBuf,
    /// The package's Chrome id (from its signing key).
    pub crx_id: Option<String>,
}

/// The id WebView2 will give an unpacked folder whose manifest has a `key`.
fn manifest_key_id(dir: &Path) -> Option<String> {
    let text = read_text(&dir.join("manifest.json"))?;
    let m: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let key = m.get("key")?.as_str()?;
    let der = base64::engine::general_purpose::STANDARD.decode(key.trim()).ok()?;
    Some(crx::id_from_key(&der))
}

/// Writes the package's developer key into the unpacked manifest so it keeps its Chrome id.
fn pin_key(dir: &Path, key: &[u8]) {
    let file = dir.join("manifest.json");
    let Some(text) = read_text(&file) else { return };
    let Ok(mut m) = serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) else { return };
    let Some(obj) = m.as_object_mut() else { return };
    if obj.contains_key("key") {
        return;
    }
    obj.insert("key".into(), serde_json::Value::String(crx::manifest_key(key)));
    if let Ok(json) = serde_json::to_vec_pretty(&m) {
        let _ = fs::write(&file, json);
    }
}

fn pending_root() -> PathBuf {
    paths().extensions().join("pending")
}

/// Unpacks a `.crx` (v2 or v3) package into Zenith's data folder.
pub fn stage_crx(bytes: &[u8]) -> Result<Staged, String> {
    stage_crx_in(bytes, &paths().extensions())
}

fn stage_crx_in(bytes: &[u8], root: &Path) -> Result<Staged, String> {
    let package = crx::parse(bytes)?;
    let zip = package.zip;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stamp = crate::store::now_ms();
    let archive = root.join(format!("pkg-{stamp}.zip"));
    fs::write(&archive, zip).map_err(|e| e.to_string())?;
    let dest = root.join(format!("crx-{stamp}"));
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let tar = Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()))
        .join("System32")
        .join("tar.exe");
    // CREATE_NO_WINDOW: tar.exe is a console program; without it a cmd window flashes up.
    use std::os::windows::process::CommandExt as _;
    let status = std::process::Command::new(tar)
        .creation_flags(0x0800_0000)
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&dest)
        .status()
        .map_err(|e| format!("could not run tar.exe: {e}"))?;
    let _ = fs::remove_file(&archive);
    if !status.success() || !dest.join("manifest.json").exists() {
        let _ = fs::remove_dir_all(&dest);
        return Err("could not unpack the extension package".into());
    }
    // Chrome's verified-contents metadata is irrelevant for unpacked loading.
    let _ = fs::remove_dir_all(dest.join("_metadata"));
    if let Some(key) = &package.public_key {
        pin_key(&dest, key);
    }
    let crx_id = package.id;
    if let Some(m) = read_manifest(&dest) {
        let named = root.join(format!("{}-{stamp}", slug(&m.name)));
        if fs::rename(&dest, &named).is_ok() {
            return Ok(Staged { dir: named, crx_id });
        }
    }
    Ok(Staged { dir: dest, crx_id })
}

/// Downloads an extension from the Chrome Web Store (blocking).
pub fn download_from_store(store_id: &str) -> Result<Vec<u8>, String> {
    crate::platform::net::get(&crx::download_url(store_id, &crx::prodversion()), MAX_PACKAGE, 60_000)
        .map_err(|e| format!("could not download the extension from the Chrome Web Store ({e})"))
}

/// Registers a staged folder with WebView2 (UI thread). Installing an extension that is already
/// installed (same id or name) replaces the earlier copy, so reinstalling is also how you
/// update it.
pub fn install_folder(dir: PathBuf, store_id: Option<String>, done: impl FnOnce(Result<ExtensionInfo, String>) + 'static) {
    let registry = load_registry();
    let by_id = manifest_key_id(&dir).filter(|id| registry.folders.contains_key(id));
    let previous = by_id.or_else(|| {
        let name = read_manifest(&dir).map(|m| m.name)?;
        registry
            .folders
            .iter()
            .find(|(_, folder)| read_manifest(folder).is_some_and(|m| m.name == name))
            .map(|(id, _)| id.clone())
    });
    match previous {
        Some(id) => remove(id, move |_| add_folder(dir, store_id, done)),
        None => add_folder(dir, store_id, done),
    }
}

fn add_folder(dir: PathBuf, store_id: Option<String>, done: impl FnOnce(Result<ExtensionInfo, String>) + 'static) {
    let profile = match profile() {
        Ok(p) => p,
        Err(e) => return done(Err(e)),
    };
    let dir2 = dir.clone();
    let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |result, ext| {
        match (result, ext) {
            (Ok(()), Some(ext)) => {
                let (id, name, enabled) = ext_fields(&ext);
                let mut registry = load_registry();
                registry.folders.insert(id.clone(), dir2.clone());
                if let Some(s) = store_id {
                    registry.sources.insert(id.clone(), s);
                }
                save_registry(&registry);
                done(Ok(info_for(id, name, enabled, &registry)));
            }
            (r, _) => {
                let _ = fs::remove_dir_all(&dir2);
                done(Err(format!(
                    "WebView2 could not load the extension: {}",
                    r.err().map(|e| e.message().to_string()).unwrap_or_default()
                )));
            }
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) = profile.AddBrowserExtension(&HSTRING::from(dir.as_os_str()), &handler) {
            eprintln!("[zenith] AddBrowserExtension failed: {e}");
        }
    }
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

// ---------------------------------------------------------------------------------------------
// Install offers: extensions found in a page wait for the user's answer.

struct Pending {
    dir: PathBuf,
    store_id: Option<String>,
}

static PENDING: Mutex<Option<HashMap<u64, Pending>>> = Mutex::new(None);
static NEXT_OFFER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstallOffer {
    pub token: u64,
    pub name: String,
    pub version: String,
    pub description: String,
    pub icon: Option<String>,
    /// Chrome-style permission warnings ("Read and change all your data on all websites").
    pub warnings: Vec<String>,
    /// The installed version when this would replace an installed extension.
    pub replaces: Option<String>,
    pub store_id: Option<String>,
}

/// Progress of an extension found in a page, sent to the tab's window.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstallEvent {
    pub tab_id: String,
    /// "fetching", "ready" (with `offer`) or "failed" (with `error`).
    pub state: &'static str,
    /// Where the extension comes from ("Chrome Web Store", a site's host name).
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offer: Option<InstallOffer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What a page asks Zenith to install.
pub enum Fetch {
    /// A Chrome Web Store id ("Add to Zenith" on a store page).
    Store(String),
    /// A `.crx` download a page started; fetched again by Zenith (WebView2 refuses to save
    /// extension packages from sites other than the store).
    Url { url: String, referer: String, user_agent: String, allow_private: bool, hint: String },
    /// A `.crx` file the page made itself (blob:/data:), read back from the page.
    Bytes { bytes: Vec<u8>, hint: String },
}

/// Upper bound for a downloaded extension package. Some store extensions (bundled ML models,
/// fonts, media) are well over 100 MB, so this only guards against runaway responses.
const MAX_PACKAGE: usize = 512 * 1024 * 1024;

fn fetch_url(url: &str, referer: String, user_agent: &str, allow_private: bool) -> Result<(Vec<u8>, String), String> {
    use crate::media::http::Client;
    let policy = Box::new(move |u: &str| {
        allow_private || !crate::platform::urls::host_of(u).is_some_and(crate::media::detect::is_private_host)
    });
    let client = Client::with_policy(user_agent, policy).map_err(|e| e.to_string())?;
    let headers = move |_: &str| vec![("Referer".to_string(), referer.clone())];
    let mut r = client.get(url, &headers, None, true).map_err(|e| format!("could not download the extension: {e}"))?;
    let bytes = r.bytes(MAX_PACKAGE).map_err(|e| format!("could not download the extension: {e}"))?;
    Ok((bytes, r.final_url.clone()))
}

/// Removes staged packages nobody answered for (left over from an earlier run).
fn sweep_pending(live: &HashMap<u64, Pending>) {
    let Ok(entries) = fs::read_dir(pending_root()) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let in_use = live.values().any(|p| p.dir == path);
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 3600);
        if !in_use && old {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn make_offer(fetch: Fetch) -> Result<InstallOffer, String> {
    let (bytes, hint_id) = match fetch {
        Fetch::Store(id) => (download_from_store(&id)?, Some(id)),
        Fetch::Url { url, referer, user_agent, allow_private, hint } => {
            let (bytes, final_url) = fetch_url(&url, referer, &user_agent, allow_private)?;
            // Store download URLs carry the id in upper case (…/EIMADPBC…_4_9_135_0.crx).
            (bytes, crx::store_id(&format!("{hint} {url} {final_url}").to_ascii_lowercase()))
        }
        Fetch::Bytes { bytes, hint } => (bytes, crx::store_id(&hint.to_ascii_lowercase())),
    };
    let staged = stage_crx_in(&bytes, &pending_root())?;
    // A package counts as a Web Store extension when the page's link or file name carries the
    // same id as its signing key (store packages and their mirrors do).
    let store_id = match (&staged.crx_id, &hint_id) {
        (Some(a), Some(b)) if a == b => Some(a.clone()),
        _ => None,
    };
    let manifest = read_manifest(&staged.dir).ok_or("the package has no valid manifest.json")?;
    let raw: serde_json::Value = read_text(&staged.dir.join("manifest.json"))
        .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default();
    let registry = load_registry();
    let replaces = staged
        .crx_id
        .as_ref()
        .and_then(|id| registry.folders.get(id))
        .and_then(|dir| read_manifest(dir))
        .map(|m| m.version);
    let token = NEXT_OFFER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let offer = InstallOffer {
        token,
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        icon: manifest.icon,
        warnings: crx::permission_warnings(&raw),
        replaces,
        store_id: store_id.clone(),
    };
    let mut guard = PENDING.lock().map_err(|_| "extension installer is busy")?;
    let map = guard.get_or_insert_with(HashMap::new);
    map.insert(token, Pending { dir: staged.dir, store_id });
    sweep_pending(map);
    Ok(offer)
}

/// Fetches and stages an extension a page offered, then asks the user (background thread).
pub fn offer(tab_id: String, source: String, fetch: Fetch) {
    let send = {
        let (tab_id, source) = (tab_id.clone(), source.clone());
        move |state, offer, error| {
            emit_tab(&tab_id, "ext-install", InstallEvent { tab_id: tab_id.clone(), state, source: source.clone(), offer, error });
        }
    };
    send("fetching", None, None);
    std::thread::spawn(move || match make_offer(fetch) {
        Ok(o) => send("ready", Some(o), None),
        Err(e) => send("failed", None, Some(e)),
    });
}

/// Tells the tab's window that an extension cannot be installed from where it was found.
pub fn refuse(tab_id: &str, source: String, error: &str) {
    emit_tab(
        tab_id,
        "ext-install",
        InstallEvent { tab_id: tab_id.to_string(), state: "failed", source, offer: None, error: Some(error.to_string()) },
    );
}

fn take_pending(token: u64) -> Option<Pending> {
    PENDING.lock().ok()?.as_mut()?.remove(&token)
}

/// The user accepted an offer (UI thread).
pub fn accept(token: u64, done: impl FnOnce(Result<ExtensionInfo, String>) + 'static) {
    let Some(p) = take_pending(token) else {
        return done(Err("this install request has expired; try again".into()));
    };
    // Out of the pending folder before WebView2 sees it: the folder must not move later.
    let Some(name) = p.dir.file_name() else { return done(Err("bad staging folder".into())) };
    let dest = paths().extensions().join(name);
    if let Err(e) = fs::rename(&p.dir, &dest) {
        let _ = fs::remove_dir_all(&p.dir);
        return done(Err(format!("could not prepare the extension: {e}")));
    }
    install_folder(dest, p.store_id, done);
}

/// The user declined an offer.
pub fn discard(token: u64) {
    if let Some(p) = take_pending(token) {
        let _ = fs::remove_dir_all(p.dir);
    }
}

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
