//! The torrents Zenith keeps between runs (`torrents/list.json`): which download each one is, where
//! it came from and how much it uploaded. The engine keeps its own state (pieces, paused, chosen
//! files) next to it in `torrents/session`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Saved {
    /// The download id in the downloads list.
    pub id: i64,
    /// Info hash, lowercase hex.
    pub hash: String,
    /// A magnet link, or a .torrent file kept in `torrents/files`.
    pub source: String,
    pub name: String,
    /// Folder the torrent's files go to (a multi-file torrent's own folder).
    pub dir: String,
    /// The file (single-file torrent) or folder shown and opened from the list.
    pub path: String,
    pub started: i64,
    /// Bytes uploaded over every run.
    pub uploaded: u64,
    /// When the download finished (it may still be seeding).
    pub completed: Option<i64>,
    /// Files the user chose; None for all of them.
    pub only_files: Option<Vec<usize>>,
    /// Started from an incognito window: never written to the download history.
    pub private: bool,
}

pub fn list_path(folder: &Path) -> PathBuf {
    folder.join("list.json")
}

/// Where a torrent's .torrent file is kept.
pub fn torrent_file(folder: &Path, hash: &str) -> PathBuf {
    folder.join("files").join(format!("{hash}.torrent"))
}

pub fn load(folder: &Path) -> Vec<Saved> {
    crate::storage::read_text(&list_path(folder))
        .and_then(|t| serde_json::from_str::<Vec<Saved>>(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.id > 0 && s.hash.len() == 40 && !s.source.is_empty())
        .collect()
}

pub fn save(folder: &Path, list: &[Saved]) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let json = serde_json::to_vec_pretty(list).map_err(std::io::Error::other)?;
    crate::storage::write_atomic(&list_path(folder), &json)
}

/// Keeps a .torrent file for a torrent (to restart it without asking peers again).
pub fn keep_torrent_file(folder: &Path, hash: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let path = torrent_file(folder, hash);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::storage::write_atomic(&path, bytes)?;
    Ok(path)
}

/// Removes a torrent's kept .torrent file.
pub fn forget_torrent_file(folder: &Path, hash: &str) {
    let _ = std::fs::remove_file(torrent_file(folder, hash));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_bad_entries() {
        let dir = std::env::temp_dir().join(format!("zenith-torrents-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load(&dir).is_empty());
        let good = Saved { id: 7, hash: "a".repeat(40), source: "magnet:?xt=urn:btih:x".into(), uploaded: 12, ..Default::default() };
        let bad = Saved { id: 8, hash: "short".into(), source: "x".into(), ..Default::default() };
        save(&dir, &[good.clone(), bad]).unwrap();
        assert_eq!(load(&dir), vec![good]);
        let kept = keep_torrent_file(&dir, &"b".repeat(40), b"d4:infodee").unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"d4:infodee");
        forget_torrent_file(&dir, &"b".repeat(40));
        assert!(!kept.exists());
        std::fs::write(list_path(&dir), "not json").unwrap();
        assert!(load(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
