//! The bookmark tree: Firefox's Places model, as Zen shows it. Three built-in folders are the
//! roots (the Bookmarks Toolbar, the Bookmarks Menu and Other Bookmarks); below them come
//! folders, bookmarks and separators. A bookmark can carry tags, a keyword (typed in the address
//! bar to open it) and the workspaces it belongs to.
//!
//! Everything here is plain functions over a rusqlite connection, run on the database worker
//! thread through `Db::call`, so they are tested against in-memory databases. The Tauri commands
//! that call them live in `commands`.
//!
//! Mistakes the user can fix (a bad address, a move into the folder's own subfolder) come back as
//! `rusqlite::Error::ToSqlConversionFailure`, which displays as the message alone, so they pass
//! through `Db::call` and reach the UI as readable text.

pub mod commands;
mod backup;
mod favicon;
mod html;
mod schema;
mod tree;
mod validate;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use backup::{auto_backup, backup_json, date_string, list_backups, read_backup, restore, BackupInfo};
pub use favicon::{favicon_data_url, favicon_url, FAVICON_MAX_BYTES};
pub use html::{export_html, import_html};
pub use schema::migrate;
pub use tree::{add, import_paths, list, move_node, remove, remove_url, reorder, update};

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
// Tests

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rusqlite::Connection;

    use super::*;

    pub(super) fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        migrate(&c).unwrap();
        c
    }

    pub(super) fn mark(title: &str, url: &str) -> NewNode {
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

    pub(super) fn folder(title: &str, children: Vec<NewNode>) -> NewNode {
        NewNode { kind: Kind::Folder, title: title.into(), children, ..mark("", "") }
    }

    pub(super) fn sep() -> NewNode {
        NewNode { kind: Kind::Separator, ..mark("", "") }
    }

    pub(super) fn titles(c: &Connection, parent: i64) -> Vec<String> {
        list(c).unwrap().into_iter().filter(|n| n.parent == Some(parent)).map(|n| n.title).collect()
    }

    pub(super) fn positions_are_contiguous(c: &Connection) {
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

    pub(super) fn sample_tree(c: &mut Connection) {
        add(c, ROOT_TOOLBAR, None, &[NewNode { tags: vec!["news".into(), "daily".into()], keyword: "n".into(), ..mark("News & \"views\"", "https://news.example/?a=1&b=2") }, sep(), folder("Dev", vec![mark("Docs", "https://docs.example/")])]).unwrap();
        add(c, ROOT_MENU, None, &[folder("Menu folder", vec![folder("Nested", vec![mark("Deep <b>", "https://deep.example/")])]), mark("Top", "https://top.example/")]).unwrap();
        add(c, ROOT_OTHER, None, &[mark("Loose", "https://loose.example/")]).unwrap();
    }

    pub(super) fn shape(c: &Connection) -> Vec<(String, String, String, Vec<String>, String)> {
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
