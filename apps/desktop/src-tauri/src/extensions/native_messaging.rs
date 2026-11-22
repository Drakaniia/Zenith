//! Native messaging host discovery, strict manifest validation, per-profile consent policy,
//! and bounded 4-byte LE stdio message framing (Section 9.2).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
};

use crate::host::paths;
use crate::storage::{read_text, write_atomic};

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
pub const MAX_NATIVE_MSG_BYTES: usize = 1024 * 1024;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NativeHostInfo {
    pub name: String,
    pub description: String,
    pub path: String,
    pub allowed_origins: Vec<String>,
    pub consented: bool,
    pub source: &'static str,
}

#[derive(Deserialize)]
struct RawManifest {
    name: String,
    #[serde(default)]
    description: String,
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    allowed_origins: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct ConsentStore {
    #[serde(default)]
    allowed_hosts: HashSet<String>,
}

fn consent_path() -> PathBuf {
    paths().extensions().join("native_hosts.json")
}

fn load_consent() -> ConsentStore {
    read_text(&consent_path())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_consent(store: &ConsentStore) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
    write_atomic(&consent_path(), &bytes).map_err(|e| e.to_string())
}

pub fn set_host_consent(host_name: &str, allowed: bool) -> Result<(), String> {
    if !is_valid_host_name(host_name) {
        return Err("invalid native messaging host name".into());
    }
    let mut store = load_consent();
    if allowed {
        store.allowed_hosts.insert(host_name.to_string());
    } else {
        store.allowed_hosts.remove(host_name);
    }
    save_consent(&store)
}

pub fn is_valid_host_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..")
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

/// Validates and parses a native messaging manifest file (rejecting relative traversal, missing
/// executable, non-stdio hosts, oversized files, or malformed `allowed_origins`).
pub fn parse_manifest_file(manifest_path: &Path, expected_name: &str) -> Result<NativeHostInfo, String> {
    let meta = fs::metadata(manifest_path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err("native host manifest exceeds 64 KB limit".into());
    }
    let raw: RawManifest = serde_json::from_str(&fs::read_to_string(manifest_path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("malformed manifest JSON: {e}"))?;
    if !is_valid_host_name(&raw.name) || raw.name != expected_name {
        return Err("manifest host name mismatch or invalid".into());
    }
    if raw.kind != "stdio" {
        return Err("only stdio native messaging hosts are supported".into());
    }
    let exe_path = {
        let p = PathBuf::from(&raw.path);
        if p.is_absolute() {
            p
        } else {
            manifest_path.parent().unwrap_or(Path::new(".")).join(p)
        }
    };
    if exe_path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("native host path must not traverse parent directories".into());
    }
    let origins: Vec<String> = raw
        .allowed_origins
        .into_iter()
        .filter(|o| {
            o.strip_prefix("chrome-extension://")
                .and_then(|r| r.strip_suffix('/'))
                .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)))
        })
        .collect();
    if origins.is_empty() {
        return Err("native host manifest has no valid allowed_origins".into());
    }
    Ok(NativeHostInfo {
        name: raw.name,
        description: raw.description,
        path: exe_path.display().to_string(),
        allowed_origins: origins,
        consented: false,
        source: "registry",
    })
}

