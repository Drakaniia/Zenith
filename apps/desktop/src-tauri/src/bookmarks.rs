//! The bookmark tree: Firefox's Places model, as Zen shows it. Three built-in folders are the
//! roots (the Bookmarks Toolbar, the Bookmarks Menu and Other Bookmarks); below them come
//! folders, bookmarks and separators. A bookmark can carry tags, a keyword (typed in the address
//! bar to open it) and the workspaces it belongs to.
//!
//! Everything here is plain functions over a rusqlite connection, run on the database worker
//! thread through `Db::call`, so they are tested against in-memory databases. The Tauri commands
//! that call them live in commands.rs.
//!
//! Mistakes the user can fix (a bad address, a move into the folder's own subfolder) come back as
//! `rusqlite::Error::ToSqlConversionFailure`, which displays as the message alone, so they pass
//! through `Db::call` and reach the UI as readable text.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::storage::{now_ms, write_atomic, Bookmark as FlatBookmark};

pub const ROOT_TOOLBAR: i64 = 1;
pub const ROOT_MENU: i64 = 2;
pub const ROOT_OTHER: i64 = 3;
const ROOTS: [(i64, &str); 3] = [(ROOT_TOOLBAR, "Bookmarks Toolbar"), (ROOT_MENU, "Bookmarks Menu"), (ROOT_OTHER, "Other Bookmarks")];

const MAX_URL: usize = 8192;
const MAX_TITLE: usize = 1024;
const MAX_TAGS: usize = 50;
const MAX_TAG: usize = 64;
const MAX_KEYWORD: usize = 64;
const MAX_SPACES: usize = 64;
const MAX_SPACE_ID: usize = 64;
const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 200_000;
const BACKUP_FORMAT: &str = "zenith-bookmarks";
const BACKUP_VERSION: u32 = 1;
/// Automatic backups kept, and safety copies made before a restore.
const KEEP_DAILY: usize = 10;
const KEEP_BEFORE_RESTORE: usize = 5;

// ---------------------------------------------------------------------------------------------
// Errors

#[derive(Debug)]
struct Invalid(String);

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Invalid {}

fn invalid(msg: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(Invalid(msg.into())))
}

// ---------------------------------------------------------------------------------------------
// Model

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bookmark,
    Folder,
    Separator,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Bookmark => "bookmark",
            Kind::Folder => "folder",
            Kind::Separator => "separator",
        }
    }

    fn parse(s: &str) -> Kind {
        match s {
            "folder" => Kind::Folder,
            "separator" => Kind::Separator,
            _ => Kind::Bookmark,
        }
    }
}

/// One node of the tree, as the UI receives it.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: i64,
    /// `None` only for the three roots.
    pub parent: Option<i64>,
    pub kind: Kind,
    pub title: String,
    pub url: String,
    pub position: i64,
    pub created: i64,
    pub modified: i64,
    pub tags: Vec<String>,
    pub keyword: String,
    pub spaces: Vec<String>,
}

/// A node to create. Folders may carry children. This is also the node of the JSON backup, which
/// is why the dates are optional (new nodes get the current time).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewNode {
    pub kind: Kind,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub keyword: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NewNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
}

/// Fields to change; omitted ones stay as they are.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct NodePatch {
    pub title: Option<String>,
    pub url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub keyword: Option<String>,
    pub spaces: Option<Vec<String>>,
}

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

// ---------------------------------------------------------------------------------------------
// Validation and cleaning

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn clean_title(t: &str) -> String {
    truncate(t.trim(), MAX_TITLE)
}

fn is_http(url: &str) -> bool {
    let lower: String = url.chars().take(8).collect::<String>().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn clean_url(url: &str) -> rusqlite::Result<String> {
    let url = url.trim();
    if !is_http(url) {
        return Err(invalid("Bookmarks need an http(s) address."));
    }
    if url.chars().count() > MAX_URL || url.chars().any(char::is_control) {
        return Err(invalid("That address isn't valid."));
    }
    Ok(url.to_string())
}

/// Trimmed, no empty ones, duplicates (ignoring case) dropped, limits applied.
fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in tags {
        let tag = truncate(&raw.replace(',', " ").split_whitespace().collect::<Vec<_>>().join(" "), MAX_TAG);
        if tag.is_empty() || out.iter().any(|t| t.to_lowercase() == tag.to_lowercase()) {
            continue;
        }
        out.push(tag);
        if out.len() >= MAX_TAGS {
            break;
        }
    }
    out
}

fn clean_keyword(k: &str) -> rusqlite::Result<String> {
    let k = k.trim();
    if k.chars().any(char::is_whitespace) {
        return Err(invalid("A keyword can't contain spaces."));
    }
    if k.chars().count() > MAX_KEYWORD {
        return Err(invalid("That keyword is too long."));
    }
    Ok(k.to_lowercase())
}

fn clean_spaces(spaces: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in spaces {
        let s = truncate(s.trim(), MAX_SPACE_ID);
        if s.is_empty() || s.contains(',') || out.contains(&s) {
            continue;
        }
        out.push(s);
        if out.len() >= MAX_SPACES {
            break;
        }
    }
    out
}

fn split_list(s: &str) -> Vec<String> {
    s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect()
}

/// A request after validation, ready to insert.
struct Prepared {
    kind: Kind,
    title: String,
    url: String,
    tags: Vec<String>,
    keyword: String,
    spaces: Vec<String>,
    created: Option<i64>,
    modified: Option<i64>,
    children: Vec<Prepared>,
}

fn prepare(n: &NewNode, depth: usize, budget: &mut usize) -> rusqlite::Result<Prepared> {
    if depth > MAX_DEPTH {
        return Err(invalid("Those folders are nested too deeply."));
    }
    if *budget == 0 {
        return Err(invalid("That is too many bookmarks at once."));
    }
    *budget -= 1;
    let (url, tags, keyword) = match n.kind {
        Kind::Bookmark => (clean_url(&n.url)?, clean_tags(&n.tags), clean_keyword(&n.keyword)?),
        _ => {
            if !n.url.trim().is_empty() {
                return Err(invalid("Only bookmarks have an address."));
            }
            (String::new(), Vec::new(), String::new())
        }
    };
    if n.kind != Kind::Folder && !n.children.is_empty() {
        return Err(invalid("Only folders can contain bookmarks."));
    }
    let children = n.children.iter().map(|c| prepare(c, depth + 1, budget)).collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Prepared {
        kind: n.kind,
        title: if n.kind == Kind::Separator { String::new() } else { clean_title(&n.title) },
        url,
        tags,
        keyword,
        spaces: if n.kind == Kind::Separator { Vec::new() } else { clean_spaces(&n.spaces) },
        created: n.created,
        modified: n.modified,
        children,
    })
}

// ---------------------------------------------------------------------------------------------
// Reading

const COLUMNS: &str = "id, parent, kind, title, url, position, created, modified, tags, keyword, spaces";

fn row_to_node(r: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    Ok(Node {
        id: r.get(0)?,
        parent: r.get(1)?,
        kind: Kind::parse(&r.get::<_, String>(2)?),
        title: r.get(3)?,
        url: r.get(4)?,
        position: r.get(5)?,
        created: r.get(6)?,
        modified: r.get(7)?,
        tags: split_list(&r.get::<_, String>(8)?),
        keyword: r.get(9)?,
        spaces: split_list(&r.get::<_, String>(10)?),
    })
}

/// Every node: the roots first, then by parent and position.
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<Node>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM bookmark_nodes ORDER BY parent IS NOT NULL, parent, position, id"
    ))?;
    let rows = stmt.query_map([], row_to_node)?;
    rows.collect()
}

fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<Node>> {
    conn.query_row(&format!("SELECT {COLUMNS} FROM bookmark_nodes WHERE id = ?1"), [id], row_to_node).optional()
}

fn children_ids(conn: &Connection, parent: i64) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached("SELECT id FROM bookmark_nodes WHERE parent = ?1 ORDER BY position, id")?;
    let rows = stmt.query_map([parent], |r| r.get(0))?;
    rows.collect()
}

fn next_position(conn: &Connection, parent: i64) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM bookmark_nodes WHERE parent = ?1", [parent], |r| r.get(0))
}

fn is_root(id: i64) -> bool {
    ROOTS.iter().any(|(r, _)| *r == id)
}

