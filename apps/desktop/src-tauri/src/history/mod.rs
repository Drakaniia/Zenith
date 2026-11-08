//! Browsing history: the queries on the `history` table (visits, search, most visited,
//! retention, imports). The table is created with the rest of the schema in `storage`.

pub mod commands;

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::storage::now_ms;

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visit: i64,
}

fn history_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: r.get(0)?,
        url: r.get(1)?,
        title: r.get(2)?,
        visit_count: r.get(3)?,
        last_visit: r.get(4)?,
    })
}

pub fn history_add_visit(conn: &mut Connection, url: &str, title: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO history (url, title, visit_count, last_visit) VALUES (?1, ?2, 1, ?3)
         ON CONFLICT(url) DO UPDATE SET visit_count = visit_count + 1, last_visit = ?3,
           title = CASE WHEN ?2 <> '' THEN ?2 ELSE title END",
        params![url, title, now_ms()],
    )?;
    Ok(())
}

pub fn history_set_title(conn: &mut Connection, url: &str, title: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE history SET title = ?2 WHERE url = ?1", params![url, title])?;
    Ok(())
}

/// Searches history. Every whitespace-separated term must match the URL or the title.
/// Ranked by "frecency": frequent and recent entries first, URL-prefix matches boosted.
pub fn history_search(
    conn: &mut Connection,
    text: &str,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<Vec<HistoryEntry>> {
    let terms: Vec<String> = text
        .split_whitespace()
        .take(6)
        .map(|t| format!("%{}%", t.replace('%', "").replace('_', "")))
        .collect();
    if terms.is_empty() {
        let mut stmt = conn.prepare_cached(
            "SELECT id, url, title, visit_count, last_visit FROM history
             ORDER BY last_visit DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], history_row)?;
        return rows.collect();
    }
    let mut sql = String::from("SELECT id, url, title, visit_count, last_visit FROM history WHERE 1=1");
    for i in 0..terms.len() {
        sql.push_str(&format!(" AND (url LIKE ?{n} OR title LIKE ?{n})", n = i + 1));
    }
    let first = terms.len() + 1;
    sql.push_str(&format!(
        " ORDER BY (CASE WHEN url LIKE ?{p1} OR url LIKE ?{p2} OR url LIKE ?{p3} THEN 0 ELSE 1 END),
           (visit_count * 3 + (last_visit / 86400000)) DESC
         LIMIT ?{l} OFFSET ?{o}",
        p1 = first,
        p2 = first + 1,
        p3 = first + 2,
        l = first + 3,
        o = first + 4
    ));
    let head = text.split_whitespace().next().unwrap_or("").to_lowercase();
    let mut values: Vec<rusqlite::types::Value> =
        terms.into_iter().map(rusqlite::types::Value::from).collect();
    values.push(format!("https://{head}%").into());
    values.push(format!("https://www.{head}%").into());
    values.push(format!("http://{head}%").into());
    values.push(limit.into());
    values.push(offset.into());
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), history_row)?;
    rows.collect()
}

/// Most visited pages (for the new-tab page).
pub fn history_top(conn: &mut Connection, limit: i64) -> rusqlite::Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, url, title, visit_count, last_visit FROM history
         ORDER BY visit_count DESC, last_visit DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], history_row)?;
    rows.collect()
}

pub fn history_delete(conn: &mut Connection, ids: &[i64]) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0;
    for id in ids {
        n += tx.execute("DELETE FROM history WHERE id = ?1", params![id])?;
    }
    tx.commit()?;
    Ok(n)
}

pub fn history_clear(conn: &mut Connection, since_ms: Option<i64>) -> rusqlite::Result<usize> {
    match since_ms {
        Some(since) => conn.execute("DELETE FROM history WHERE last_visit >= ?1", params![since]),
        None => conn.execute("DELETE FROM history", []),
    }
}

/// Applies the retention policy: drop entries older than `days`, keep at most `max_rows`.
pub fn history_prune(conn: &mut Connection, days: i64, max_rows: i64) -> rusqlite::Result<()> {
    if days > 0 {
        let cutoff = now_ms() - days * 86_400_000;
        conn.execute("DELETE FROM history WHERE last_visit < ?1", params![cutoff])?;
    }
    conn.execute(
        "DELETE FROM history WHERE id NOT IN (SELECT id FROM history ORDER BY last_visit DESC LIMIT ?1)",
        params![max_rows],
    )?;
    Ok(())
}

/// Merges history entries from another browser: a page already in history keeps the higher
/// visit count and the later visit, and gains a title if it had none.
pub fn history_import(conn: &mut Connection, entries: &[(String, String, i64, i64)]) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO history (url, title, visit_count, last_visit) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(url) DO UPDATE SET visit_count = MAX(visit_count, excluded.visit_count),
               last_visit = MAX(last_visit, excluded.last_visit),
               title = CASE WHEN title = '' THEN excluded.title ELSE title END",
        )?;
        for (url, title, visits, last) in entries {
            stmt.execute(params![url, title, visits, last])?;
        }
    }
    tx.commit()?;
    Ok(entries.len())
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
    fn history_roundtrip_and_search() {
        let mut c = mem();
        history_add_visit(&mut c, "https://example.com/", "Example Domain").unwrap();
        history_add_visit(&mut c, "https://example.com/", "").unwrap();
        history_add_visit(&mut c, "https://rust-lang.org/learn", "Learn Rust").unwrap();
        let all = history_search(&mut c, "", 10, 0).unwrap();
        assert_eq!(all.len(), 2);
        let ex = history_search(&mut c, "example", 10, 0).unwrap();
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0].visit_count, 2);
        assert_eq!(ex[0].title, "Example Domain");
        let multi = history_search(&mut c, "learn rust", 10, 0).unwrap();
        assert_eq!(multi.len(), 1);
        assert_eq!(history_search(&mut c, "learn zzz", 10, 0).unwrap().len(), 0);
        history_clear(&mut c, None).unwrap();
        assert!(history_search(&mut c, "", 10, 0).unwrap().is_empty());
    }


    #[test]
    fn history_imports_merge() {
        let mut c = mem();
        history_add_visit(&mut c, "https://a.example/", "").unwrap();
        let entries = vec![
            ("https://a.example/".to_string(), "A".to_string(), 7, 1_000),
            ("https://c.example/".to_string(), "C".to_string(), 2, 2_000),
        ];
        assert_eq!(history_import(&mut c, &entries).unwrap(), 2);
        let all = history_search(&mut c, "", 10, 0).unwrap();
        let a = all.iter().find(|e| e.url == "https://a.example/").unwrap();
        assert_eq!((a.visit_count, a.title.as_str()), (7, "A"));
        assert!(a.last_visit > 1_000, "keeps the later local visit");
        assert_eq!(all.len(), 2);
    }
}
