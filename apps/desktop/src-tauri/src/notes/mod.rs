//! Notes (the built-in notes editor, zenith://notes).

pub mod commands;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::storage::now_ms;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// Made by the UI, so a new note can be shown and edited before it is saved.
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub updated: i64,
}

/// Notes, most recently edited first.
pub fn notes_list(conn: &mut Connection) -> rusqlite::Result<Vec<Note>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, title, body, created, updated FROM notes ORDER BY updated DESC, created DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Note {
            id: r.get(0)?,
            title: r.get(1)?,
            body: r.get(2)?,
            created: r.get(3)?,
            updated: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// Inserts or updates a note. A save older than the stored copy (another window wrote a newer
/// edit in the meantime) is ignored. Returns whether the note was written.
pub fn note_save(conn: &mut Connection, n: &Note) -> rusqlite::Result<bool> {
    let now = now_ms();
    let created = if n.created > 0 { n.created } else { now };
    let updated = if n.updated > 0 { n.updated } else { now };
    let changed = conn.execute(
        "INSERT INTO notes (id, title, body, created, updated) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET title = excluded.title, body = excluded.body,
           updated = excluded.updated
         WHERE excluded.updated >= notes.updated",
        params![n.id, n.title, n.body, created, updated],
    )?;
    Ok(changed > 0)
}

pub fn note_delete(conn: &mut Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::storage::migrate(&c).unwrap();
        c
    }

    #[test]
    fn notes_save_list_delete() {
        let mut c = mem();
        let note = |id: &str, body: &str, updated: i64| Note {
            id: id.into(),
            title: format!("Title {id}"),
            body: body.into(),
            created: 1,
            updated,
        };
        assert!(note_save(&mut c, &note("a", "first", 10)).unwrap());
        assert!(note_save(&mut c, &note("b", "second", 20)).unwrap());
        let list = notes_list(&mut c).unwrap();
        assert_eq!(list.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);

        // A newer edit wins; a stale one (from another window) is ignored.
        assert!(note_save(&mut c, &note("a", "edited", 30)).unwrap());
        assert!(!note_save(&mut c, &note("a", "stale", 15)).unwrap());
        let list = notes_list(&mut c).unwrap();
        assert_eq!(list[0].id, "a");
        assert_eq!(list[0].body, "edited");
        assert_eq!(list[0].created, 1, "keeps the creation time");

        // Text round-trips unchanged, including quotes and newlines.
        let tricky = Note { body: "line 1\n'quoted' \"text\" ; DROP".into(), ..note("c", "", 40) };
        note_save(&mut c, &tricky).unwrap();
        assert_eq!(notes_list(&mut c).unwrap()[0], tricky);

        note_delete(&mut c, "a").unwrap();
        assert_eq!(notes_list(&mut c).unwrap().len(), 2);
    }
}