/// Enumerates validated native messaging hosts registered in Windows HKCU/HKLM.
pub fn list_hosts() -> Vec<NativeHostInfo> {
    let consent = load_consent();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let roots = [
        (HKEY_CURRENT_USER, "Software\\Zenith\\NativeMessagingHosts", "zenith-hkcu"),
        (HKEY_CURRENT_USER, "Software\\Google\\Chrome\\NativeMessagingHosts", "chrome-hkcu"),
        (HKEY_LOCAL_MACHINE, "Software\\Zenith\\NativeMessagingHosts", "zenith-hklm"),
        (HKEY_LOCAL_MACHINE, "Software\\Google\\Chrome\\NativeMessagingHosts", "chrome-hklm"),
    ];
    for (hive, subkey, source) in roots {
        for (name, manifest_path) in enum_registry_hosts(hive, subkey) {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Ok(mut info) = parse_manifest_file(Path::new(&manifest_path), &name) {
                info.consented = consent.allowed_hosts.contains(&info.name);
                info.source = source;
                out.push(info);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn enum_registry_hosts(root: HKEY, subkey: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut hkey = HKEY::default();
    let sub_w = wide(subkey);
    unsafe {
        if RegOpenKeyExW(root, PCWSTR(sub_w.as_ptr()), Some(0), KEY_READ, &mut hkey).is_err() {
            return results;
        }
        let mut idx = 0u32;
        loop {
            let mut name_buf = [0u16; 256];
            let mut name_len = name_buf.len() as u32;
            let status = RegEnumKeyExW(
                hkey,
                idx,
                Some(windows::core::PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            );
            if status.is_err() {
                break;
            }
            idx += 1;
            let host_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            let mut child = HKEY::default();
            let child_w = wide(&host_name);
            if RegOpenKeyExW(hkey, PCWSTR(child_w.as_ptr()), Some(0), KEY_READ, &mut child).is_ok() {
                let mut val_type = REG_SZ;
                let mut data = vec![0u8; 2048];
                let mut data_len = data.len() as u32;
                if RegQueryValueExW(child, PCWSTR::null(), None, Some(&mut val_type), Some(data.as_mut_ptr()), Some(&mut data_len)).is_ok()
                    && data_len >= 2
                {
                    let words: Vec<u16> = data[..data_len as usize]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .take_while(|&w| w != 0)
                        .collect();
                    let path = String::from_utf16_lossy(&words);
                    if !path.is_empty() {
                        results.push((host_name, path));
                    }
                }
                let _ = RegCloseKey(child);
            }
        }
        let _ = RegCloseKey(hkey);
    }
    results
}

/// Encodes a Chrome native messaging stdio frame (4-byte LE length + UTF-8 JSON, <= 1 MB).
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() > MAX_NATIVE_MSG_BYTES {
        return Err("native message exceeds 1 MB limit".into());
    }
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Decodes a single bounded Chrome native messaging stdio frame.
pub fn decode_frame(bytes: &[u8]) -> Result<&[u8], String> {
    if bytes.len() < 4 {
        return Err("truncated native message header".into());
    }
    let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    if len > MAX_NATIVE_MSG_BYTES {
        return Err("native message frame exceeds 1 MB limit".into());
    }
    if bytes.len() < 4 + len {
        return Err("truncated native message body".into());
    }
    Ok(&bytes[4..4 + len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_host_names_and_manifests() {
        assert!(is_valid_host_name("com.zenith.echo_host"));
        assert!(!is_valid_host_name("Com.Upper"));
        assert!(!is_valid_host_name("../bad"));

        let dir = std::env::temp_dir().join(format!("zenith-nm-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let good = dir.join("good.json");
        fs::write(
            &good,
            r#"{"name":"com.zenith.test","description":"Test","path":"C:\\Windows\\System32\\cmd.exe","type":"stdio","allowed_origins":["chrome-extension://fkjmjfpacnjjgnppeenhiolleglehgik/"]}"#,
        )
        .unwrap();
        let parsed = parse_manifest_file(&good, "com.zenith.test").unwrap();
        assert_eq!(parsed.name, "com.zenith.test");
        assert_eq!(parsed.allowed_origins.len(), 1);

        let bad_traversal = dir.join("bad.json");
        fs::write(
            &bad_traversal,
            r#"{"name":"com.zenith.bad","path":"..\\evil.exe","type":"stdio","allowed_origins":["chrome-extension://fkjmjfpacnjjgnppeenhiolleglehgik/"]}"#,
        )
        .unwrap();
        assert!(parse_manifest_file(&bad_traversal, "com.zenith.bad").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn framing_enforces_one_mb_bound() {
        let frame = encode_frame(br#"{"ok":true}"#).unwrap();
        assert_eq!(decode_frame(&frame).unwrap(), br#"{"ok":true}"#);
        let huge = vec![b'a'; MAX_NATIVE_MSG_BYTES + 1];
        assert!(encode_frame(&huge).is_err());
        let bad_hdr = (MAX_NATIVE_MSG_BYTES as u32 + 1).to_le_bytes();
        assert!(decode_frame(&bad_hdr).is_err());
    }
}
