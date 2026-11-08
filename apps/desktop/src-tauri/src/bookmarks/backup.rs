//! JSON backups of the whole tree and restoring them, plus the backup files on disk: daily
//! automatic backups, safety copies made before a restore, their names and the date helpers.

use std::fs;
use std::path::Path;

use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};

use super::tree::{insert_one, Ctx, Tree};
use super::validate::prepare;
use super::{invalid, Kind, NewNode, Node, BACKUP_FORMAT, BACKUP_VERSION, KEEP_BEFORE_RESTORE, KEEP_DAILY, MAX_NODES, ROOT_MENU, ROOT_OTHER, ROOT_TOOLBAR};
use crate::storage::{now_ms, write_atomic};

// ---------------------------------------------------------------------------------------------
// JSON backups

#[derive(Serialize, Deserialize)]
struct BackupFile {
    format: String,
    version: u32,
    created: i64,
    roots: BackupRoots,
}

#[derive(Serialize, Deserialize)]
struct BackupRoots {
    toolbar: Vec<NewNode>,
    menu: Vec<NewNode>,
    other: Vec<NewNode>,
}

pub struct Backup {
    pub json: String,
    /// Bookmarks (not folders or separators) in it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub bookmarks: usize,
    /// Everything below the three roots.
    pub nodes: usize,
}

fn backup_node(tree: &Tree, n: &Node) -> NewNode {
    NewNode {
        kind: n.kind,
        title: n.title.clone(),
        url: n.url.clone(),
        tags: n.tags.clone(),
        keyword: n.keyword.clone(),
        spaces: n.spaces.clone(),
        children: tree.kids(n.id).map(|c| backup_node(tree, c)).collect(),
        created: Some(n.created),
        modified: Some(n.modified),
    }
}

pub fn backup_json(conn: &Connection) -> rusqlite::Result<Backup> {
    let tree = Tree::load(conn)?;
    let root = |id: i64| tree.kids(id).map(|n| backup_node(&tree, n)).collect::<Vec<_>>();
    let file = BackupFile {
        format: BACKUP_FORMAT.into(),
        version: BACKUP_VERSION,
        created: now_ms(),
        roots: BackupRoots { toolbar: root(ROOT_TOOLBAR), menu: root(ROOT_MENU), other: root(ROOT_OTHER) },
    };
    let json = serde_json::to_string(&file).map_err(|e| invalid(format!("Couldn't write the backup: {e}")))?;
    let bookmarks = tree.nodes.values().filter(|n| n.kind == Kind::Bookmark).count();
    let nodes = tree.nodes.values().filter(|n| n.parent.is_some()).count();
    Ok(Backup { json, bookmarks, nodes })
}

fn parse_backup(json: &str) -> rusqlite::Result<BackupFile> {
    let file: BackupFile = serde_json::from_str(json).map_err(|_| invalid("That isn't a Zenith bookmarks backup."))?;
    if file.format != BACKUP_FORMAT {
        return Err(invalid("That isn't a Zenith bookmarks backup."));
    }
    if file.version > BACKUP_VERSION {
        return Err(invalid("That backup was made by a newer version of Zenith."));
    }
    Ok(file)
}

fn count_marks(nodes: &[NewNode]) -> usize {
    nodes.iter().map(|n| usize::from(n.kind == Kind::Bookmark) + count_marks(&n.children)).sum()
}

/// Replaces every bookmark with a backup's. The file is checked completely first, so a bad one
/// changes nothing. With `safety_dir`, the current bookmarks are saved there as a
/// `-before-restore` backup before they are replaced. Returns the number of bookmarks restored.
pub fn restore(conn: &mut Connection, safety_dir: Option<&Path>, json: &str, now: i64) -> rusqlite::Result<usize> {
    let file = parse_backup(json)?;
    let mut budget = MAX_NODES;
    let mut roots = Vec::new();
    for (root, nodes) in [(ROOT_TOOLBAR, &file.roots.toolbar), (ROOT_MENU, &file.roots.menu), (ROOT_OTHER, &file.roots.other)] {
        let prepared = nodes.iter().map(|n| prepare(n, 0, &mut budget)).collect::<rusqlite::Result<Vec<_>>>()?;
        roots.push((root, prepared));
    }
    // The lock is taken first, so the safety copy holds exactly what is replaced: nothing another
    // window adds in between is lost from both.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(dir) = safety_dir {
        let current = backup_json(&tx)?;
        if current.nodes > 0 {
            let name = next_before_restore_name(dir, &date_string(now));
            write_backup_file(dir, &name, &current.json)
                .map_err(|e| invalid(format!("Couldn't save a copy of your current bookmarks first: {e}")))?;
            prune_backups(dir);
        }
    }
    tx.execute("DELETE FROM bookmark_nodes WHERE parent IS NOT NULL", [])?;
    let mut ctx = Ctx { now, steal_keywords: false, merge: false, added: 0 };
    for (root, nodes) in &roots {
        for p in nodes {
            insert_one(&tx, *root, p, &mut ctx, true)?;
        }
    }
    tx.commit()?;
    Ok(ctx.added)
}

