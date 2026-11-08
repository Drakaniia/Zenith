//! Download records, the downloads page's list. The WebView2 side of downloads, which keeps
//! these records up to date, is in `host::downloads`.

pub mod commands;

use rusqlite::{params, Connection};
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


pub fn downloads_remove(conn: &mut Connection, id: Option<i64>) -> rusqlite::Result<()> {
    match id {
        Some(id) => conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])?,
        None => conn.execute("DELETE FROM downloads WHERE state <> 'progress'", [])?,
    };
    Ok(())
}
