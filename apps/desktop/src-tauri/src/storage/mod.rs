//! Persistence.
//!
//! - Session and settings: JSON snapshots written atomically (temp file + rename), so an abrupt
//!   exit always leaves the last complete snapshot on disk.
//! - History, bookmarks and download records: SQLite (WAL) owned by one worker thread, so the
//!   UI thread never blocks on disk I/O.
//!
//! The schema is created here; each domain keeps its own queries (`history`, `downloads`,
//! `notes`, `bookmarks`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Paths {
    /// %APPDATA%\<id>: session, settings, database.
    pub roaming: PathBuf,
    /// %LOCALAPPDATA%\<id>: WebView2 profile, extensions, filter lists.
    pub local: PathBuf,
}

impl Paths {
    pub fn session(&self) -> PathBuf {
        self.roaming.join("session.json")
    }
    pub fn settings(&self) -> PathBuf {
        self.roaming.join("settings.json")
    }
    pub fn db(&self) -> PathBuf {
        self.roaming.join("zenith.db")
    }
    pub fn webview_data(&self) -> PathBuf {
        self.local.join("WebView2")
    }
    pub fn extensions(&self) -> PathBuf {
        self.local.join("extensions")
    }
    pub fn filters(&self) -> PathBuf {
        self.local.join("filters")
    }
    /// Start-page backgrounds the user uploaded, one data URL per file.
    pub fn wallpapers(&self) -> PathBuf {
        self.local.join("wallpapers")
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Days since 1970-01-01 as (year, month, day), in the proleptic Gregorian calendar.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// "2026-11-07" (UTC) for a time in milliseconds.
pub fn date_string(ms: i64) -> String {
    let (y, m, d) = civil_from_days(ms.div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Writes `data` to `path` atomically: write + flush a sibling temp file, then rename over.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}

pub fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        // Tolerate a UTF-8 byte-order mark (files edited by hand or written by Windows tools).
        .map(|s| s.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(s))
        .filter(|s| !s.trim().is_empty())
}

// ---------------------------------------------------------------------------------------------
// Database worker

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

#[derive(Clone)]
pub struct Db {
    tx: mpsc::Sender<Job>,
}

impl Db {
    pub fn open(path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("zenith-db".into())
            .spawn(move || {
                let mut conn = match open_connection(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[zenith] database unavailable ({e}); using in-memory store");
                        let c = Connection::open_in_memory().expect("in-memory sqlite");
                        migrate(&c).expect("migrate in-memory sqlite");
                        c
                    }
                };
                while let Ok(job) = rx.recv() {
                    job(&mut conn);
                }
            })
            .expect("spawn db thread");
        Self { tx }
    }

    /// Fire-and-forget write.
    pub fn exec(&self, f: impl FnOnce(&mut Connection) + Send + 'static) {
        let _ = self.tx.send(Box::new(f));
    }

    /// Waits (up to `timeout`) until every write queued so far is done. Used before the process
    /// exits abruptly (the update installer ends the app with `process::exit`).
    pub fn flush(&self, timeout: std::time::Duration) {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Box::new(move |_| {
            let _ = tx.send(());
        }));
        let _ = rx.recv_timeout(timeout);
    }

    pub async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.exec(move |c| {
            let _ = tx.send(f(c));
        });
        rx.await
            .map_err(|_| "database worker stopped".to_string())?
            .map_err(|e| e.to_string())
    }
}

fn open_connection(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let conn = Connection::open(path)?;
    // An incognito window writes bookmarks to the same database from another process.
    conn.busy_timeout(std::time::Duration::from_secs(3))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    migrate(&conn)?;
    Ok(conn)
}

pub(crate) fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY,
            url TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL DEFAULT '',
            visit_count INTEGER NOT NULL DEFAULT 1,
            last_visit INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS history_last_visit ON history(last_visit DESC);

        CREATE TABLE IF NOT EXISTS downloads (
            id INTEGER PRIMARY KEY,
            url TEXT NOT NULL,
            path TEXT NOT NULL,
            mime TEXT NOT NULL DEFAULT '',
            total INTEGER NOT NULL DEFAULT 0,
            received INTEGER NOT NULL DEFAULT 0,
            state TEXT NOT NULL,
            started INTEGER NOT NULL,
            ended INTEGER
        );
        CREATE TABLE IF NOT EXISTS notes (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            body TEXT NOT NULL DEFAULT '',
            created INTEGER NOT NULL,
            updated INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS notes_updated ON notes(updated DESC);",
    )?;
    // Bookmarks have their own tables, and the old flat list is converted on the first run.
    crate::bookmarks::migrate(conn)
}

// ---------------------------------------------------------------------------------------------
// Bookmarks
//
// The bookmark tree lives in `bookmarks`. What stays here is the flat record the importers for
// other browsers produce (the folder is a path like "Chrome / Dev"); `bookmarks::import_paths`
// places those in the tree.

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    #[serde(default)]
    pub id: i64,
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub created: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_computed_without_a_calendar_crate() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(date_string(1_700_000_000_000), "2023-11-14");
        assert_eq!(date_string(86_399_999), "1970-01-01");
        assert_eq!(date_string(86_400_000), "1970-01-02");
        assert_eq!(date_string(-1), "1969-12-31");
    }

    #[test]
    fn atomic_write_replaces_file() {
        let dir = std::env::temp_dir().join(format!("zenith-test-{}", now_ms()));
        let file = dir.join("s.json");
        write_atomic(&file, b"{\"a\":1}").unwrap();
        write_atomic(&file, b"{\"a\":2}").unwrap();
        assert_eq!(read_text(&file).unwrap(), "{\"a\":2}");
        assert!(!file.with_extension("tmp").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
