//! The bookmark tables and the three roots, and the one-time conversion of the old flat
//! `bookmarks` table into the tree.

use std::collections::HashMap;

use rusqlite::{params, Connection};

use super::tree::{ensure_path, insert_row};
use super::validate::{clean_title, clean_url};
use super::{Kind, ROOTS, ROOT_OTHER};
use crate::storage::now_ms;

// ---------------------------------------------------------------------------------------------
// Schema and migration

/// Creates the tables and the three roots, and converts the old flat `bookmarks` table once.
/// Safe to run on every start and from two processes at the same time.
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS bookmark_nodes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            parent INTEGER,
            kind TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            url TEXT NOT NULL DEFAULT '',
            position INTEGER NOT NULL DEFAULT 0,
            created INTEGER NOT NULL,
            modified INTEGER NOT NULL,
            tags TEXT NOT NULL DEFAULT '',
            keyword TEXT NOT NULL DEFAULT '',
            spaces TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS bookmark_nodes_parent ON bookmark_nodes(parent, position);
        CREATE INDEX IF NOT EXISTS bookmark_nodes_url ON bookmark_nodes(url);",
    )?;
    let now = now_ms();
    for (i, (id, title)) in ROOTS.iter().enumerate() {
        conn.execute(
            "INSERT OR IGNORE INTO bookmark_nodes (id, parent, kind, title, position, created, modified)
             VALUES (?1, NULL, 'folder', ?2, ?3, ?4, ?4)",
            params![id, title, i as i64, now],
        )?;
    }
    migrate_legacy(conn)
}

const LEGACY_FLAG: &str = "bookmarks-tree";

fn flag_set(conn: &Connection, key: &str) -> rusqlite::Result<bool> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM meta WHERE key = ?1)", [key], |r| r.get(0))
}

fn migrate_legacy(conn: &Connection) -> rusqlite::Result<()> {
    if flag_set(conn, LEGACY_FLAG)? {
        return Ok(());
    }
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let done = (|| {
        // Another process may have converted while this one waited for the lock.
        if flag_set(conn, LEGACY_FLAG)? {
            return Ok(());
        }
        let has_legacy: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'bookmarks')",
            [],
            |r| r.get(0),
        )?;
        if has_legacy {
            convert_legacy(conn)?;
        }
        conn.execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, '1')", [LEGACY_FLAG])?;
        Ok(())
    })();
    match done {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// Every old bookmark goes under Other Bookmarks; the old "A / B" folder text becomes nested folders.
/// The old table is left in place as an implicit backup.
fn convert_legacy(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT url, title, folder, created FROM bookmarks ORDER BY folder COLLATE NOCASE, position, id",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut folders: HashMap<String, i64> = HashMap::new();
    for (url, title, folder, created) in rows {
        let parent = match folders.get(&folder) {
            Some(id) => *id,
            None => {
                let id = ensure_path(conn, ROOT_OTHER, &folder, created)?;
                folders.insert(folder, id);
                id
            }
        };
        // Same rule as every other way in: only http(s) addresses become bookmarks.
        let Ok(url) = clean_url(&url) else { continue };
        insert_row(conn, parent, Kind::Bookmark, &clean_title(&title), &url, created, created, &[], "", &[])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bookmarks::list;
    use crate::bookmarks::tests::{mem, positions_are_contiguous, titles};

    #[test]
    fn roots_exist_and_migrating_again_changes_nothing() {
        let c = mem();
        let all = list(&c).unwrap();
        assert_eq!(all.iter().map(|n| (n.id, n.title.as_str())).collect::<Vec<_>>(), [(1, "Bookmarks Toolbar"), (2, "Bookmarks Menu"), (3, "Other Bookmarks")]);
        assert!(all.iter().all(|n| n.parent.is_none() && n.kind == Kind::Folder));
        migrate(&c).unwrap();
        assert_eq!(list(&c).unwrap().len(), 3);
    }

    #[test]
    fn legacy_bookmarks_move_under_other_bookmarks_once() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE bookmarks (id INTEGER PRIMARY KEY, url TEXT NOT NULL, title TEXT NOT NULL DEFAULT '', folder TEXT NOT NULL DEFAULT '', position INTEGER NOT NULL DEFAULT 0, created INTEGER NOT NULL);
             INSERT INTO bookmarks (url, title, folder, position, created) VALUES
               ('https://b.example/', 'B', '', 1, 2000),
               ('https://a.example/', 'A', '', 0, 1000),
               ('https://c.example/', 'C', 'Chrome', 0, 3000),
               ('https://d.example/', 'D', 'Chrome / Dev', 0, 4000),
               ('https://e.example/', 'E', 'Chrome / Dev', 1, 5000),
               ('https://f.example/', 'F', 'a/b', 0, 6000);",
        )
        .unwrap();
        migrate(&c).unwrap();
        let all = list(&c).unwrap();
        let other: Vec<_> = all.iter().filter(|n| n.parent == Some(ROOT_OTHER)).collect();
        // Loose bookmarks keep their order; folders come in the order the old list sorted their names
        // (ignoring case): "a/b" before "Chrome".
        assert_eq!(other.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["A", "B", "a/b", "Chrome"]);
        assert_eq!(other[0].created, 1000);
        assert_eq!(other[0].modified, 1000);
        let chrome = other.iter().find(|n| n.title == "Chrome").unwrap();
        let dev = all.iter().find(|n| n.parent == Some(chrome.id) && n.kind == Kind::Folder).unwrap();
        assert_eq!(dev.title, "Dev");
        assert_eq!(titles(&c, dev.id), ["D", "E"]);
        assert_eq!(titles(&c, chrome.id), ["C", "Dev"]);
        positions_are_contiguous(&c);
        // Running again (a restart, or a second process) adds nothing.
        migrate(&c).unwrap();
        assert_eq!(list(&c).unwrap().len(), all.len());
        // The old table stays as it was.
        let n: i64 = c.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 6);
    }

    #[test]
    fn legacy_rows_that_are_not_web_addresses_are_left_behind() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE bookmarks (id INTEGER PRIMARY KEY, url TEXT NOT NULL, title TEXT NOT NULL DEFAULT '', folder TEXT NOT NULL DEFAULT '', position INTEGER NOT NULL DEFAULT 0, created INTEGER NOT NULL);
             INSERT INTO bookmarks (url, title, folder, position, created) VALUES
               ('javascript:alert(1)', 'bad', '', 0, 1), ('file:///C:/x', 'bad', '', 1, 1), ('https://ok.example/', 'ok', '', 2, 1);",
        )
        .unwrap();
        migrate(&c).unwrap();
        assert_eq!(titles(&c, ROOT_OTHER), ["ok"]);
    }
}