fn require_folder(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    let kind: Option<String> = conn.query_row("SELECT kind FROM bookmark_nodes WHERE id = ?1", [id], |r| r.get(0)).optional()?;
    match kind.as_deref() {
        Some("folder") => Ok(()),
        Some(_) => Err(invalid("Bookmarks can only go into folders.")),
        None => Err(invalid("That folder no longer exists.")),
    }
}

/// True when `node` is `ancestor` or lies below it.
fn is_inside(conn: &Connection, mut node: i64, ancestor: i64) -> rusqlite::Result<bool> {
    for _ in 0..=MAX_DEPTH * 4 {
        if node == ancestor {
            return Ok(true);
        }
        let up: Option<Option<i64>> = conn.query_row("SELECT parent FROM bookmark_nodes WHERE id = ?1", [node], |r| r.get(0)).optional()?;
        match up {
            Some(Some(p)) => node = p,
            _ => return Ok(false),
        }
    }
    // Nested deeper than anything the app makes: refuse rather than guess "not inside".
    Err(invalid("Those folders are nested too deeply."))
}

// ---------------------------------------------------------------------------------------------
// Writing

/// Stores the children's order and parent: position `i` for `ids[i]`.
fn write_order(conn: &Connection, parent: i64, ids: &[i64]) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare_cached("UPDATE bookmark_nodes SET parent = ?1, position = ?2 WHERE id = ?3")?;
    for (i, id) in ids.iter().enumerate() {
        stmt.execute(params![parent, i as i64, id])?;
    }
    Ok(())
}

fn repack(conn: &Connection, parent: i64) -> rusqlite::Result<()> {
    let ids = children_ids(conn, parent)?;
    write_order(conn, parent, &ids)
}

#[allow(clippy::too_many_arguments)]
fn insert_row(
    conn: &Connection,
    parent: i64,
    kind: Kind,
    title: &str,
    url: &str,
    created: i64,
    modified: i64,
    tags: &[String],
    keyword: &str,
    spaces: &[String],
) -> rusqlite::Result<i64> {
    let position = next_position(conn, parent)?;
    conn.execute(
        "INSERT INTO bookmark_nodes (parent, kind, title, url, position, created, modified, tags, keyword, spaces)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![parent, kind.as_str(), title, url, position, created, modified, tags.join(","), keyword, spaces.join(",")],
    )?;
    Ok(conn.last_insert_rowid())
}

fn find_folder(conn: &Connection, parent: i64, title: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM bookmark_nodes WHERE parent = ?1 AND kind = 'folder' AND title = ?2 ORDER BY position, id LIMIT 1",
        params![parent, title],
        |r| r.get(0),
    )
    .optional()
}

/// The folder at the path below `root` ("A / B"), created as needed.
fn ensure_path(conn: &Connection, root: i64, path: &str, now: i64) -> rusqlite::Result<i64> {
    let mut parent = root;
    for part in path.split(" / ").map(str::trim).filter(|p| !p.is_empty()) {
        let name = clean_title(part);
        parent = match find_folder(conn, parent, &name)? {
            Some(id) => id,
            None => insert_row(conn, parent, Kind::Folder, &name, "", now, now, &[], "", &[])?,
        };
    }
    Ok(parent)
}

struct Ctx {
    now: i64,
    /// A keyword set on a new bookmark is taken from whoever had it (the user chose it just now);
    /// imports and restores keep what is there.
    steal_keywords: bool,
    /// Importing into existing bookmarks: reuse same-named folders, skip bookmarks already there
    /// (and separators in a folder that already has content).
    merge: bool,
    /// New bookmarks (not folders or separators).
    added: usize,
}

fn keyword_taken(conn: &Connection, keyword: &str) -> rusqlite::Result<bool> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM bookmark_nodes WHERE keyword = ?1)", [keyword], |r| r.get(0))
}

/// Inserts a prepared node (and its children) at the end of `parent`. `None`: skipped (merge mode).
fn insert_one(conn: &Connection, parent: i64, p: &Prepared, ctx: &mut Ctx, fresh_parent: bool) -> rusqlite::Result<Option<i64>> {
    if ctx.merge {
        match p.kind {
            // A re-import must not stack up separators in a folder that already has content.
            Kind::Separator if !fresh_parent => return Ok(None),
            Kind::Bookmark => {
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM bookmark_nodes WHERE parent = ?1 AND kind = 'bookmark' AND url = ?2)",
                    params![parent, p.url],
                    |r| r.get(0),
                )?;
                if exists {
                    return Ok(None);
                }
            }
            Kind::Folder => {
                if let Some(id) = find_folder(conn, parent, &p.title)? {
                    let fresh = children_ids(conn, id)?.is_empty();
                    for c in &p.children {
                        insert_one(conn, id, c, ctx, fresh)?;
                    }
                    return Ok(Some(id));
                }
            }
            Kind::Separator => {}
        }
    }
    let created = p.created.unwrap_or(ctx.now);
    let modified = p.modified.unwrap_or(created);
    let keyword = if p.kind == Kind::Bookmark && !p.keyword.is_empty() {
        if ctx.steal_keywords {
            conn.execute("UPDATE bookmark_nodes SET keyword = '' WHERE keyword = ?1", [&p.keyword])?;
            p.keyword.clone()
        } else if keyword_taken(conn, &p.keyword)? {
            String::new()
        } else {
            p.keyword.clone()
        }
    } else {
        String::new()
    };
    let id = insert_row(conn, parent, p.kind, &p.title, &p.url, created, modified, &p.tags, &keyword, &p.spaces)?;
    if p.kind == Kind::Bookmark {
        ctx.added += 1;
    }
    for c in &p.children {
        insert_one(conn, id, c, ctx, true)?;
    }
    Ok(Some(id))
}

/// Adds nodes under `parent` at `index` (`None` or out of range: at the end). Returns the ids of
/// the new top-level nodes in order. Interactive adds don't de-duplicate: a page can be bookmarked
/// twice.
pub fn add(conn: &mut Connection, parent: i64, index: Option<i64>, nodes: &[NewNode]) -> rusqlite::Result<Vec<i64>> {
    let mut budget = MAX_NODES;
    let prepared = nodes.iter().map(|n| prepare(n, 0, &mut budget)).collect::<rusqlite::Result<Vec<_>>>()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_folder(&tx, parent)?;
    let mut ctx = Ctx { now: now_ms(), steal_keywords: true, merge: false, added: 0 };
    let mut ids = Vec::with_capacity(prepared.len());
    for p in &prepared {
        if let Some(id) = insert_one(&tx, parent, p, &mut ctx, true)? {
            ids.push(id);
        }
    }
    if let Some(index) = index {
        let mut order = children_ids(&tx, parent)?;
        order.retain(|id| !ids.contains(id));
        let at = index.clamp(0, order.len() as i64) as usize;
        order.splice(at..at, ids.iter().copied());
        write_order(&tx, parent, &order)?;
    }
    tx.commit()?;
    Ok(ids)
}

pub fn update(conn: &mut Connection, id: i64, patch: &NodePatch) -> rusqlite::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let node = get(&tx, id)?.ok_or_else(|| invalid("That bookmark no longer exists."))?;
    let root = node.parent.is_none();
    let mark = node.kind == Kind::Bookmark;
    let title = match &patch.title {
        Some(t) if !root => clean_title(t),
        _ => node.title.clone(),
    };
    let url = match &patch.url {
        Some(u) if mark => clean_url(u)?,
        _ => node.url.clone(),
    };
    let tags = match &patch.tags {
        Some(t) if mark => clean_tags(t),
        _ => node.tags.clone(),
    };
    let keyword = match &patch.keyword {
        Some(k) if mark => clean_keyword(k)?,
        _ => node.keyword.clone(),
    };
    let spaces = match &patch.spaces {
        Some(s) if !root && node.kind != Kind::Separator => clean_spaces(s),
        _ => node.spaces.clone(),
    };
    if !keyword.is_empty() {
        tx.execute("UPDATE bookmark_nodes SET keyword = '' WHERE keyword = ?1 AND id <> ?2", params![keyword, id])?;
    }
    tx.execute(
        "UPDATE bookmark_nodes SET title = ?2, url = ?3, tags = ?4, keyword = ?5, spaces = ?6, modified = ?7 WHERE id = ?1",
        params![id, title, url, tags.join(","), keyword, spaces.join(","), now_ms()],
    )?;
    tx.commit()
}

