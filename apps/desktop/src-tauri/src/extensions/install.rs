//! Installing extensions: unpacked folders, `.crx` packages, and Chrome Web Store downloads
//! staged into Zenith's data folder and registered in `registry.json`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine as _;
use serde::Serialize;

use super::{crx, info_for, load_registry, read_manifest, remove, save_registry, ExtensionInfo};
use crate::host::{emit_tab, paths};
use crate::storage::read_text;

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

pub fn stage_folder(source: &Path) -> Result<PathBuf, String> {
    let manifest = read_manifest(source).ok_or("the folder has no valid manifest.json")?;
    let dest = paths()
        .extensions()
        .join(format!("{}-{}", slug(&manifest.name), crate::storage::now_ms()));
    copy_dir(source, &dest).map_err(|e| format!("could not copy the extension: {e}"))?;
    Ok(dest)
}

pub struct Staged {
    pub dir: PathBuf,
    pub crx_id: Option<String>,
}

fn manifest_key_id(dir: &Path) -> Option<String> {
    let text = read_text(&dir.join("manifest.json"))?;
    let m: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let key = m.get("key")?.as_str()?;
    let der = base64::engine::general_purpose::STANDARD.decode(key.trim()).ok()?;
    Some(crx::id_from_key(&der))
}

/// Computes Chromium's deterministic unpacked extension ID from the canonical path when
/// `manifest.json` does not carry an RSA `key`.
fn id_for_unpacked_dir(dir: &Path) -> String {
    let canonical = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let raw = canonical.to_string_lossy();
    id_for_path(raw.strip_prefix(r"\\?\").unwrap_or(&raw))
}

/// Chromium hashes the UTF-16LE path (Windows `FilePath`) with the drive letter uppercased.
fn id_for_path(path: &str) -> String {
    let mut path = path.to_string();
    if path.as_bytes().get(1) == Some(&b':') {
        path[..1].make_ascii_uppercase();
    }
    let bytes: Vec<u8> = path.encode_utf16().flat_map(u16::to_le_bytes).collect();
    crx::id_from_key(&bytes)
}

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

pub fn stage_crx(bytes: &[u8]) -> Result<Staged, String> {
    stage_crx_in(bytes, &paths().extensions())
}

fn stage_crx_in(bytes: &[u8], root: &Path) -> Result<Staged, String> {
    let package = crx::parse(bytes)?;
    let zip = package.zip;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stamp = crate::storage::now_ms();
    let archive = root.join(format!("pkg-{stamp}.zip"));
    fs::write(&archive, zip).map_err(|e| e.to_string())?;
    let dest = root.join(format!("crx-{stamp}"));
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let tar = Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()))
        .join("System32")
        .join("tar.exe");
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

pub fn download_from_store(store_id: &str) -> Result<Vec<u8>, String> {
    crate::platform::net::get(&crx::download_url(store_id, &crx::prodversion()), MAX_PACKAGE, 60_000)
        .map_err(|e| format!("could not download the extension from the Chrome Web Store ({e})"))
}

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
    let Some(manifest) = read_manifest(&dir) else {
        let _ = fs::remove_dir_all(&dir);
        return done(Err("the extension has no valid manifest.json".into()));
    };
    let id = manifest_key_id(&dir).unwrap_or_else(|| id_for_unpacked_dir(&dir));
    let mut registry = load_registry();
    registry.folders.insert(id.clone(), dir.clone());
    registry.disabled.remove(&id);
    if let Some(s) = store_id {
        registry.sources.insert(id.clone(), s);
    }
    save_registry(&registry);
    let info = info_for(id, manifest.name, true, &registry);
    super::load_live(&dir, move || done(Ok(info)));
}

// ---------------------------------------------------------------------------------------------
// Install offers

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
    pub warnings: Vec<String>,
    pub replaces: Option<String>,
    pub store_id: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstallEvent {
    pub tab_id: String,
    pub state: &'static str,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offer: Option<InstallOffer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub enum Fetch {
    Store(String),
    Url { url: String, referer: String, user_agent: String, allow_private: bool, hint: String },
    Bytes { bytes: Vec<u8>, hint: String },
}

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
            (bytes, crx::store_id(&format!("{hint} {url} {final_url}").to_ascii_lowercase()))
        }
        Fetch::Bytes { bytes, hint } => (bytes, crx::store_id(&hint.to_ascii_lowercase())),
    };
    let staged = stage_crx_in(&bytes, &pending_root())?;
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

pub fn accept(token: u64, done: impl FnOnce(Result<ExtensionInfo, String>) + 'static) {
    let Some(p) = take_pending(token) else {
        return done(Err("this install request has expired; try again".into()));
    };
    let Some(name) = p.dir.file_name() else { return done(Err("bad staging folder".into())) };
    let dest = paths().extensions().join(name);
    if let Err(e) = fs::rename(&p.dir, &dest) {
        let _ = fs::remove_dir_all(&p.dir);
        return done(Err(format!("could not prepare the extension: {e}")));
    }
    install_folder(dest, p.store_id, done);
}

pub fn discard(token: u64) {
    if let Some(p) = take_pending(token) {
        let _ = fs::remove_dir_all(p.dir);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unpacked_id_matches_chromium() {
        // Verified against live Chromium 156 `Extensions.loadUnpacked`.
        assert_eq!(
            super::id_for_path(r"C:\Users\Qwenzy\AppData\Local\Temp\zenith-e2e-ext-test"),
            "cjiikgpfnbbcmpcjkibdjfldfmjnmijb"
        );
        assert_eq!(super::id_for_path(r"C:\Zenith\extensions\marker"), "kfdmipabondplkfccpokebienbmfcpla");
        assert_eq!(super::id_for_path(r"c:\Zenith\extensions\marker"), "kfdmipabondplkfccpokebienbmfcpla");
    }
}