// ---------------------------------------------------------------------------------------------
// Backup files

/// Days since 1970-01-01 as (year, month, day), in the proleptic Gregorian calendar.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
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

#[derive(Debug, PartialEq, Eq)]
enum BackupName {
    /// `bookmarks-YYYY-MM-DD.json`: the daily one.
    Daily(String),
    /// `bookmarks-YYYY-MM-DD-before-restore[-N].json`: a copy made before a restore.
    BeforeRestore(String, u32),
}

fn parse_backup_name(name: &str) -> Option<BackupName> {
    let body = name.strip_prefix("bookmarks-")?.strip_suffix(".json")?;
    let date = body.get(..10)?;
    let shape_ok = date.chars().enumerate().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() });
    if !shape_ok {
        return None;
    }
    match &body[10..] {
        "" => Some(BackupName::Daily(date.to_string())),
        "-before-restore" => Some(BackupName::BeforeRestore(date.to_string(), 1)),
        tail => {
            let n = tail.strip_prefix("-before-restore-")?;
            if n.is_empty() || n.len() > 2 || !n.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let n: u32 = n.parse().ok()?;
            (n >= 2).then(|| BackupName::BeforeRestore(date.to_string(), n))
        }
    }
}

/// True for the file names this module writes (and only those may be restored by name).
pub fn valid_backup_name(name: &str) -> bool {
    parse_backup_name(name).is_some()
}

pub fn daily_backup_name(date: &str) -> String {
    format!("bookmarks-{date}.json")
}

fn next_before_restore_name(dir: &Path, date: &str) -> String {
    let name = |n: u32| if n == 1 { format!("bookmarks-{date}-before-restore.json") } else { format!("bookmarks-{date}-before-restore-{n}.json") };
    (1..=99).map(name).find(|n| !dir.join(n).exists()).unwrap_or_else(|| name(99))
}

fn write_backup_file(dir: &Path, name: &str, json: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    write_atomic(&dir.join(name), json.as_bytes())
}

/// Keeps the newest daily backups and safety copies and deletes the rest.
fn prune_backups(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut daily: Vec<(String, String)> = Vec::new();
    let mut before: Vec<((String, u32), String)> = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        match parse_backup_name(&name) {
            Some(BackupName::Daily(date)) => daily.push((date, name)),
            Some(BackupName::BeforeRestore(date, n)) => before.push(((date, n), name)),
            None => {}
        }
    }
    daily.sort_by(|a, b| b.cmp(a));
    before.sort_by(|a, b| b.cmp(a));
    let old = daily.into_iter().skip(KEEP_DAILY).map(|(_, n)| n).chain(before.into_iter().skip(KEEP_BEFORE_RESTORE).map(|(_, n)| n));
    for name in old {
        let _ = fs::remove_file(dir.join(name));
    }
}

/// Writes today's backup when there isn't one yet and there is something to save.
pub fn auto_backup(conn: &Connection, dir: &Path, now: i64) -> rusqlite::Result<()> {
    let name = daily_backup_name(&date_string(now));
    if dir.join(&name).exists() {
        return Ok(());
    }
    let backup = backup_json(conn)?;
    if backup.nodes == 0 {
        return Ok(());
    }
    write_backup_file(dir, &name, &backup.json).map_err(|e| invalid(format!("Couldn't write the bookmarks backup: {e}")))?;
    prune_backups(dir);
    Ok(())
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    /// When the file was written (milliseconds).
    pub created: i64,
    /// Bookmarks in it.
    pub count: usize,
}

