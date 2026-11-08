//! Imports bookmarks and history from other browsers installed for this Windows user.
//!
//! Sources are found by scanning the browsers' standard profile folders. The UI only ever passes
//! back a source id from that scan, never a path, so it cannot point the importer at an
//! arbitrary file. Databases are copied to a temporary folder first: the other browser may be
//! running and holding them open, and its files must never be modified.
//!
//! Only bookmarks and history are read. Passwords, cookies and open tabs are not imported.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;

use crate::storage::{now_ms, Bookmark};

/// Most history entries taken from one profile (newest first).
const HISTORY_LIMIT: i64 = 20_000;
/// Microseconds between 1601-01-01 (Chromium's epoch) and 1970-01-01, in milliseconds.
const CHROMIUM_EPOCH_OFFSET_MS: i64 = 11_644_473_600_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Engine {
    Chromium,
    Firefox,
}

struct Browser {
    key: &'static str,
    name: &'static str,
    /// Name used for the bookmarks folder the import goes into.
    short: &'static str,
    engine: Engine,
    /// Environment variable of the base folder.
    base: &'static str,
    /// Folder holding the profiles, relative to `base`.
    dir: &'static str,
    /// The folder is itself the only profile (Opera).
    single: bool,
}

const BROWSERS: &[Browser] = &[
    Browser { key: "chrome", name: "Google Chrome", short: "Chrome", engine: Engine::Chromium, base: "LOCALAPPDATA", dir: r"Google\Chrome\User Data", single: false },
    Browser { key: "edge", name: "Microsoft Edge", short: "Edge", engine: Engine::Chromium, base: "LOCALAPPDATA", dir: r"Microsoft\Edge\User Data", single: false },
    Browser { key: "brave", name: "Brave", short: "Brave", engine: Engine::Chromium, base: "LOCALAPPDATA", dir: r"BraveSoftware\Brave-Browser\User Data", single: false },
    Browser { key: "vivaldi", name: "Vivaldi", short: "Vivaldi", engine: Engine::Chromium, base: "LOCALAPPDATA", dir: r"Vivaldi\User Data", single: false },
    Browser { key: "chromium", name: "Chromium", short: "Chromium", engine: Engine::Chromium, base: "LOCALAPPDATA", dir: r"Chromium\User Data", single: false },
    Browser { key: "opera", name: "Opera", short: "Opera", engine: Engine::Chromium, base: "APPDATA", dir: r"Opera Software\Opera Stable", single: true },
    Browser { key: "operagx", name: "Opera GX", short: "Opera GX", engine: Engine::Chromium, base: "APPDATA", dir: r"Opera Software\Opera GX Stable", single: true },
    Browser { key: "firefox", name: "Firefox", short: "Firefox", engine: Engine::Firefox, base: "APPDATA", dir: r"Mozilla\Firefox\Profiles", single: false },
    Browser { key: "zen", name: "Zen", short: "Zen", engine: Engine::Firefox, base: "APPDATA", dir: r"zen\Profiles", single: false },
];

/// A browser profile that has something to import.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportSource {
    /// Opaque id, passed back to `import_run`.
    pub id: String,
    /// Browser key ("chrome", "firefox", …), for the UI's icon.
    pub browser: String,
    pub name: String,
    /// Profile name; empty for browsers with a single profile folder.
    pub profile: String,
    pub bookmarks: bool,
    pub history: bool,
}

struct Found {
    source: ImportSource,
    dir: PathBuf,
    engine: Engine,
    short: &'static str,
}

/// A history entry read from another browser.
#[derive(Debug, Clone, PartialEq)]
pub struct Visit {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visit: i64,
}

/// What was read from a profile, before it is written to Zenith's database.
#[derive(Default, Debug)]
pub struct Imported {
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<Visit>,
    /// Parts that could not be read, as sentences for the UI.
    pub errors: Vec<String>,
}

/// Profiles with bookmarks or history, in a stable order (browser list order, then profile).
pub fn detect() -> Vec<ImportSource> {
    scan().into_iter().map(|f| f.source).collect()
}