/// Moves a node. `index` counts the destination's children after the node has left its old place.
pub fn move_node(conn: &mut Connection, id: i64, parent: i64, index: i64) -> rusqlite::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let node = get(&tx, id)?.ok_or_else(|| invalid("That bookmark no longer exists."))?;
    let Some(old_parent) = node.parent else {
        return Err(invalid("The built-in folders can't be moved."));
    };
    require_folder(&tx, parent)?;
    if is_inside(&tx, parent, id)? {
        return Err(invalid("A folder can't be moved into itself."));
    }
    let mut order = children_ids(&tx, parent)?;
    order.retain(|x| *x != id);
    let at = index.clamp(0, order.len() as i64) as usize;
    order.insert(at, id);
    write_order(&tx, parent, &order)?;
    if old_parent != parent {
        repack(&tx, old_parent)?;
    }
    tx.execute("UPDATE bookmark_nodes SET modified = ?2 WHERE id = ?1", params![id, now_ms()])?;
    tx.commit()
}

/// Sets the order of a folder's children; `order` must name each of them exactly once.
pub fn reorder(conn: &mut Connection, parent: i64, order: &[i64]) -> rusqlite::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_folder(&tx, parent)?;
    let mut current = children_ids(&tx, parent)?;
    let mut wanted = order.to_vec();
    current.sort_unstable();
    wanted.sort_unstable();
    if current != wanted {
        return Err(invalid("The folder changed while it was being sorted."));
    }
    write_order(&tx, parent, order)?;
    tx.commit()
}

fn delete_subtree(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "WITH RECURSIVE sub(id) AS (SELECT ?1 UNION ALL SELECT n.id FROM bookmark_nodes n JOIN sub ON n.parent = sub.id)
         DELETE FROM bookmark_nodes WHERE id IN (SELECT id FROM sub)",
        [id],
    )?;
    Ok(())
}

/// Deletes nodes (folders with everything in them). Nodes that are already gone are ignored.
pub fn remove(conn: &mut Connection, ids: &[i64]) -> rusqlite::Result<()> {
    if ids.iter().any(|id| is_root(*id)) {
        return Err(invalid("The built-in folders can't be deleted."));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut touched = Vec::new();
    for id in ids {
        let Some(node) = get(&tx, *id)? else { continue };
        delete_subtree(&tx, *id)?;
        if let Some(p) = node.parent {
            if !touched.contains(&p) {
                touched.push(p);
            }
        }
    }
    for p in touched {
        // The parent itself may have gone with an earlier id.
        if get(&tx, p)?.is_some() {
            repack(&tx, p)?;
        }
    }
    tx.commit()
}

/// Deletes every bookmark with exactly this address.
pub fn remove_url(conn: &mut Connection, url: &str) -> rusqlite::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let found: Vec<(i64, i64)> = {
        let mut stmt = tx.prepare("SELECT id, parent FROM bookmark_nodes WHERE kind = 'bookmark' AND url = ?1")?;
        let rows = stmt.query_map([url], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut parents = Vec::new();
    for (id, parent) in found {
        tx.execute("DELETE FROM bookmark_nodes WHERE id = ?1", [id])?;
        if !parents.contains(&parent) {
            parents.push(parent);
        }
    }
    for p in parents {
        repack(&tx, p)?;
    }
    tx.commit()
}

/// Imports other browsers' bookmarks, which arrive as flat records with a folder path ("Chrome /
/// Dev"): the folders are created under the Bookmarks Toolbar and a bookmark already in its folder
/// is skipped, so importing twice adds nothing. Returns how many are new.
pub fn import_paths(conn: &mut Connection, items: &[FlatBookmark]) -> rusqlite::Result<usize> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = now_ms();
    let mut folders: HashMap<&str, i64> = HashMap::new();
    let mut added = 0;
    for b in items {
        let Ok(url) = clean_url(&b.url) else { continue };
        let parent = match folders.get(b.folder.as_str()) {
            Some(id) => *id,
            None => {
                let id = ensure_path(&tx, ROOT_TOOLBAR, &b.folder, now)?;
                folders.insert(b.folder.as_str(), id);
                id
            }
        };
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM bookmark_nodes WHERE parent = ?1 AND kind = 'bookmark' AND url = ?2)",
            params![parent, url],
            |r| r.get(0),
        )?;
        if exists {
            continue;
        }
        let created = if b.created > 0 { b.created } else { now };
        insert_row(&tx, parent, Kind::Bookmark, &clean_title(&b.title), &url, created, created, &[], "", &[])?;
        added += 1;
    }
    tx.commit()?;
    Ok(added)
}

// ---------------------------------------------------------------------------------------------
// The tree in memory (export and backup)

struct Tree {
    nodes: HashMap<i64, Node>,
    children: HashMap<i64, Vec<i64>>,
}

impl Tree {
    fn load(conn: &Connection) -> rusqlite::Result<Tree> {
        let mut nodes = HashMap::new();
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        for n in list(conn)? {
            if let Some(p) = n.parent {
                children.entry(p).or_default().push(n.id);
            }
            nodes.insert(n.id, n);
        }
        Ok(Tree { nodes, children })
    }

    fn kids(&self, id: i64) -> impl Iterator<Item = &Node> {
        self.children.get(&id).into_iter().flatten().filter_map(|c| self.nodes.get(c))
    }

    fn root_title(&self, id: i64) -> String {
        self.nodes.get(&id).map(|n| n.title.clone()).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------------------------
// Netscape bookmark files (what Firefox, Chrome and Edge export and import)

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn html_children(out: &mut String, tree: &Tree, parent: i64, depth: usize) {
    let pad = "    ".repeat(depth);
    for n in tree.kids(parent) {
        match n.kind {
            Kind::Separator => out.push_str(&format!("{pad}<HR>\n")),
            Kind::Folder => {
                out.push_str(&format!(
                    "{pad}<DT><H3 ADD_DATE=\"{}\" LAST_MODIFIED=\"{}\">{}</H3>\n{pad}<DL><p>\n",
                    n.created / 1000,
                    n.modified / 1000,
                    html_escape(&n.title)
                ));
                html_children(out, tree, n.id, depth + 1);
                out.push_str(&format!("{pad}</DL><p>\n"));
            }
            Kind::Bookmark => {
                let mut extra = String::new();
                if !n.keyword.is_empty() {
                    extra.push_str(&format!(" SHORTCUTURL=\"{}\"", html_escape(&n.keyword)));
                }
                if !n.tags.is_empty() {
                    extra.push_str(&format!(" TAGS=\"{}\"", html_escape(&n.tags.join(","))));
                }
                out.push_str(&format!(
                    "{pad}<DT><A HREF=\"{}\" ADD_DATE=\"{}\" LAST_MODIFIED=\"{}\"{extra}>{}</A>\n",
                    html_escape(&n.url),
                    n.created / 1000,
                    n.modified / 1000,
                    html_escape(&n.title)
                ));
            }
        }
    }
}

/// The whole tree as a Netscape bookmark file: the Bookmarks Menu's contents at the top, then the
/// toolbar and Other Bookmarks as the folders Firefox marks with `PERSONAL_TOOLBAR_FOLDER` and
/// `UNFILED_BOOKMARKS_FOLDER`.
pub fn export_html(conn: &Connection) -> rusqlite::Result<String> {
    let tree = Tree::load(conn)?;
    let mut out = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<!-- This is an automatically generated file.\n     It will be read and overwritten.\n     DO NOT EDIT! -->\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks Menu</H1>\n\n<DL><p>\n",
    );
    html_children(&mut out, &tree, ROOT_MENU, 1);
    for (root, flag) in [(ROOT_TOOLBAR, "PERSONAL_TOOLBAR_FOLDER"), (ROOT_OTHER, "UNFILED_BOOKMARKS_FOLDER")] {
        let (created, modified) = tree.nodes.get(&root).map(|n| (n.created, n.modified)).unwrap_or((0, 0));
        out.push_str(&format!(
            "    <DT><H3 ADD_DATE=\"{}\" LAST_MODIFIED=\"{}\" {flag}=\"true\">{}</H3>\n    <DL><p>\n",
            created / 1000,
            modified / 1000,
            html_escape(&tree.root_title(root))
        ));
        html_children(&mut out, &tree, root, 2);
        out.push_str("    </DL><p>\n");
    }
    out.push_str("</DL>\n");
    Ok(out)
}

enum Item {
    Folder(ParsedFolder),
    Mark(ParsedMark),
    Separator,
}

struct ParsedFolder {
    title: String,
    toolbar: bool,
    unfiled: bool,
    created: Option<i64>,
    modified: Option<i64>,
    items: Vec<Item>,
}

struct ParsedMark {
    url: String,
    title: String,
    keyword: String,
    tags: Vec<String>,
    created: Option<i64>,
    modified: Option<i64>,
}

/// Decodes the entities bookmark files use: the named ones and numeric ones.
fn html_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded = rest.find(';').filter(|e| *e <= 10).and_then(|end| {
            let name = &rest[1..end];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => name
                    .strip_prefix('#')
                    .and_then(|n| match n.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => n.parse::<u32>().ok(),
                    })
                    .and_then(char::from_u32),
            };
            ch.map(|c| (c, end + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Removes tags from a text (a title that holds markup).
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Index of the `>` that ends the tag starting at `start`, skipping any inside quoted values.
fn tag_end(html: &str, start: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in html[start + 1..].char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(start + 1 + i),
            _ => {}
        }
    }
    None
}

/// `name="value"` pairs of a tag (names lower-cased, values still escaped).
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = tag.chars().collect();
    let mut i = 0;
    while i < chars.len() && !chars[i].is_whitespace() {
        i += 1;
    }
    while i < chars.len() {
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == '/') {
            i += 1;
        }
        let name_start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' && chars[i] != '/' {
            i += 1;
        }
        if i == name_start {
            break;
        }
        let name: String = chars[name_start..i].iter().collect::<String>().to_ascii_lowercase();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let q = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != q {
                    value.push(chars[i]);
                    i += 1;
                }
                i += 1;
            } else {
                while i < chars.len() && !chars[i].is_whitespace() {
                    value.push(chars[i]);
                    i += 1;
                }
            }
        }
        out.push((name, value));
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// A date attribute as milliseconds: files carry seconds (Firefox, Chrome), sometimes milliseconds
/// or microseconds.
fn date_ms(raw: Option<&str>) -> Option<i64> {
    let n: i64 = raw?.trim().parse().ok()?;
    match n {
        n if n <= 0 => None,
        n if n < 100_000_000_000 => Some(n * 1000),
        n if n < 100_000_000_000_000 => Some(n),
        n => Some(n / 1000),
    }
}

