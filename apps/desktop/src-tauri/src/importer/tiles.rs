//! Start-page tiles from other browsers, for Zenith's Tableau.
//!
//! Yandex keeps its Tableau in the profile file `Tablo`, a protobuf message without a published
//! schema: each tile is field 2 > field 1, holding the URL (1) and a details message (18) with
//! the page title (1) and the tile's own name (24). Other Chromium browsers have no pinned tiles
//! on disk worth reading, so their most visited sites (`Top Sites`) are used instead.
//!
//! shortcut: Chrome/Edge "custom shortcuts" (Preferences `custom_links`) and Firefox's pinned
//! sites (prefs.js `browser.newtabpage.pinned`) are not read, upgrade if users ask for them.

use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;

use super::{is_web, scan, with_copy, Engine};

/// Most tiles read from one profile; the Tableau keeps fewer anyway.
const TILE_LIMIT: usize = 48;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Tile {
    pub url: String,
    pub title: String,
}

/// Whether a profile folder has tiles to offer.
pub(super) fn available(dir: &Path, engine: Engine) -> bool {
    engine == Engine::Chromium && (dir.join("Tablo").is_file() || dir.join("Top Sites").is_file())
}

/// Reads the start-page tiles of a profile found by `detect`, in the other browser's order.
pub fn read(id: &str) -> Result<Vec<Tile>, String> {
    let found = scan()
        .into_iter()
        .find(|f| f.source.id == id && f.source.tiles)
        .ok_or_else(|| "That browser profile has no start page tiles.".to_string())?;
    let tablo = fs::read(found.dir.join("Tablo")).map(|b| yandex_tablo(&b)).unwrap_or_default();
    if !tablo.is_empty() {
        return Ok(tablo);
    }
    let name = &found.source.name;
    let top = found.dir.join("Top Sites");
    if !top.is_file() {
        return Ok(Vec::new());
    }
    with_copy(&top, chromium_top_sites).map_err(|_| format!("Couldn't read {name}'s start page tiles. Close {name} and try again."))
}

pub fn chromium_top_sites(conn: &Connection) -> rusqlite::Result<Vec<Tile>> {
    let mut stmt = conn.prepare("SELECT url, title FROM top_sites ORDER BY url_rank LIMIT ?1")?;
    let rows = stmt.query_map(params![TILE_LIMIT as i64], |r| Ok(Tile { url: r.get(0)?, title: r.get(1)? }))?;
    Ok(rows.filter_map(Result::ok).filter(|t| is_web(&t.url)).collect())
}

/// Tiles from Yandex's `Tablo` file; empty when it isn't in the expected shape.
pub fn yandex_tablo(bytes: &[u8]) -> Vec<Tile> {
    let text = |b: Option<&[u8]>| b.and_then(|b| std::str::from_utf8(b).ok()).unwrap_or("").trim().to_string();
    let mut out = Vec::new();
    for entry in field(bytes, 2) {
        let Some(item) = field(entry, 1).next() else { continue };
        let url = text(field(item, 1).next());
        if !is_web(&url) {
            continue;
        }
        let meta = field(item, 18).next().unwrap_or_default();
        let mut title = text(field(meta, 24).next());
        if title.is_empty() {
            title = text(field(meta, 1).next());
        }
        out.push(Tile { url, title });
        if out.len() >= TILE_LIMIT {
            break;
        }
    }
    out
}

/// The length-delimited values of field `n` in a protobuf message. Stops at the first malformed
/// byte, so damaged input gives fewer values rather than an error.
fn field(msg: &[u8], n: u64) -> impl Iterator<Item = &[u8]> {
    let mut b = msg;
    let mut out = Vec::new();
    while let Some(key) = varint(&mut b) {
        let skip = match key & 7 {
            0 => match varint(&mut b) {
                Some(_) => 0,
                None => break,
            },
            1 => 8,
            5 => 4,
            2 => match varint(&mut b) {
                Some(len) if (len as usize) <= b.len() => {
                    if key >> 3 == n {
                        out.push(&b[..len as usize]);
                    }
                    len as usize
                }
                _ => break,
            },
            _ => break,
        };
        if skip > b.len() {
            break;
        }
        b = &b[skip..];
    }
    out.into_iter()
}

fn varint(b: &mut &[u8]) -> Option<u64> {
    let mut v = 0u64;
    for i in 0..10 {
        let (&byte, rest) = b.split_first()?;
        *b = rest;
        v |= u64::from(byte & 0x7f) << (7 * i);
        if byte < 0x80 {
            return Some(v);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_varint(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    fn len(n: u64, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        put_varint(&mut out, n << 3 | 2); // fields 16+ need a two-byte key
        put_varint(&mut out, body.len() as u64);
        out.extend_from_slice(body);
        out
    }

    fn tile(url: &str, page: &str, name: &str) -> Vec<u8> {
        let meta = [len(1, page.as_bytes()), len(24, name.as_bytes()), vec![0x30, 0x01]].concat();
        let item = [len(1, url.as_bytes()), vec![0x10, 0x01], len(18, &meta)].concat();
        len(2, &len(1, &item))
    }

    #[test]
    fn reads_yandex_tablo() {
        let long = "x".repeat(300);
        let bytes = [
            vec![0x08, 0x02],
            tile("https://mail.google.com/", "Inbox - Gmail", "Gmail"),
            tile("https://roadmap.sh/", &long, ""),
            tile("browser://settings", "Settings", ""),
            len(3, &len(1, b"https://removed.example/")),
        ]
        .concat();
        let got = yandex_tablo(&bytes);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], Tile { url: "https://mail.google.com/".into(), title: "Gmail".into() });
        assert_eq!(got[1].title, long, "falls back to the page title");
        // Cut short or not protobuf at all: whatever parsed so far, never a panic.
        assert_eq!(yandex_tablo(&bytes[..bytes.len() / 2]).len(), 1);
        assert!(yandex_tablo(b"not a tablo file").is_empty());
        assert!(yandex_tablo(&[0xff; 12]).is_empty());
    }

    #[test]
    fn reads_top_sites() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE top_sites (url TEXT NOT NULL PRIMARY KEY, url_rank INTEGER NOT NULL, title TEXT NOT NULL);
             INSERT INTO top_sites VALUES ('https://b.example/', 1, 'B');
             INSERT INTO top_sites VALUES ('https://a.example/', 0, 'A');
             INSERT INTO top_sites VALUES ('chrome://newtab/', 2, 'New tab');",
        )
        .unwrap();
        let got = chromium_top_sites(&c).unwrap();
        assert_eq!(got.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    }
}