fn scan() -> Vec<Found> {
    let mut out = Vec::new();
    for b in BROWSERS {
        let Some(base) = std::env::var_os(b.base) else { continue };
        let root = PathBuf::from(base).join(b.dir);
        if !root.is_dir() {
            continue;
        }
        let mut profiles: Vec<(String, PathBuf)> = if b.single {
            vec![(String::new(), root.clone())]
        } else {
            fs::read_dir(&root)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                        .filter(|(name, _)| b.engine == Engine::Firefox || is_chromium_profile_dir(name))
                        .collect()
                })
                .unwrap_or_default()
        };
        profiles.sort_by(|a, b| profile_order(&a.0).cmp(&profile_order(&b.0)).then_with(|| a.0.cmp(&b.0)));
        let names = if b.engine == Engine::Chromium && !b.single {
            fs::read_to_string(root.join("Local State")).map(|s| chromium_profile_names(&s)).unwrap_or_default()
        } else {
            HashMap::new()
        };
        for (dir_name, dir) in profiles {
            let (bookmarks, history) = match b.engine {
                Engine::Chromium => (dir.join("Bookmarks").is_file(), dir.join("History").is_file()),
                Engine::Firefox => {
                    let places = dir.join("places.sqlite").is_file();
                    (places, places)
                }
            };
            if !bookmarks && !history {
                continue;
            }
            let profile = match b.engine {
                Engine::Chromium if b.single => String::new(),
                Engine::Chromium => names.get(&dir_name).cloned().unwrap_or_else(|| dir_name.clone()),
                Engine::Firefox => firefox_profile_name(&dir_name),
            };
            out.push(Found {
                source: ImportSource {
                    id: format!("{}:{}", b.key, dir_name),
                    browser: b.key.to_string(),
                    name: b.name.to_string(),
                    profile,
                    bookmarks,
                    history,
                },
                dir,
                engine: b.engine,
                short: b.short,
            });
        }
    }
    out
}

fn is_chromium_profile_dir(name: &str) -> bool {
    name == "Default" || name.starts_with("Profile ")
}

/// "Default" first, then "Profile 1", "Profile 2", … numerically.
fn profile_order(name: &str) -> (u8, u32) {
    if name == "Default" {
        return (0, 0);
    }
    match name.strip_prefix("Profile ").and_then(|n| n.parse().ok()) {
        Some(n) => (1, n),
        None => (2, 0),
    }
}

