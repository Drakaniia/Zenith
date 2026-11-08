//! Reading and writing the tree: listing, adding, editing, moving, reordering and removing nodes,
//! importing other browsers' folder paths, and the whole tree in memory for export and backup.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::validate::{clean_keyword, clean_spaces, clean_tags, clean_title, clean_url, prepare, split_list, Prepared};
use super::{invalid, Kind, NewNode, Node, NodePatch, MAX_DEPTH, MAX_NODES, ROOTS, ROOT_TOOLBAR};
use crate::storage::{now_ms, Bookmark as FlatBookmark};

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

pub(super) fn children_ids(conn: &Connection, parent: i64) -> rusqlite::Result<Vec<i64>> {
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
pub(super) fn insert_row(
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
pub(super) fn ensure_path(conn: &Connection, root: i64, path: &str, now: i64) -> rusqlite::Result<i64> {
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

pub(super) struct Ctx {
    pub(super) now: i64,
    /// A keyword set on a new bookmark is taken from whoever had it (the user chose it just now);
    /// imports and restores keep what is there.
    pub(super) steal_keywords: bool,
    /// Importing into existing bookmarks: reuse same-named folders, skip bookmarks already there
    /// (and separators in a folder that already has content).
    pub(super) merge: bool,
    /// New bookmarks (not folders or separators).
    pub(super) added: usize,
}

fn keyword_taken(conn: &Connection, keyword: &str) -> rusqlite::Result<bool> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM bookmark_nodes WHERE keyword = ?1)", [keyword], |r| r.get(0))
}

/// Inserts a prepared node (and its children) at the end of `parent`. `None`: skipped (merge mode).
pub(super) fn insert_one(conn: &Connection, parent: i64, p: &Prepared, ctx: &mut Ctx, fresh_parent: bool) -> rusqlite::Result<Option<i64>> {
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

pub(super) struct Tree {
    pub(super) nodes: HashMap<i64, Node>,
    children: HashMap<i64, Vec<i64>>,
}

impl Tree {
    pub(super) fn load(conn: &Connection) -> rusqlite::Result<Tree> {
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

    pub(super) fn kids(&self, id: i64) -> impl Iterator<Item = &Node> {
        self.children.get(&id).into_iter().flatten().filter_map(|c| self.nodes.get(c))
    }

    pub(super) fn root_title(&self, id: i64) -> String {
        self.nodes.get(&id).map(|n| n.title.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bookmarks::{MAX_TITLE, ROOT_MENU, ROOT_OTHER};
    use crate::bookmarks::tests::{folder, mark, mem, positions_are_contiguous, sep, titles};

    fn err(r: rusqlite::Result<impl std::fmt::Debug>) -> String {
        r.unwrap_err().to_string()
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
}
