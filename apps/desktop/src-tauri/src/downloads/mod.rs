//! Download records, the downloads page's list. The WebView2 side of downloads, which keeps
//! these records up to date, is in `host::downloads`.

mod again;
pub mod commands;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRecord {
    pub id: i64,
    pub url: String,
    pub path: String,
    pub mime: String,
    pub total: i64,
    pub received: i64,
    pub state: String,
    pub started: i64,
    pub ended: Option<i64>,
}

pub fn download_upsert(conn: &mut Connection, d: &DownloadRecord) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO downloads (id, url, path, mime, total, received, state, started, ended)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET path = ?3, total = ?5, received = ?6, state = ?7, ended = ?9",
        params![d.id, d.url, d.path, d.mime, d.total, d.received, d.state, d.started, d.ended],
    )?;
    Ok(())
}

pub fn downloads_list(conn: &mut Connection, limit: i64) -> rusqlite::Result<Vec<DownloadRecord>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, url, path, mime, total, received, state, started, ended FROM downloads
         ORDER BY started DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(DownloadRecord {
            id: r.get(0)?,
            url: r.get(1)?,
            path: r.get(2)?,
            mime: r.get(3)?,
            total: r.get(4)?,
            received: r.get(5)?,
            state: r.get(6)?,
            started: r.get(7)?,
            ended: r.get(8)?,
        })
    })?;
    rows.collect()
}


/// One record of the history.
pub fn download_get(conn: &mut Connection, id: i64) -> rusqlite::Result<Option<DownloadRecord>> {
    conn.query_row(
        "SELECT id, url, path, mime, total, received, state, started, ended FROM downloads WHERE id = ?1",
        params![id],
        |r| {
            Ok(DownloadRecord {
                id: r.get(0)?,
                url: r.get(1)?,
                path: r.get(2)?,
                mime: r.get(3)?,
                total: r.get(4)?,
                received: r.get(5)?,
                state: r.get(6)?,
                started: r.get(7)?,
                ended: r.get(8)?,
            })
        },
    )
    .optional()
}

/// Where a finished download was saved, from the history.
pub fn completed_path(conn: &mut Connection, id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT path FROM downloads WHERE id = ?1 AND state = 'completed'",
        params![id],
        |r| r.get(0),
    )
    .optional()
}

/// Paths of finished downloads of `url`, newest first.
pub fn completed_paths_for_url(conn: &mut Connection, url: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT path FROM downloads WHERE url = ?1 AND state = 'completed' ORDER BY started DESC LIMIT 10",
    )?;
    let rows = stmt.query_map(params![url], |r| r.get(0))?;
    rows.collect()
}

/// The file name of an earlier, finished download of this web address whose file is still on disk.
/// Blocks briefly on the database; only for http(s) addresses (blob: and data: ones are unique).
pub fn already_downloaded(url: &str) -> Option<String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let url = url.to_string();
    let paths = crate::host::db().query_blocking(std::time::Duration::from_millis(400), move |c| completed_paths_for_url(c, &url))?;
    paths.into_iter().map(std::path::PathBuf::from).find(|p| p.is_file()).and_then(|p| Some(p.file_name()?.to_string_lossy().into_owned()))
}

/// Start of the message shown when a download is skipped because the file is already there.
pub const ALREADY_PREFIX: &str = "Already downloaded";

pub fn already_message(name: &str) -> String {
    format!("{ALREADY_PREFIX}: {name}")
}

/// Downloads finished in this session, incognito ones included (they are not in the history):
/// the files the downloads button may drag out.
static FINISHED: LazyLock<Mutex<HashMap<i64, String>>> = LazyLock::new(Default::default);

pub fn remember_finished(id: i64, path: &str) {
    if let Ok(mut m) = FINISHED.lock() {
        m.insert(id, path.to_string());
    }
}

pub fn finished_path(id: i64) -> Option<String> {
    FINISHED.lock().ok()?.get(&id).cloned()
}

/// How long a new download may be listed as running before it shows up among the active ones.
pub const STALE_AFTER_MS: i64 = 5_000;

/// Marks as failed the records still saying "progress" that nothing is downloading any more
/// (Zenith closed or crashed during the download): they would otherwise show as running forever,
/// with buttons that no longer work. `live` are the ids of the downloads still running.
pub fn settle_stale(conn: &mut Connection, live: &[i64], now: i64) -> rusqlite::Result<usize> {
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare_cached("SELECT id FROM downloads WHERE state = 'progress' AND started < ?1")?;
        let rows = stmt.query_map(params![now - STALE_AFTER_MS], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<Vec<i64>>>()?
    };
    let mut n = 0;
    for id in ids.into_iter().filter(|id| !live.contains(id)) {
        n += conn.execute(
            "UPDATE downloads SET state = 'interrupted', ended = COALESCE(ended, ?2) WHERE id = ?1 AND state = 'progress'",
            params![id, now],
        )?;
    }
    Ok(n)
}

pub fn downloads_remove(conn: &mut Connection, id: Option<i64>) -> rusqlite::Result<()> {
    match id {
        Some(id) => conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])?,
        None => conn.execute("DELETE FROM downloads WHERE state <> 'progress'", [])?,
    };
    Ok(())
}