/// The backups in the folder, newest first.
pub fn list_backups(dir: &Path) -> Vec<BackupInfo> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !valid_backup_name(&name) {
            continue;
        }
        let created = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let count = fs::read_to_string(e.path())
            .ok()
            .and_then(|s| serde_json::from_str::<BackupFile>(&s).ok())
            .map(|f| count_marks(&f.roots.toolbar) + count_marks(&f.roots.menu) + count_marks(&f.roots.other))
            .unwrap_or(0);
        out.push(BackupInfo { name, created, count });
    }
    out.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.name.cmp(&a.name)));
    out
}

/// The text of a backup file in the folder, by name (never by path).
pub fn read_backup(dir: &Path, name: &str) -> Result<String, String> {
    if !valid_backup_name(name) {
        return Err("That isn't one of Zenith's bookmark backups.".into());
    }
    fs::read_to_string(dir.join(name)).map_err(|_| "That backup could not be read.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bookmarks::{add, list};
    use crate::bookmarks::tests::{mark, mem, positions_are_contiguous, sample_tree, shape, titles};

    #[test]
    fn backups_restore_everything_and_replace_what_was_there() {
        let mut a = mem();
        sample_tree(&mut a);
        let backup = backup_json(&a).unwrap();
        assert_eq!(backup.bookmarks, 5);
        assert_eq!(backup.nodes, 9);
        let before = shape(&a);
        let mut b = mem();
        add(&mut b, ROOT_TOOLBAR, None, &[mark("old", "https://old.example/")]).unwrap();
        let n = restore(&mut b, None, &backup.json, 1_700_000_000_000).unwrap();
        assert_eq!(n, 5);
        assert_eq!(shape(&b), before, "the old bookmarks are gone, the backup's are back");
        let news = list(&b).unwrap().into_iter().find(|n| n.keyword == "n").unwrap();
        assert_eq!(news.tags, ["news", "daily"]);
        assert_eq!(list(&b).unwrap().iter().filter(|n| n.parent.is_none()).map(|n| n.id).collect::<Vec<_>>(), [1, 2, 3]);
        positions_are_contiguous(&b);
    }

    #[test]
    fn a_bad_backup_leaves_the_bookmarks_alone() {
        let mut c = mem();
        sample_tree(&mut c);
        let before = shape(&c);
        for bad in [
            "not json",
            "{}",
            r#"{"format":"something-else","version":1,"created":0,"roots":{"toolbar":[],"menu":[],"other":[]}}"#,
            r#"{"format":"zenith-bookmarks","version":9,"created":0,"roots":{"toolbar":[],"menu":[],"other":[]}}"#,
            r#"{"format":"zenith-bookmarks","version":1,"created":0,"roots":{"toolbar":[{"kind":"bookmark","title":"x","url":"ftp://nope"}],"menu":[],"other":[]}}"#,
            r#"{"format":"zenith-bookmarks","version":1,"created":0,"roots":{"toolbar":[],"menu":[{"kind":"nonsense"}],"other":[]}}"#,
        ] {
            assert!(restore(&mut c, None, bad, 0).is_err(), "{bad}");
            assert_eq!(shape(&c), before);
        }
    }

    #[test]
    fn restoring_saves_a_safety_copy_first() {
        let dir = std::env::temp_dir().join(format!("zenith-bm-test-{}-{}", std::process::id(), now_ms()));
        let mut c = mem();
        sample_tree(&mut c);
        let json = backup_json(&c).unwrap().json;
        add(&mut c, ROOT_OTHER, None, &[mark("extra", "https://extra.example/")]).unwrap();
        let now = 1_700_000_000_000;
        restore(&mut c, Some(&dir), &json, now).unwrap();
        let name = format!("bookmarks-{}-before-restore.json", date_string(now));
        let saved = fs::read_to_string(dir.join(&name)).unwrap();
        assert!(saved.contains("extra.example"), "the copy holds what was replaced");
        assert!(!titles(&c, ROOT_OTHER).contains(&"extra".to_string()));
        // A second restore the same day doesn't overwrite the first copy.
        restore(&mut c, Some(&dir), &json, now).unwrap();
        assert!(dir.join(&name).exists());
        assert!(dir.join(format!("bookmarks-{}-before-restore-2.json", date_string(now))).exists());
        // An invalid file makes no copy at all.
        assert!(restore(&mut c, Some(&dir), "nope", now).is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn backup_names_are_checked() {
        for ok in ["bookmarks-2026-11-07.json", "bookmarks-2026-11-07-before-restore.json", "bookmarks-2026-11-07-before-restore-2.json", "bookmarks-2026-11-07-before-restore-99.json"] {
            assert!(valid_backup_name(ok), "{ok}");
        }
        for bad in [
            "",
            "bookmarks.json",
            "bookmarks-2026-11-07.txt",
            "../bookmarks-2026-11-07.json",
            "..\\bookmarks-2026-11-07.json",
            "dir/bookmarks-2026-11-07.json",
            "bookmarks-2026-1-07.json",
            "bookmarks-2026-11-07-before-restore-1.json",
            "bookmarks-2026-11-07-before-restore-100.json",
            "bookmarks-2026-11-07-before-restore-.json",
            "bookmarks-2026-11-07-extra.json",
            "bookmarks-20x6-11-07.json",
            "bookmarks-2026-11-07.json.exe",
            "Bookmarks-2026-11-07.json",
        ] {
            assert!(!valid_backup_name(bad), "{bad}");
        }
        assert_eq!(parse_backup_name("bookmarks-2026-11-07-before-restore-3.json"), Some(BackupName::BeforeRestore("2026-11-07".into(), 3)));
        assert!(read_backup(Path::new("."), "../Cargo.toml").is_err());
    }

    #[test]
    fn dates_are_computed_without_a_calendar_crate() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(date_string(1_700_000_000_000), "2023-11-14");
        assert_eq!(date_string(86_399_999), "1970-01-01");
        assert_eq!(date_string(86_400_000), "1970-01-02");
    }

    #[test]
    fn daily_backups_are_written_once_and_pruned() {
        let dir = std::env::temp_dir().join(format!("zenith-bm-daily-{}-{}", std::process::id(), now_ms()));
        let mut c = mem();
        // Nothing to save yet.
        auto_backup(&c, &dir, 1_700_000_000_000).unwrap();
        assert!(!dir.exists() || fs::read_dir(&dir).unwrap().count() == 0);
        sample_tree(&mut c);
        let day = 86_400_000;
        for i in 0..14 {
            auto_backup(&c, &dir, 1_700_000_000_000 + i * day).unwrap();
        }
        let names: Vec<_> = list_backups(&dir).into_iter().map(|b| b.name).collect();
        assert_eq!(names.len(), KEEP_DAILY, "only the newest ten stay");
        assert!(names.contains(&format!("bookmarks-{}.json", date_string(1_700_000_000_000 + 13 * day))));
        assert!(!names.contains(&format!("bookmarks-{}.json", date_string(1_700_000_000_000))));
        // The same day again does nothing, even after a change.
        let before = fs::read_to_string(dir.join(format!("bookmarks-{}.json", date_string(1_700_000_000_000 + 13 * day)))).unwrap();
        add(&mut c, ROOT_OTHER, None, &[mark("later", "https://later.example/")]).unwrap();
        auto_backup(&c, &dir, 1_700_000_000_000 + 13 * day + 1000).unwrap();
        let after = fs::read_to_string(dir.join(format!("bookmarks-{}.json", date_string(1_700_000_000_000 + 13 * day)))).unwrap();
        assert_eq!(before, after);
        let info = list_backups(&dir);
        assert_eq!(info[0].count, 5);
        // Safety copies are pruned on their own.
        for i in 1..=8 {
            let name = next_before_restore_name(&dir, "2030-01-01");
            write_backup_file(&dir, &name, "{}").unwrap();
            assert!(valid_backup_name(&name), "{i}: {name}");
        }
        prune_backups(&dir);
        let all = list_backups(&dir);
        assert_eq!(all.iter().filter(|b| b.name.contains("before-restore")).count(), KEEP_BEFORE_RESTORE);
        assert_eq!(all.iter().filter(|b| !b.name.contains("before-restore")).count(), KEEP_DAILY);
        assert_eq!(read_backup(&dir, &all[0].name).is_ok(), true);
        let _ = fs::remove_dir_all(dir);
    }
}