struct Frame {
    folder: Option<ParsedFolder>,
    items: Vec<Item>,
}

/// Reads a Netscape bookmark file into nested items. Folders are `<H3>` titles followed by a
/// `<DL>` list; the file's own outermost list is not a folder.
fn parse_html(html: &str) -> Vec<Item> {
    let lower = html.to_ascii_lowercase();
    let mut stack = vec![Frame { folder: None, items: Vec::new() }];
    let mut pending: Option<ParsedFolder> = None;
    let mut too_deep = 0usize;
    let mut i = 0;

    // A heading that never got a list is an empty folder.
    fn flush(pending: &mut Option<ParsedFolder>, stack: &mut [Frame]) {
        if let Some(f) = pending.take() {
            if let Some(top) = stack.last_mut() {
                top.items.push(Item::Folder(f));
            }
        }
    }

    while let Some(rel) = lower[i..].find('<') {
        let start = i + rel;
        if lower[start..].starts_with("<!--") {
            i = lower[start + 4..].find("-->").map(|e| start + 4 + e + 3).unwrap_or(lower.len());
            continue;
        }
        let Some(end) = tag_end(html, start) else { break };
        let tag = &html[start + 1..end];
        let name = lower[start + 1..end].split(|c: char| c.is_whitespace() || c == '/').find(|s| !s.is_empty()).unwrap_or("");
        let closing = lower[start + 1..].starts_with('/');
        i = end + 1;
        match (closing, name) {
            (false, "h3") => {
                flush(&mut pending, &mut stack);
                let close = lower[i..].find("</h3>").map(|c| i + c).unwrap_or(lower.len());
                let attrs = attributes(tag);
                pending = Some(ParsedFolder {
                    title: html_unescape(strip_tags(&html[i..close]).trim()),
                    toolbar: attr(&attrs, "personal_toolbar_folder").is_some_and(|v| v.eq_ignore_ascii_case("true")),
                    unfiled: attr(&attrs, "unfiled_bookmarks_folder").is_some_and(|v| v.eq_ignore_ascii_case("true")),
                    created: date_ms(attr(&attrs, "add_date")),
                    modified: date_ms(attr(&attrs, "last_modified")),
                    items: Vec::new(),
                });
                i = (close + 5).min(lower.len());
            }
            (false, "dl") => {
                // Nesting past any real file (a hostile one could nest a million lists): the lists
                // beyond the limit are flattened into the one at the limit.
                if stack.len() >= 2 * MAX_DEPTH {
                    too_deep += 1;
                    pending = None;
                    continue;
                }
                let folder = pending.take();
                stack.push(Frame { folder, items: Vec::new() });
            }
            (true, "dl") => {
                if too_deep > 0 {
                    too_deep -= 1;
                    continue;
                }
                flush(&mut pending, &mut stack);
                if stack.len() > 1 {
                    let frame = stack.pop().expect("a frame above the base one");
                    let top = stack.last_mut().expect("the base frame");
                    match frame.folder {
                        Some(mut f) => {
                            f.items = frame.items;
                            top.items.push(Item::Folder(f));
                        }
                        // A list without a heading (the file's outermost one): its items belong here.
                        None => top.items.extend(frame.items),
                    }
                }
            }
            (false, "a") => {
                flush(&mut pending, &mut stack);
                let close = lower[i..].find("</a>").map(|c| i + c).unwrap_or(lower.len());
                let attrs = attributes(tag);
                if let Some(href) = attr(&attrs, "href") {
                    let mark = ParsedMark {
                        url: html_unescape(href.trim()),
                        title: html_unescape(strip_tags(&html[i..close]).trim()),
                        keyword: attr(&attrs, "shortcuturl").map(html_unescape).unwrap_or_default(),
                        tags: attr(&attrs, "tags").map(|t| html_unescape(t).split(',').map(str::to_string).collect()).unwrap_or_default(),
                        created: date_ms(attr(&attrs, "add_date")),
                        modified: date_ms(attr(&attrs, "last_modified")),
                    };
                    if let Some(top) = stack.last_mut() {
                        top.items.push(Item::Mark(mark));
                    }
                }
                i = (close + 4).min(lower.len());
            }
            (false, "hr") => {
                flush(&mut pending, &mut stack);
                if let Some(top) = stack.last_mut() {
                    top.items.push(Item::Separator);
                }
            }
            _ => {}
        }
    }
    flush(&mut pending, &mut stack);
    // An unclosed file: close what is still open.
    while stack.len() > 1 {
        let frame = stack.pop().expect("a frame above the base one");
        let top = stack.last_mut().expect("the base frame");
        match frame.folder {
            Some(mut f) => {
                f.items = frame.items;
                top.items.push(Item::Folder(f));
            }
            None => top.items.extend(frame.items),
        }
    }
    stack.pop().map(|f| f.items).unwrap_or_default()
}

/// Turns parsed items into insertable nodes. Links other than http(s) (javascript:, place:, file:)
/// are dropped; so is anything past the depth and size limits.
fn convert_items(items: &[Item], depth: usize, budget: &mut usize) -> Vec<Prepared> {
    let mut out = Vec::new();
    for item in items {
        if *budget == 0 {
            break;
        }
        match item {
            Item::Separator => {
                *budget -= 1;
                out.push(Prepared {
                    kind: Kind::Separator,
                    title: String::new(),
                    url: String::new(),
                    tags: Vec::new(),
                    keyword: String::new(),
                    spaces: Vec::new(),
                    created: None,
                    modified: None,
                    children: Vec::new(),
                });
            }
            Item::Mark(m) => {
                let Ok(url) = clean_url(&m.url) else { continue };
                *budget -= 1;
                out.push(Prepared {
                    kind: Kind::Bookmark,
                    title: clean_title(&m.title),
                    url,
                    tags: clean_tags(&m.tags),
                    keyword: clean_keyword(&m.keyword).unwrap_or_default(),
                    spaces: Vec::new(),
                    created: m.created,
                    modified: m.modified,
                    children: Vec::new(),
                });
            }
            Item::Folder(f) => {
                if depth >= MAX_DEPTH {
                    continue;
                }
                *budget -= 1;
                out.push(Prepared {
                    kind: Kind::Folder,
                    title: clean_title(&f.title),
                    url: String::new(),
                    tags: Vec::new(),
                    keyword: String::new(),
                    spaces: Vec::new(),
                    created: f.created,
                    modified: f.modified,
                    children: convert_items(&f.items, depth + 1, budget),
                });
            }
        }
    }
    out
}