/// Display names of Chromium profiles from the browser's `Local State` file.
pub fn chromium_profile_names(local_state: &str) -> HashMap<String, String> {
    let value: serde_json::Value = serde_json::from_str(local_state).unwrap_or_default();
    value["profile"]["info_cache"]
        .as_object()
        .map(|cache| {
            cache
                .iter()
                .filter_map(|(dir, info)| {
                    let name = info["name"].as_str()?.trim();
                    (!name.is_empty()).then(|| (dir.clone(), name.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// "x1y2z3.default-release" -> "default-release".
pub fn firefox_profile_name(dir: &str) -> String {
    match dir.split_once('.') {
        Some((_, rest)) if !rest.is_empty() => rest.to_string(),
        _ => dir.to_string(),
    }
}

/// Reads the chosen parts of a profile found by `detect`.
pub fn read(id: &str, bookmarks: bool, history: bool, history_days: i64) -> Result<Imported, String> {
    let found = scan()
        .into_iter()
        .find(|f| f.source.id == id)
        .ok_or_else(|| "That browser profile is no longer available.".to_string())?;
    let name = &found.source.name;
    let mut out = Imported::default();
    let since_ms = if history_days > 0 { now_ms() - history_days * 86_400_000 } else { 0 };

    match found.engine {
        Engine::Chromium => {
            if bookmarks && found.source.bookmarks {
                match fs::read_to_string(found.dir.join("Bookmarks")) {
                    Ok(json) => out.bookmarks = chromium_bookmarks(&json, found.short),
                    Err(_) => out.errors.push(format!("Couldn't read {name}'s bookmarks.")),
                }
            }
            if history && found.source.history {
                match with_copy(&found.dir.join("History"), |c| chromium_history(c, since_ms)) {
                    Ok(v) => out.history = v,
                    Err(_) => out.errors.push(format!("Couldn't read {name}'s history. Close {name} and try again.")),
                }
            }
        }
        Engine::Firefox => {
            if (bookmarks || history) && found.source.bookmarks {
                let places = found.dir.join("places.sqlite");
                let read = with_copy(&places, |c| {
                    let b = if bookmarks { firefox_bookmarks(c, found.short)? } else { Vec::new() };
                    let h = if history { firefox_history(c, since_ms)? } else { Vec::new() };
                    Ok((b, h))
                });
                match read {
                    Ok((b, h)) => {
                        out.bookmarks = b;
                        out.history = h;
                    }
                    Err(_) => out.errors.push(format!("Couldn't read {name}'s data. Close {name} and try again.")),
                }
            }
        }
    }
    Ok(out)
}

/// Copies a SQLite database (and its write-ahead log) to a temporary folder, opens the copy and
/// runs `f` on it. The original file is never opened by SQLite.
fn with_copy<T>(src: &Path, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, String> {
    let dir = std::env::temp_dir().join(format!("zenith-import-{}-{}", std::process::id(), now_ms()));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let result = (|| {
        let file = src.file_name().ok_or("bad path")?.to_string_lossy().into_owned();
        let dst = dir.join(&file);
        fs::copy(src, &dst).map_err(|e| e.to_string())?;
        let wal = src.with_file_name(format!("{file}-wal"));
        if wal.is_file() {
            let _ = fs::copy(&wal, dir.join(format!("{file}-wal")));
        }
        let conn = Connection::open_with_flags(&dst, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .map_err(|e| e.to_string())?;
        f(&conn).map_err(|e| e.to_string())
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

fn is_web(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://")) && url.len() <= 4096
}

/// Chromium timestamps: microseconds since 1601-01-01.
pub fn chromium_time_ms(us: i64) -> i64 {
    if us <= 0 {
        0
    } else {
        us / 1000 - CHROMIUM_EPOCH_OFFSET_MS
    }
}

/// Bookmarks from a Chromium `Bookmarks` file. The bookmarks bar becomes the folder `label`;
/// other roots and nested folders go below it ("Chrome / Other bookmarks / Recipes").
pub fn chromium_bookmarks(json: &str, label: &str) -> Vec<Bookmark> {
    let value: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let roots = &value["roots"];
    let mut out = Vec::new();
    for (key, sub) in [("bookmark_bar", None), ("other", Some("Other bookmarks")), ("synced", Some("Mobile bookmarks"))] {
        let node = &roots[key];
        if node.is_null() {
            continue;
        }
        let mut path = vec![label.to_string()];
        if let Some(sub) = sub {
            path.push(sub.to_string());
        }
        walk_chromium(node, &mut path, &mut out);
    }
    out
}

fn walk_chromium(node: &serde_json::Value, path: &mut Vec<String>, out: &mut Vec<Bookmark>) {
    if path.len() > 32 {
        return;
    }
    for child in node["children"].as_array().into_iter().flatten() {
        let name = child["name"].as_str().unwrap_or("").trim().to_string();
        match child["type"].as_str() {
            Some("url") => {
                let Some(url) = child["url"].as_str() else { continue };
                if !is_web(url) {
                    continue;
                }
                let created = child["date_added"].as_str().and_then(|s| s.parse().ok()).map(chromium_time_ms).unwrap_or(0);
                out.push(Bookmark { id: 0, url: url.to_string(), title: name, folder: path.join(" / "), position: 0, created });
            }
            Some("folder") => {
                path.push(if name.is_empty() { "Untitled folder".into() } else { name });
                walk_chromium(child, path, out);
                path.pop();
            }
            _ => {}
        }
    }
}

fn chromium_history(conn: &Connection, since_ms: i64) -> rusqlite::Result<Vec<Visit>> {
    let since = if since_ms > 0 { (since_ms + CHROMIUM_EPOCH_OFFSET_MS) * 1000 } else { 0 };
    let mut stmt = conn.prepare(
        "SELECT url, COALESCE(title, ''), visit_count, last_visit_time FROM urls
         WHERE hidden = 0 AND last_visit_time >= ?1 ORDER BY last_visit_time DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![since, HISTORY_LIMIT], |r| {
        Ok(Visit {
            url: r.get(0)?,
            title: r.get(1)?,
            visit_count: r.get::<_, i64>(2)?.max(1),
            last_visit: chromium_time_ms(r.get(3)?),
        })
    })?;
    Ok(rows.filter_map(Result::ok).filter(|v| is_web(&v.url)).collect())
}

/// Bookmarks from Firefox's `places.sqlite`. The toolbar becomes the folder `label`; the menu,
/// "Other bookmarks" and mobile roots go below it. Tags are not bookmarks and are skipped.
pub fn firefox_bookmarks(conn: &Connection, label: &str) -> rusqlite::Result<Vec<Bookmark>> {
    let mut stmt = conn.prepare("SELECT id, parent, COALESCE(title, ''), COALESCE(guid, '') FROM moz_bookmarks WHERE type = 2")?;
    let folders: HashMap<i64, (i64, String, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?
        .filter_map(Result::ok)
        .collect();
    let mut stmt = conn.prepare(
        "SELECT b.parent, COALESCE(NULLIF(b.title, ''), p.title, ''), p.url, COALESCE(b.dateAdded, 0)
         FROM moz_bookmarks b JOIN moz_places p ON p.id = b.fk
         WHERE b.type = 1 ORDER BY b.parent, b.position",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
    let mut out = Vec::new();
    for (parent, title, url, added) in rows.filter_map(Result::ok) {
        if !is_web(&url) {
            continue;
        }
        let Some(folder) = firefox_folder_path(&folders, parent, label) else { continue };
        out.push(Bookmark { id: 0, url, title, folder, position: 0, created: added / 1000 });
    }
    Ok(out)
}

fn firefox_folder_path(folders: &HashMap<i64, (i64, String, String)>, mut id: i64, label: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for _ in 0..64 {
        let (parent, title, guid) = folders.get(&id)?;
        match guid.as_str() {
            "root________" | "toolbar_____" => break,
            "menu________" => {
                parts.push("Bookmarks menu".into());
                break;
            }
            "unfiled_____" => {
                parts.push("Other bookmarks".into());
                break;
            }
            "mobile______" => {
                parts.push("Mobile bookmarks".into());
                break;
            }
            "tags________" => return None,
            _ => parts.push(if title.trim().is_empty() { "Untitled folder".into() } else { title.trim().to_string() }),
        }
        id = *parent;
    }
    parts.push(label.to_string());
    parts.reverse();
    Some(parts.join(" / "))
}

fn firefox_history(conn: &Connection, since_ms: i64) -> rusqlite::Result<Vec<Visit>> {
    let mut stmt = conn.prepare(
        "SELECT url, COALESCE(title, ''), visit_count, last_visit_date FROM moz_places
         WHERE hidden = 0 AND last_visit_date IS NOT NULL AND last_visit_date >= ?1
         ORDER BY last_visit_date DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![since_ms.max(0) * 1000, HISTORY_LIMIT], |r| {
        Ok(Visit {
            url: r.get(0)?,
            title: r.get(1)?,
            visit_count: r.get::<_, i64>(2)?.max(1),
            last_visit: r.get::<_, i64>(3)? / 1000,
        })
    })?;
    Ok(rows.filter_map(Result::ok).filter(|v| is_web(&v.url)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_bookmarks_keep_folders() {
        let json = r#"{"roots":{
            "bookmark_bar":{"type":"folder","name":"Bookmarks bar","children":[
                {"type":"url","name":"Rust","url":"https://rust-lang.org/","date_added":"13350000000000000"},
                {"type":"folder","name":"Dev","children":[
                    {"type":"url","name":"MDN","url":"https://developer.mozilla.org/"},
                    {"type":"url","name":"Local","url":"javascript:alert(1)"}
                ]}
            ]},
            "other":{"type":"folder","name":"Other bookmarks","children":[
                {"type":"url","name":"Example","url":"http://example.com/"}
            ]},
            "synced":{"type":"folder","name":"Mobile bookmarks","children":[]}
        }}"#;
        let list = chromium_bookmarks(json, "Chrome");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].folder, "Chrome");
        assert_eq!(list[0].title, "Rust");
        assert!(list[0].created > 1_600_000_000_000, "converted from the 1601 epoch");
        assert_eq!(list[1].folder, "Chrome / Dev");
        assert_eq!(list[2].folder, "Chrome / Other bookmarks");
        assert!(chromium_bookmarks("not json", "Chrome").is_empty());
    }

    #[test]
    fn profile_names() {
        let names = chromium_profile_names(r#"{"profile":{"info_cache":{"Default":{"name":"Work"},"Profile 2":{"name":" "}}}}"#);
        assert_eq!(names.get("Default").map(String::as_str), Some("Work"));
        assert!(!names.contains_key("Profile 2"));
        assert_eq!(firefox_profile_name("ab12cd.default-release"), "default-release");
        assert_eq!(firefox_profile_name("plain"), "plain");
        assert!(is_chromium_profile_dir("Profile 3"));
        assert!(!is_chromium_profile_dir("System Profile"));
        let mut dirs = vec!["Profile 10", "Profile 2", "Default"];
        dirs.sort_by_key(|d| profile_order(d));
        assert_eq!(dirs, ["Default", "Profile 2", "Profile 10"]);
    }

    fn firefox_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER,
                 last_visit_date INTEGER, hidden INTEGER DEFAULT 0);
             CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, parent INTEGER,
                 position INTEGER, title TEXT, dateAdded INTEGER, guid TEXT);
             INSERT INTO moz_bookmarks VALUES (1, 2, NULL, 0, 0, '', 0, 'root________');
             INSERT INTO moz_bookmarks VALUES (2, 2, NULL, 1, 0, 'menu', 0, 'menu________');
             INSERT INTO moz_bookmarks VALUES (3, 2, NULL, 1, 1, 'toolbar', 0, 'toolbar_____');
             INSERT INTO moz_bookmarks VALUES (4, 2, NULL, 1, 2, 'tags', 0, 'tags________');
             INSERT INTO moz_bookmarks VALUES (5, 2, NULL, 3, 0, 'News', 0, 'folderNews01');
             INSERT INTO moz_bookmarks VALUES (6, 2, NULL, 4, 0, 'mytag', 0, 'tagfolder001');
             INSERT INTO moz_places VALUES (10, 'https://a.example/', 'A', 3, 1700000000000000, 0);
             INSERT INTO moz_places VALUES (11, 'https://b.example/', 'B', 1, 1600000000000000, 0);
             INSERT INTO moz_places VALUES (12, 'place:sort=8', 'Recent', 1, 1700000000000000, 0);
             INSERT INTO moz_places VALUES (13, 'https://hidden.example/', 'H', 1, 1700000000000000, 1);
             INSERT INTO moz_bookmarks VALUES (20, 1, 10, 3, 0, 'Toolbar A', 1700000000000000, 'bm1');
             INSERT INTO moz_bookmarks VALUES (21, 1, 11, 5, 0, NULL, 0, 'bm2');
             INSERT INTO moz_bookmarks VALUES (22, 1, 10, 2, 0, 'Menu A', 0, 'bm3');
             INSERT INTO moz_bookmarks VALUES (23, 1, 10, 6, 0, NULL, 0, 'bm4');
             INSERT INTO moz_bookmarks VALUES (24, 1, 12, 3, 1, 'Smart', 0, 'bm5');",
        )
        .unwrap();
        c
    }

    #[test]
    fn firefox_bookmarks_and_history() {
        let c = firefox_db();
        let mut list = firefox_bookmarks(&c, "Firefox").unwrap();
        list.sort_by(|a, b| a.title.cmp(&b.title));
        let got: Vec<(&str, &str)> = list.iter().map(|b| (b.title.as_str(), b.folder.as_str())).collect();
        assert_eq!(got, [("B", "Firefox / News"), ("Menu A", "Firefox / Bookmarks menu"), ("Toolbar A", "Firefox")]);
        assert_eq!(list[2].created, 1_700_000_000_000);

        let all = firefox_history(&c, 0).unwrap();
        assert_eq!(all.iter().map(|v| v.url.as_str()).collect::<Vec<_>>(), ["https://a.example/", "https://b.example/"]);
        assert_eq!(all[0].last_visit, 1_700_000_000_000);
        let recent = firefox_history(&c, 1_650_000_000_000).unwrap();
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn chromium_history_reads_copy() {
        let dir = std::env::temp_dir().join(format!("zenith-import-test-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("History");
        {
            let c = Connection::open(&file).unwrap();
            let t = |ms: i64| (ms + CHROMIUM_EPOCH_OFFSET_MS) * 1000;
            c.execute_batch(&format!(
                "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER,
                     last_visit_time INTEGER, hidden INTEGER DEFAULT 0);
                 INSERT INTO urls VALUES (1, 'https://new.example/', 'New', 4, {}, 0);
                 INSERT INTO urls VALUES (2, 'https://old.example/', 'Old', 1, {}, 0);
                 INSERT INTO urls VALUES (3, 'chrome://settings/', 'Settings', 1, {}, 0);",
                t(1_700_000_000_000),
                t(1_500_000_000_000),
                t(1_700_000_000_000)
            ))
            .unwrap();
        }
        let all = with_copy(&file, |c| chromium_history(c, 0)).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], Visit { url: "https://new.example/".into(), title: "New".into(), visit_count: 4, last_visit: 1_700_000_000_000 });
        let recent = with_copy(&file, |c| chromium_history(c, 1_600_000_000_000)).unwrap();
        assert_eq!(recent.len(), 1);
        assert!(with_copy(&dir.join("missing"), |c| chromium_history(c, 0)).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
