//! Validation and cleaning of what arrives from the UI, files and backups: addresses, titles,
//! tags, keywords and workspaces, and whole nodes prepared for insertion.

use super::{invalid, Kind, NewNode, MAX_DEPTH, MAX_KEYWORD, MAX_SPACES, MAX_SPACE_ID, MAX_TAG, MAX_TAGS, MAX_TITLE, MAX_URL};

// ---------------------------------------------------------------------------------------------
// Validation and cleaning

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

pub(super) fn clean_title(t: &str) -> String {
    truncate(t.trim(), MAX_TITLE)
}

fn is_http(url: &str) -> bool {
    let lower: String = url.chars().take(8).collect::<String>().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

pub(super) fn clean_url(url: &str) -> rusqlite::Result<String> {
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
pub(super) fn clean_tags(tags: &[String]) -> Vec<String> {
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

pub(super) fn clean_keyword(k: &str) -> rusqlite::Result<String> {
    let k = k.trim();
    if k.chars().any(char::is_whitespace) {
        return Err(invalid("A keyword can't contain spaces."));
    }
    if k.chars().count() > MAX_KEYWORD {
        return Err(invalid("That keyword is too long."));
    }
    Ok(k.to_lowercase())
}

pub(super) fn clean_spaces(spaces: &[String]) -> Vec<String> {
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

pub(super) fn split_list(s: &str) -> Vec<String> {
    s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect()
}

/// A request after validation, ready to insert.
pub(super) struct Prepared {
    pub(super) kind: Kind,
    pub(super) title: String,
    pub(super) url: String,
    pub(super) tags: Vec<String>,
    pub(super) keyword: String,
    pub(super) spaces: Vec<String>,
    pub(super) created: Option<i64>,
    pub(super) modified: Option<i64>,
    pub(super) children: Vec<Prepared>,
}

pub(super) fn prepare(n: &NewNode, depth: usize, budget: &mut usize) -> rusqlite::Result<Prepared> {
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