/// Imports a Netscape bookmark file into the existing bookmarks. The folder marked as the toolbar
/// merges into the Bookmarks Toolbar, the one marked "unfiled" into Other Bookmarks, and everything
/// else goes into the Bookmarks Menu with its folders. Same-named folders are reused and bookmarks
/// already in their folder are skipped, so importing a file twice adds nothing. Returns the number
/// of new bookmarks.
pub fn import_html(conn: &mut Connection, html: &str) -> rusqlite::Result<usize> {
    let items = parse_html(html);
    let mut budget = MAX_NODES;
    let (mut toolbar, mut menu, mut other) = (Vec::new(), Vec::new(), Vec::new());
    let mut loose: Vec<&Item> = Vec::new();
    for item in &items {
        match item {
            Item::Folder(f) if f.toolbar => toolbar.extend(convert_items(&f.items, 1, &mut budget)),
            Item::Folder(f) if f.unfiled => other.extend(convert_items(&f.items, 1, &mut budget)),
            _ => loose.push(item),
        }
    }
    for item in loose {
        menu.extend(convert_items(std::slice::from_ref(item), 0, &mut budget));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut ctx = Ctx { now: now_ms(), steal_keywords: false, merge: true, added: 0 };
    for (root, nodes) in [(ROOT_TOOLBAR, &toolbar), (ROOT_MENU, &menu), (ROOT_OTHER, &other)] {
        // Separators only make sense when the folder starts out empty.
        let fresh = children_ids(&tx, root)?.is_empty();
        for p in nodes {
            insert_one(&tx, root, p, &mut ctx, fresh)?;
        }
    }
    tx.commit()?;
    Ok(ctx.added)
}

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

// ---------------------------------------------------------------------------------------------
// Favicons

/// Largest favicon file read.
pub const FAVICON_MAX_BYTES: usize = 128 * 1024;

/// The address of a site's default icon, `/favicon.ico` on the page's own origin. None for
/// anything but a plain http(s) address.
pub fn favicon_url(page: &str) -> Option<String> {
    let rest = page.strip_prefix("https://").map(|r| ("https", r)).or_else(|| page.strip_prefix("http://").map(|r| ("http", r)))?;
    let authority = rest.1.split(['/', '?', '#']).next()?;
    // Credentials in the address are never sent along.
    if authority.is_empty() || authority.contains('@') || authority.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    // Judge the host the way the request will see it: parsed, so that forms like 127.1, 0x7f.1 or
    // 2130706433 are the loopback address they are. Only real public host names and public IPv4
    // addresses are asked.
    let url = tauri::Url::parse(&format!("{}://{authority}/favicon.ico", rest.0)).ok()?;
    let host = url.host_str()?;
    let public = if url.domain().is_some() {
        host.rsplit('.').next().is_some_and(|tld| tld.chars().any(|c| c.is_ascii_alphabetic())) && !crate::media::detect::is_private_host(host)
    } else {
        // An address: IPv4 only (IPv6 forms are too many to judge), and not a private one.
        !host.contains(':') && !crate::media::detect::is_private_host(host)
    };
    public.then(|| url.to_string())
}

/// A `data:` URL for an icon file, when its first bytes say it is a raster image. SVG is not
/// accepted: the UI shows icons in `<img>` where it would be harmless, but nothing needs it.
pub fn favicon_data_url(bytes: &[u8]) -> Option<String> {
    use base64::Engine;
    if bytes.is_empty() || bytes.len() > FAVICON_MAX_BYTES {
        return None;
    }
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) && bytes.len() > 6 {
        "image/x-icon"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return None;
    };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

// ---------------------------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        migrate(&c).unwrap();
        c
    }

    fn mark(title: &str, url: &str) -> NewNode {
        NewNode {
            kind: Kind::Bookmark,
            title: title.into(),
            url: url.into(),
            tags: vec![],
            keyword: String::new(),
            spaces: vec![],
            children: vec![],
            created: None,
            modified: None,
        }
    }

    fn folder(title: &str, children: Vec<NewNode>) -> NewNode {
        NewNode { kind: Kind::Folder, title: title.into(), children, ..mark("", "") }
    }

    fn sep() -> NewNode {
        NewNode { kind: Kind::Separator, ..mark("", "") }
    }

    fn titles(c: &Connection, parent: i64) -> Vec<String> {
        list(c).unwrap().into_iter().filter(|n| n.parent == Some(parent)).map(|n| n.title).collect()
    }

    fn positions_are_contiguous(c: &Connection) {
        let all = list(c).unwrap();
        let mut by_parent: HashMap<i64, Vec<i64>> = HashMap::new();
        for n in &all {
            if let Some(p) = n.parent {
                by_parent.entry(p).or_default().push(n.position);
            }
        }
        for (p, mut pos) in by_parent {
            pos.sort_unstable();
            assert_eq!(pos, (0..pos.len() as i64).collect::<Vec<_>>(), "positions under {p}");
        }
    }

    fn err(r: rusqlite::Result<impl std::fmt::Debug>) -> String {
        r.unwrap_err().to_string()
    }

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
    fn adding_inserts_at_an_index_and_keeps_positions_contiguous() {
        let mut c = mem();
        add(&mut c, ROOT_TOOLBAR, None, &[mark("a", "https://a.example/"), mark("b", "https://b.example/")]).unwrap();
        let ids = add(&mut c, ROOT_TOOLBAR, Some(1), &[mark("x", "https://x.example/"), mark("y", "https://y.example/")]).unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["a", "x", "y", "b"]);
        add(&mut c, ROOT_TOOLBAR, Some(-5), &[sep()]).unwrap();
        add(&mut c, ROOT_TOOLBAR, Some(99), &[folder("end", vec![])]).unwrap();
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["", "a", "x", "y", "b", "end"]);
        positions_are_contiguous(&c);
        // The same page can be bookmarked twice.
        add(&mut c, ROOT_MENU, None, &[mark("a", "https://a.example/"), mark("a again", "https://a.example/")]).unwrap();
        assert_eq!(titles(&c, ROOT_MENU).len(), 2);
    }

    #[test]
    fn folders_can_arrive_with_children() {
        let mut c = mem();
        let ids = add(&mut c, ROOT_MENU, None, &[folder("Dev", vec![mark("Docs", "https://docs.example/"), folder("Tools", vec![mark("T", "https://t.example/")])])]).unwrap();
        assert_eq!(ids.len(), 1, "only the top-level id is returned");
        assert_eq!(titles(&c, ids[0]), ["Docs", "Tools"]);
        let tools = list(&c).unwrap().into_iter().find(|n| n.title == "Tools").unwrap();
        assert_eq!(titles(&c, tools.id), ["T"]);
    }

    #[test]
    fn bad_requests_are_refused_and_write_nothing() {
        let mut c = mem();
        assert!(err(add(&mut c, ROOT_TOOLBAR, None, &[mark("x", "ftp://x.example/")])).contains("http(s)"));
        assert!(err(add(&mut c, ROOT_TOOLBAR, None, &[mark("x", "javascript:alert(1)")])).contains("http(s)"));
        assert!(err(add(&mut c, ROOT_TOOLBAR, None, &[mark("x", "https://x.example/\nnext")])).contains("valid"));
        assert!(err(add(&mut c, ROOT_TOOLBAR, None, &[mark("x", &format!("https://x.example/{}", "a".repeat(9000)))])).contains("valid"));
        let with_url = NewNode { url: "https://x.example/".into(), ..folder("f", vec![]) };
        assert!(err(add(&mut c, ROOT_TOOLBAR, None, &[with_url])).contains("address"));
        let kids_on_mark = NewNode { children: vec![mark("y", "https://y.example/")], ..mark("x", "https://x.example/") };
        assert!(err(add(&mut c, ROOT_TOOLBAR, None, &[kids_on_mark])).contains("folders"));
        let keyword = NewNode { keyword: "two words".into(), ..mark("x", "https://x.example/") };
        assert!(err(add(&mut c, ROOT_TOOLBAR, None, &[keyword])).contains("keyword"));
        // A valid node next to an invalid one: nothing is written.
        assert!(add(&mut c, ROOT_TOOLBAR, None, &[mark("ok", "https://ok.example/"), mark("bad", "nope")]).is_err());
        assert!(titles(&c, ROOT_TOOLBAR).is_empty());
        // Parents must exist and be folders.
        let id = add(&mut c, ROOT_TOOLBAR, None, &[mark("m", "https://m.example/")]).unwrap()[0];
        assert!(err(add(&mut c, id, None, &[mark("x", "https://x.example/")])).contains("folders"));
        assert!(err(add(&mut c, 999, None, &[mark("x", "https://x.example/")])).contains("no longer exists"));
        // Too deep.
        let mut deep = folder("leaf", vec![]);
        for _ in 0..40 {
            deep = folder("level", vec![deep]);
        }
        assert!(err(add(&mut c, ROOT_TOOLBAR, None, &[deep])).contains("deeply"));
    }

    #[test]
    fn titles_are_trimmed_and_cut_and_tags_are_cleaned() {
        let mut c = mem();
        let long = "t".repeat(2000);
        let node = NewNode {
            tags: vec![" News ".into(), "news".into(), "a,b".into(), "".into(), "  ".into(), "x".repeat(100), "Work  Stuff".into()],
            spaces: vec!["home".into(), " home ".into(), "".into(), "a,b".into(), "work".into()],
            ..mark(&format!("  {long}  "), "  https://x.example/  ")
        };
        let id = add(&mut c, ROOT_TOOLBAR, None, &[node]).unwrap()[0];
        let n = list(&c).unwrap().into_iter().find(|n| n.id == id).unwrap();
        assert_eq!(n.title.chars().count(), MAX_TITLE);
        assert_eq!(n.url, "https://x.example/");
        assert_eq!(n.tags, ["News", "a b", &"x".repeat(64), "Work Stuff"]);
        assert_eq!(n.spaces, ["home", "work"]);
    }

    #[test]
    fn update_changes_only_what_is_given() {
        let mut c = mem();
        let id = add(&mut c, ROOT_TOOLBAR, None, &[mark("Old", "https://old.example/")]).unwrap()[0];
        update(&mut c, id, &NodePatch { title: Some("New".into()), tags: Some(vec!["a".into(), "A".into(), "b".into()]), ..Default::default() }).unwrap();
        let n = list(&c).unwrap().into_iter().find(|n| n.id == id).unwrap();
        assert_eq!((n.title.as_str(), n.url.as_str()), ("New", "https://old.example/"));
        assert_eq!(n.tags, ["a", "b"]);
        update(&mut c, id, &NodePatch { url: Some("https://new.example/".into()), spaces: Some(vec!["home".into()]), ..Default::default() }).unwrap();
        let n = list(&c).unwrap().into_iter().find(|n| n.id == id).unwrap();
        assert_eq!((n.url.as_str(), n.tags.len(), n.spaces.clone()), ("https://new.example/", 2, vec!["home".to_string()]));
        assert!(err(update(&mut c, id, &NodePatch { url: Some("mailto:a@b.c".into()), ..Default::default() })).contains("http(s)"));
        assert!(err(update(&mut c, 999, &NodePatch::default())).contains("no longer exists"));
        // Roots keep their names; a folder has no address, tags or keyword.
        update(&mut c, ROOT_MENU, &NodePatch { title: Some("Renamed".into()), ..Default::default() }).unwrap();
        assert_eq!(titles(&c, 0).len(), 0);
        assert_eq!(list(&c).unwrap().iter().find(|n| n.id == ROOT_MENU).unwrap().title, "Bookmarks Menu");
        let f = add(&mut c, ROOT_TOOLBAR, None, &[folder("F", vec![])]).unwrap()[0];
        update(&mut c, f, &NodePatch { title: Some("Folder".into()), url: Some("https://x.example/".into()), tags: Some(vec!["t".into()]), keyword: Some("k".into()), ..Default::default() }).unwrap();
        let n = list(&c).unwrap().into_iter().find(|n| n.id == f).unwrap();
        assert_eq!((n.title.as_str(), n.url.as_str(), n.tags.len(), n.keyword.as_str()), ("Folder", "", 0, ""));
    }

    #[test]
    fn a_keyword_belongs_to_one_bookmark() {
        let mut c = mem();
        let a = add(&mut c, ROOT_TOOLBAR, None, &[NewNode { keyword: "W".into(), ..mark("a", "https://a.example/") }]).unwrap()[0];
        let b = add(&mut c, ROOT_TOOLBAR, None, &[mark("b", "https://b.example/")]).unwrap()[0];
        let kw = |c: &Connection, id: i64| list(c).unwrap().into_iter().find(|n| n.id == id).unwrap().keyword;
        assert_eq!(kw(&c, a), "w", "lower-cased");
        update(&mut c, b, &NodePatch { keyword: Some("w".into()), ..Default::default() }).unwrap();
        assert_eq!((kw(&c, a).as_str(), kw(&c, b).as_str()), ("", "w"), "the newest one takes it");
        add(&mut c, ROOT_TOOLBAR, None, &[NewNode { keyword: "w".into(), ..mark("c", "https://c.example/") }]).unwrap();
        assert_eq!(kw(&c, b), "");
    }

    #[test]
    fn moving_reorders_within_a_folder_and_between_folders() {
        let mut c = mem();
        let ids = add(&mut c, ROOT_TOOLBAR, None, &[mark("a", "https://a.example/"), mark("b", "https://b.example/"), mark("c", "https://c.example/"), folder("F", vec![])]).unwrap();
        let (a, b, f) = (ids[0], ids[1], ids[3]);
        // Index counts the children once the node has left: a to index 1 sits between b and c.
        move_node(&mut c, a, ROOT_TOOLBAR, 1).unwrap();
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["b", "a", "c", "F"]);
        move_node(&mut c, a, ROOT_TOOLBAR, 99).unwrap();
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["b", "c", "F", "a"]);
        move_node(&mut c, b, f, 0).unwrap();
        assert_eq!(titles(&c, f), ["b"]);
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["c", "F", "a"]);
        move_node(&mut c, a, f, 0).unwrap();
        assert_eq!(titles(&c, f), ["a", "b"]);
        positions_are_contiguous(&c);
    }

    #[test]
    fn a_move_that_would_break_the_tree_is_refused() {
        let mut c = mem();
        let outer = add(&mut c, ROOT_TOOLBAR, None, &[folder("outer", vec![folder("inner", vec![])])]).unwrap()[0];
        let inner = list(&c).unwrap().into_iter().find(|n| n.title == "inner").unwrap().id;
        let m = add(&mut c, ROOT_TOOLBAR, None, &[mark("m", "https://m.example/")]).unwrap()[0];
        assert!(err(move_node(&mut c, outer, inner, 0)).contains("itself"));
        assert!(err(move_node(&mut c, outer, outer, 0)).contains("itself"));
        assert!(err(move_node(&mut c, ROOT_MENU, ROOT_TOOLBAR, 0)).contains("can't be moved"));
        assert!(err(move_node(&mut c, outer, m, 0)).contains("folders"));
        assert!(err(move_node(&mut c, outer, 999, 0)).contains("no longer exists"));
        assert!(err(move_node(&mut c, 999, ROOT_TOOLBAR, 0)).contains("no longer exists"));
        move_node(&mut c, inner, ROOT_TOOLBAR, 0).unwrap();
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["inner", "outer", "m"]);
    }

    #[test]
    fn reordering_needs_every_child_exactly_once() {
        let mut c = mem();
        let ids = add(&mut c, ROOT_TOOLBAR, None, &[mark("a", "https://a.example/"), mark("b", "https://b.example/"), mark("c", "https://c.example/")]).unwrap();
        reorder(&mut c, ROOT_TOOLBAR, &[ids[2], ids[0], ids[1]]).unwrap();
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["c", "a", "b"]);
        assert!(reorder(&mut c, ROOT_TOOLBAR, &[ids[0], ids[1]]).is_err());
        assert!(reorder(&mut c, ROOT_TOOLBAR, &[ids[0], ids[0], ids[1]]).is_err());
        assert!(reorder(&mut c, ROOT_TOOLBAR, &[ids[0], ids[1], 999]).is_err());
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["c", "a", "b"], "a refused order changes nothing");
        positions_are_contiguous(&c);
    }

    #[test]
    fn removing_deletes_folders_with_their_contents_and_closes_the_gap() {
        let mut c = mem();
        let ids = add(
            &mut c,
            ROOT_TOOLBAR,
            None,
            &[mark("a", "https://a.example/"), folder("F", vec![mark("in", "https://in.example/"), folder("G", vec![mark("deep", "https://deep.example/")])]), mark("z", "https://z.example/")],
        )
        .unwrap();
        remove(&mut c, &[ids[1]]).unwrap();
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["a", "z"]);
        assert_eq!(list(&c).unwrap().len(), 5, "three roots and two bookmarks: nothing below F is left behind");
        positions_are_contiguous(&c);
        // Already gone, or listed twice, or a child listed with its parent: fine.
        remove(&mut c, &[999, ids[0], ids[0]]).unwrap();
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["z"]);
        let nested = add(&mut c, ROOT_MENU, None, &[folder("P", vec![mark("c", "https://c.example/")])]).unwrap()[0];
        let child = list(&c).unwrap().into_iter().find(|n| n.parent == Some(nested)).unwrap().id;
        remove(&mut c, &[nested, child]).unwrap();
        assert!(titles(&c, ROOT_MENU).is_empty());
        // The built-in folders stay, and a refused call deletes nothing.
        assert!(err(remove(&mut c, &[ids[2], ROOT_OTHER])).contains("can't be deleted"));
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["z"]);
    }

    #[test]
    fn removing_by_address_deletes_every_copy() {
        let mut c = mem();
        add(&mut c, ROOT_TOOLBAR, None, &[mark("one", "https://same.example/"), mark("other", "https://other.example/"), mark("two", "https://same.example/")]).unwrap();
        add(&mut c, ROOT_MENU, None, &[mark("three", "https://same.example/")]).unwrap();
        remove_url(&mut c, "https://same.example/").unwrap();
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["other"]);
        assert!(titles(&c, ROOT_MENU).is_empty());
        positions_are_contiguous(&c);
    }

    fn sample_tree(c: &mut Connection) {
        add(c, ROOT_TOOLBAR, None, &[NewNode { tags: vec!["news".into(), "daily".into()], keyword: "n".into(), ..mark("News & \"views\"", "https://news.example/?a=1&b=2") }, sep(), folder("Dev", vec![mark("Docs", "https://docs.example/")])]).unwrap();
        add(c, ROOT_MENU, None, &[folder("Menu folder", vec![folder("Nested", vec![mark("Deep <b>", "https://deep.example/")])]), mark("Top", "https://top.example/")]).unwrap();
        add(c, ROOT_OTHER, None, &[mark("Loose", "https://loose.example/")]).unwrap();
    }

    fn shape(c: &Connection) -> Vec<(String, String, String, Vec<String>, String)> {
        // (path, kind, title, tags, url) for everything below the roots, in tree order.
        let all = list(c).unwrap();
        let by_id: HashMap<i64, &Node> = all.iter().map(|n| (n.id, n)).collect();
        let path = |n: &Node| {
            let mut parts = vec![];
            let mut cur = n.parent;
            while let Some(p) = cur {
                parts.push(by_id[&p].title.clone());
                cur = by_id[&p].parent;
            }
            parts.reverse();
            parts.join("/")
        };
        let mut out: Vec<_> = all.iter().filter(|n| n.parent.is_some()).map(|n| (path(n), n.kind.as_str().to_string(), n.title.clone(), n.tags.clone(), n.url.clone())).collect();
        out.sort();
        out
    }

    #[test]
    fn html_export_and_import_round_trip() {
        let mut a = mem();
        sample_tree(&mut a);
        let html = export_html(&a).unwrap();
        assert!(html.contains("PERSONAL_TOOLBAR_FOLDER=\"true\">Bookmarks Toolbar</H3>"));
        assert!(html.contains("UNFILED_BOOKMARKS_FOLDER=\"true\">Other Bookmarks</H3>"));
        assert!(html.contains("SHORTCUTURL=\"n\""));
        assert!(html.contains("TAGS=\"news,daily\""));
        assert!(html.contains("<HR>"));
        assert!(html.contains("News &amp; &quot;views&quot;"));
        assert!(html.contains("a=1&amp;b=2"));
        let mut b = mem();
        let n = import_html(&mut b, &html).unwrap();
        assert_eq!(n, 5);
        assert_eq!(shape(&a), shape(&b));
        let toolbar = titles(&b, ROOT_TOOLBAR);
        assert_eq!(toolbar, ["News & \"views\"", "", "Dev"], "the separator comes back in place");
        let news = list(&b).unwrap().into_iter().find(|n| n.keyword == "n").unwrap();
        assert_eq!(news.tags, ["news", "daily"]);
        // Importing the same file again adds nothing.
        assert_eq!(import_html(&mut b, &html).unwrap(), 0);
        assert_eq!(shape(&a), shape(&b));
        positions_are_contiguous(&b);
    }

    #[test]
    fn firefox_and_chrome_files_land_in_the_right_places() {
        let firefox = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks Menu</H1>
<DL><p>
    <DT><H3 ADD_DATE="1600000000" LAST_MODIFIED="1600000001">Mozilla Firefox</H3>
    <DL><p>
        <DT><A HREF="https://www.mozilla.org/firefox/help/" ADD_DATE="1600000002" LAST_MODIFIED="1600000003" TAGS="help,firefox">Get Help</A>
        <DD>A description line that is ignored
    </DL><p>
    <DT><H3 ADD_DATE="1" LAST_MODIFIED="2" PERSONAL_TOOLBAR_FOLDER="true">Bookmarks Toolbar</H3>
    <DL><p>
        <DT><A HREF="https://toolbar.example/" ADD_DATE="1">On the bar</A>
        <DT><A HREF="place:sort=8&maxResults=10" ADD_DATE="1">A smart folder</A>
        <DT><A HREF="javascript:alert(1)">Bookmarklet</A>
    </DL><p>
    <DT><H3 ADD_DATE="1" LAST_MODIFIED="2" UNFILED_BOOKMARKS_FOLDER="true">Other Bookmarks</H3>
    <DL><p>
        <DT><A HREF="https://other.example/">Unsorted</A>
        <DT><H3>Empty folder</H3>
        <DL><p>
        </DL><p>
    </DL><p>
</DL>"#;
        let mut c = mem();
        assert_eq!(import_html(&mut c, firefox).unwrap(), 3);
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["On the bar"]);
        assert_eq!(titles(&c, ROOT_OTHER), ["Unsorted", "Empty folder"]);
        assert_eq!(titles(&c, ROOT_MENU), ["Mozilla Firefox"]);
        let help = list(&c).unwrap().into_iter().find(|n| n.title == "Get Help").unwrap();
        assert_eq!(help.tags, ["help", "firefox"]);
        assert_eq!(help.created, 1_600_000_002_000);
        assert_eq!(help.modified, 1_600_000_003_000);

        let chrome = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks</H1>
