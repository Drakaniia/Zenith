//! Profile pictures: the avatar kinds and the rendered 256 px PNG, the `.ico` made from it, how a
//! profile looks (`Look`), and writing, reading, removing and sweeping the files in `Avatars`.

use std::fs;
use std::path::PathBuf;

use crate::storage;

use super::*;

/// `image` (a picture the user chose) or `emoji:<name>` (a bundled emoji; lowercase ASCII
/// letters, digits and hyphens).
pub fn valid_avatar(avatar: &str) -> bool {
    if avatar == "image" {
        return true;
    }
    avatar.strip_prefix("emoji:").is_some_and(|n| {
        !n.is_empty() && n.len() <= 48 && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

/// A PNG of `ICON_SIZE` × `ICON_SIZE` pixels, at most 2 MB (the header is checked; the image
/// itself is only ever decoded by Windows and the PNG decoder).
pub fn valid_icon_png(png: &[u8]) -> bool {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if png.len() < 33 || png.len() > MAX_ICON_BYTES || !png.starts_with(SIGNATURE) || &png[12..16] != b"IHDR" {
        return false;
    }
    let dim = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    dim(16) == ICON_SIZE && dim(20) == ICON_SIZE
}

/// An `.ico` holding one 256 px PNG image (Windows Vista and later read PNG-compressed icons).
pub fn ico_from_png(png: &[u8]) -> Vec<u8> {
    let mut ico = Vec::with_capacity(22 + png.len());
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]); // reserved, type 1 (icon), 1 image
    // Width and height 0 mean 256; no palette; reserved; 1 plane; 32 bits per pixel.
    ico.extend_from_slice(&[0, 0, 0, 0, 1, 0, 32, 0]);
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes()); // the image follows the 6 + 16 byte header
    ico.extend_from_slice(png);
    ico
}

/// How a profile looks: its colour, its picture (see `valid_avatar`) and, for an emoji or an
/// image, the picture rendered by the UI (see `valid_icon_png`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Look<'a> {
    pub color: &'a str,
    pub avatar: Option<&'a str>,
    pub icon: Option<&'a [u8]>,
}

impl Look<'_> {
    pub(super) fn check(&self) -> Result<(), String> {
        if !valid_color(self.color) {
            return Err("That colour isn't valid.".into());
        }
        if self.avatar.is_some_and(|a| !valid_avatar(a)) || self.icon.is_some_and(|png| !valid_icon_png(png)) {
            return Err("That picture can't be used.".into());
        }
        if self.avatar == Some("image") && self.icon.is_none() {
            return Err("Choose the picture again.".into());
        }
        Ok(())
    }
}

/// Saves the rendered picture of profile `id` under a revision newer than `after`; returns the
/// revision, or 0 when the look has no rendered picture.
pub(super) fn write_icon(base: &Base, id: &str, after: i64, look: &Look) -> Result<i64, String> {
    let (Some(_), Some(png)) = (look.avatar, look.icon) else { return Ok(0) };
    let rev = now_ms().max(after + 1);
    let fail = |e: std::io::Error| format!("Could not save the profile picture: {e}");
    storage::write_atomic(&base.icon_file(id, rev, "png"), png).map_err(fail)?;
    storage::write_atomic(&base.icon_file(id, rev, "ico"), &ico_from_png(png)).map_err(fail)?;
    Ok(rev)
}

pub(super) fn remove_icon(base: &Base, id: &str, rev: i64) {
    if rev != 0 && valid_id(id) {
        let _ = fs::remove_file(base.icon_file(id, rev, "png"));
        let _ = fs::remove_file(base.icon_file(id, rev, "ico"));
    }
}

/// The profile's rendered picture (PNG), when it has an emoji or an image.
pub fn icon_png(base: &Base, p: &Profile) -> Option<Vec<u8>> {
    if p.avatar.is_none() || p.icon_rev == 0 || !valid_id(&p.id) {
        return None;
    }
    fs::read(base.icon_file(&p.id, p.icon_rev, "png")).ok().filter(|png| valid_icon_png(png))
}

/// The profile's `.ico`, when it has a rendered picture.
pub(super) fn icon_ico(base: &Base, p: &Profile) -> Option<PathBuf> {
    if p.avatar.is_none() || p.icon_rev == 0 || !valid_id(&p.id) {
        return None;
    }
    Some(base.icon_file(&p.id, p.icon_rev, "ico")).filter(|f| f.is_file())
}

/// Deletes pictures in `Avatars` that no profile uses any more (replaced while a save failed,
/// or left by a crash). Only `<id>-<rev>.png|ico|tmp` files are ever removed.
pub(super) fn sweep_icons(base: &Base, list: &List) {
    let Ok(entries) = fs::read_dir(base.avatar_dir()) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((stem, ext)) = name.rsplit_once('.') else { continue };
        let Some((id, rev)) = stem.rsplit_once('-') else { continue };
        let Ok(rev) = rev.parse::<i64>() else { continue };
        if !matches!(ext, "png" | "ico" | "tmp") || !valid_id(id) {
            continue;
        }
        let used = list.get(id).is_some_and(|p| p.icon_rev == rev && p.avatar.is_some());
        // Revisions are save times: a fresh file may belong to a save another process is making.
        let fresh = now_ms() - rev < 60_000;
        if !used && !fresh {
            let _ = fs::remove_file(entry.path());
        }
    }
}