<DL><p>
    <DT><H3 ADD_DATE="1700000000" LAST_MODIFIED="1700000000" PERSONAL_TOOLBAR_FOLDER="true">Bookmarks bar</H3>
    <DL><p>
        <DT><A HREF="https://bar.example/" ADD_DATE="1700000001" ICON="data:image/png;base64,AAAA">Bar item</A>
        <DT><H3 ADD_DATE="1700000002" LAST_MODIFIED="1700000002">Recipes</H3>
        <DL><p>
            <DT><A HREF="https://pasta.example/?a=1&amp;b=2" ADD_DATE="1700000003">Pasta &#39;n&#x27; more</A>
        </DL><p>
    </DL><p>
    <DT><H3 ADD_DATE="1700000004" LAST_MODIFIED="1700000004">Other bookmarks</H3>
    <DL><p>
        <DT><A HREF="HTTPS://UPPER.example/" ADD_DATE="1700000005">Upper case scheme</A>
    </DL><p>
</DL><p>"#;
        let mut c = mem();
        assert_eq!(import_html(&mut c, chrome).unwrap(), 3);
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["Bar item", "Recipes"]);
        assert_eq!(titles(&c, ROOT_MENU), ["Other bookmarks"]);
        let pasta = list(&c).unwrap().into_iter().find(|n| n.url.contains("pasta")).unwrap();
        assert_eq!(pasta.url, "https://pasta.example/?a=1&b=2");
        assert_eq!(pasta.title, "Pasta 'n' more");
    }

    #[test]
    fn importing_merges_into_existing_folders_without_touching_keywords() {
        let mut c = mem();
        add(&mut c, ROOT_TOOLBAR, None, &[NewNode { keyword: "k".into(), ..mark("mine", "https://mine.example/") }, folder("Dev", vec![mark("kept", "https://kept.example/")])]).unwrap();
        let html = r#"<DL><p>
            <DT><H3 PERSONAL_TOOLBAR_FOLDER="true">Bookmarks Toolbar</H3>
            <DL><p>
              <DT><A HREF="https://mine.example/" SHORTCUTURL="x">same page, same folder</A>
              <DT><A HREF="https://new.example/" SHORTCUTURL="k">takes a used keyword</A>
              <DT><H3>Dev</H3>
              <DL><p>
                <DT><A HREF="https://kept.example/">duplicate</A>
                <DT><A HREF="https://added.example/">added</A>
              </DL><p>
            </DL><p>
        </DL>"#;
        assert_eq!(import_html(&mut c, html).unwrap(), 2);
        let all = list(&c).unwrap();
        assert_eq!(all.iter().filter(|n| n.title == "Dev").count(), 1, "the existing folder is reused");
        let dev = all.iter().find(|n| n.title == "Dev").unwrap().id;
        assert_eq!(titles(&c, dev), ["kept", "added"]);
        assert_eq!(all.iter().find(|n| n.title == "mine").unwrap().keyword, "k");
        assert_eq!(all.iter().find(|n| n.title == "takes a used keyword").unwrap().keyword, "");
        assert!(import_html(&mut c, "not a bookmarks file at all").unwrap() == 0);
    }

    #[test]
    fn entities_and_odd_markup_are_read_tolerantly() {
        assert_eq!(html_unescape("a &amp; b &lt;c&gt; &quot;d&quot; &#65;&#x42; &unknown; &"), "a & b <c> \"d\" AB &unknown; &");
        assert_eq!(strip_tags("Deep <b>bold</b> text"), "Deep bold text");
        let attrs = attributes("A HREF=\"https://x.example/?q=a>b\" add_date=5 Tags='x,y' hidden");
        assert_eq!(attr(&attrs, "href"), Some("https://x.example/?q=a>b"));
        assert_eq!(attr(&attrs, "add_date"), Some("5"));
        assert_eq!(attr(&attrs, "tags"), Some("x,y"));
        assert_eq!(attr(&attrs, "hidden"), Some(""));
        assert_eq!(date_ms(Some("1700000000")), Some(1_700_000_000_000));
        assert_eq!(date_ms(Some("1700000000000")), Some(1_700_000_000_000));
        assert_eq!(date_ms(Some("1700000000000000")), Some(1_700_000_000_000));
        assert_eq!(date_ms(Some("0")), None);
        assert_eq!(date_ms(Some("x")), None);
        // A '>' inside a quoted address doesn't end the tag.
        let mut c = mem();
        let html = "<DL><DT><A HREF=\"https://x.example/?q=a>b\">Quoted</A><DT><a href='https://y.example/'>single quotes</a></DL>";
        assert_eq!(import_html(&mut c, html).unwrap(), 2);
        assert_eq!(titles(&c, ROOT_MENU), ["Quoted", "single quotes"]);
    }

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

    #[test]
    fn other_browsers_land_in_a_folder_on_the_toolbar_and_importing_twice_adds_nothing() {
        let mut c = mem();
        let b = |url: &str, folder: &str| FlatBookmark { id: 0, url: url.into(), title: "t".into(), folder: folder.into(), position: 0, created: 5000 };
        let items = [b("https://a.example/", "Chrome"), b("https://b.example/", "Chrome / Dev"), b("https://c.example/", "Chrome / Dev"), b("ftp://skipped.example/", "Chrome")];
        assert_eq!(import_paths(&mut c, &items).unwrap(), 3);
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["Chrome"]);
        let chrome = list(&c).unwrap().into_iter().find(|n| n.title == "Chrome").unwrap();
        // The first bookmark comes before the subfolder that the next one needed.
        assert_eq!(titles(&c, chrome.id), ["t", "Dev"]);
        assert_eq!(import_paths(&mut c, &items).unwrap(), 0);
        // The same page in another folder is a new bookmark.
        assert_eq!(import_paths(&mut c, &[b("https://a.example/", "Edge")]).unwrap(), 1);
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["Chrome", "Edge"]);
        let created = list(&c).unwrap().into_iter().find(|n| n.url == "https://a.example/").unwrap().created;
        assert_eq!(created, 5000, "the other browser's date is kept");
        positions_are_contiguous(&c);
    }

    #[test]
    fn favicons_come_from_the_site_itself_and_only_as_images() {
        assert_eq!(favicon_url("https://example.com/a/b?c#d").as_deref(), Some("https://example.com/favicon.ico"));
        assert_eq!(favicon_url("http://93.184.216.34/x").as_deref(), Some("http://93.184.216.34/favicon.ico"));
        for bad in [
            "ftp://example.com/", "javascript:alert(1)", "https://", "https://user:pw@example.com/", "https://exa mple.com/", "example.com",
            "http://localhost:3000/x", "http://127.0.0.1/", "http://127.1/", "http://0x7f.0.0.1/", "http://2130706433/", "http://192.168.1.1/",
            "http://169.254.169.254/", "http://[::1]/", "http://10.0.0.5:8080/", "http://router.local/", "http://intranet/", "http://0177.0.0.1/",
        ] {
            assert!(favicon_url(bad).is_none(), "{bad}");
        }
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
        assert!(favicon_data_url(&png).unwrap().starts_with("data:image/png;base64,"));
        assert!(favicon_data_url(&[0, 0, 1, 0, 1, 0, 16, 16]).unwrap().starts_with("data:image/x-icon;base64,"));
        assert!(favicon_data_url(b"<svg onload=alert(1)></svg>").is_none());
        assert!(favicon_data_url(b"<!DOCTYPE html><html>").is_none());
        assert!(favicon_data_url(&[]).is_none());
        assert!(favicon_data_url(&vec![0xFF; FAVICON_MAX_BYTES + 1]).is_none());
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

    #[test]
    fn hostile_nesting_in_a_file_is_contained() {
        let mut c = mem();
        let mut html = String::from("<DL><p>");
        for _ in 0..20_000 {
            html.push_str("<DT><H3>f</H3><DL><p>");
        }
        html.push_str("<DT><A HREF=\"https://deep.example/\">deep</A>");
        for _ in 0..20_000 {
            html.push_str("</DL><p>");
        }
        html.push_str("</DL>");
        // It neither overflows the stack nor creates more than the allowed depth.
        let n = import_html(&mut c, &html).unwrap();
        assert!(n <= 1, "the bookmark past the limit may be dropped, never duplicated");
        let depth = |c: &Connection| {
            let all = list(c).unwrap();
            let by: HashMap<i64, Option<i64>> = all.iter().map(|n| (n.id, n.parent)).collect();
            all.iter().map(|n| { let (mut d, mut cur) = (0, n.parent); while let Some(p) = cur { d += 1; cur = by[&p]; } d }).max().unwrap()
        };
        assert!(depth(&c) <= MAX_DEPTH + 2, "depth {}", depth(&c));
    }

    #[test]
    fn nodes_serialize_the_way_the_ui_reads_them() {
        let mut c = mem();
        let id = add(&mut c, ROOT_TOOLBAR, None, &[NewNode { tags: vec!["t".into()], spaces: vec!["home".into()], keyword: "k".into(), ..mark("T", "https://t.example/") }]).unwrap()[0];
        let nodes = list(&c).unwrap();
        let json = serde_json::to_value(&nodes).unwrap();
        let root = &json[0];
        assert_eq!(root["parent"], serde_json::Value::Null);
        assert_eq!(root["kind"], "folder");
        let n = json.as_array().unwrap().iter().find(|n| n["id"] == id).unwrap();
        assert_eq!(n["kind"], "bookmark");
        assert_eq!(n["tags"], serde_json::json!(["t"]));
        assert_eq!(n["spaces"], serde_json::json!(["home"]));
        assert_eq!(n["keyword"], "k");
        for key in ["id", "parent", "kind", "title", "url", "position", "created", "modified", "tags", "keyword", "spaces"] {
            assert!(n.get(key).is_some(), "{key}");
        }
        // The UI's creation requests leave most fields out.
        let req: NewNode = serde_json::from_str(r#"{"kind":"folder","title":"F","children":[{"kind":"separator"}]}"#).unwrap();
        assert_eq!((req.children.len(), req.url.as_str()), (1, ""));
        let patch: NodePatch = serde_json::from_str(r#"{"title":"x","tags":["a"]}"#).unwrap();
        assert_eq!((patch.title.as_deref(), patch.url.is_none(), patch.tags.map(|t| t.len())), (Some("x"), true, Some(1)));
    }
}
