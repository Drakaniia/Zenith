//! Browser profiles.
//!
//! Every profile is fully separate: settings, session, history, bookmarks, passwords, cookies and
//! site data (its own WebView2 user data folder), extensions, filter lists and wallpapers. Each
//! profile runs in its own Zenith process, so profiles can be open side by side; a process serves
//! exactly one profile for its whole life (see `init`).
//!
//! Layout (`<base>` is `%APPDATA%\<identifier>` / `%LOCALAPPDATA%\<identifier>`, or the folders
//! inside `ZENITH_PROFILE_DIR`):
//! - `<base roaming>\profiles.json`: the list of profiles and the last one used.
//! - The Default profile is the data Zenith always had, right in the base folders (no migration).
//! - Other profiles live in `<base roaming>\Profiles\<id>` and `<base local>\Profiles\<id>`.
//!
//! `zenith --profile <id>` opens a profile; without the flag the last-used one opens (links from
//! other apps included). Each profile gets its own single-instance key (the app identifier is
//! suffixed, see `identifier`) and its own taskbar identity (`aumid`), so a second launch of a
//! profile goes to that profile's running process and its windows group on their own.
//!
//! Profile ids are generated slugs checked by `valid_id`; a name typed by the user never becomes
//! part of a path, and only `Profiles\<valid id>` folders are ever deleted.
//!
//! A profile's picture is its initial on its colour, a bundled emoji on its colour, or an image
//! the user chose. The UI renders the emoji and image kinds to a 256 px PNG (`valid_icon_png`),
//! kept as `<base roaming>\Avatars\<id>-<rev>.png` with an `.ico` next to it: every profile's
//! process can show it, and it becomes the window icon and the desktop shortcut's icon. The
//! revision changes with every new picture so Explorer's icon cache never shows an old one.

use std::fs::{self, File, OpenOptions};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::storage::{self, Paths};

/// The profile that holds the data of Zenith versions before profiles existed.
pub const DEFAULT: &str = "default";
/// Command-line flag choosing the profile: `--profile <id>` (or `--profile=<id>`).
pub const FLAG: &str = "--profile";

const LIST_FILE: &str = "profiles.json";
const SUBDIR: &str = "Profiles";
/// Held open without sharing while the profile's process runs (see `is_running`).
const LOCK_FILE: &str = "profile.lock";
const AVATAR_DIR: &str = "Avatars";
/// Size of the rendered profile picture (both sides).
pub const ICON_SIZE: u32 = 256;
const MAX_ICON_BYTES: usize = 2 * 1024 * 1024;
const MAX_PROFILES: usize = 24;
const MAX_NAME_CHARS: usize = 40;
const MAX_ID_LEN: usize = 32;

/// Colours offered for profiles (the first one is the Default profile's).
pub const COLORS: &[&str] = &["#8b7cf6", "#3b82f6", "#14b8a6", "#22c55e", "#f59e0b", "#f97316", "#ef4444", "#ec4899", "#64748b"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Creation time, ms since the epoch (0 for the Default profile).
    #[serde(default)]
    pub created: i64,
    /// Desktop shortcut Zenith created for this profile; deleted along with the profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<PathBuf>,
    /// The picture (see `valid_avatar`); `None` is the initial on the colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// Revision of the rendered picture in `Avatars` (see `Base::icon_file`); 0 when there is none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub icon_rev: i64,
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct List {
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Opened when Zenith starts without `--profile`.
    #[serde(default)]
    pub last_used: String,
}

impl List {
    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }
}

/// The folders holding the profile list and the Default profile.
#[derive(Clone, Debug)]
pub struct Base {
    pub roaming: PathBuf,
    pub local: PathBuf,
}

impl Base {
    fn list_file(&self) -> PathBuf {
        self.roaming.join(LIST_FILE)
    }

    fn avatar_dir(&self) -> PathBuf {
        self.roaming.join(AVATAR_DIR)
    }

    /// A rendered profile picture: `Avatars\<id>-<rev>.<ext>` (`png` or `ico`). `id` must be valid.
    pub fn icon_file(&self, id: &str, rev: i64, ext: &str) -> PathBuf {
        self.avatar_dir().join(format!("{id}-{rev}.{ext}"))
    }

    /// Data folders of a profile. `id` must be valid.
    pub fn paths(&self, id: &str) -> Paths {
        if id == DEFAULT {
            Paths { roaming: self.roaming.clone(), local: self.local.clone() }
        } else {
            Paths { roaming: self.roaming.join(SUBDIR).join(id), local: self.local.join(SUBDIR).join(id) }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Validation and repair

/// Lowercase ASCII letters, digits and inner hyphens, at most 32 characters.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

/// `#rrggbb`.
pub fn valid_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

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

/// Trimmed, single-line, at most 40 characters; `None` when nothing is left.
pub fn clean_name(name: &str) -> Option<String> {
    let flat: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let words: Vec<&str> = flat.split_whitespace().collect();
    let joined = words.join(" ");
    let cut: String = joined.chars().take(MAX_NAME_CHARS).collect();
    let cut = cut.trim().to_string();
    (!cut.is_empty()).then_some(cut)
}

/// A new id derived from the name: `work`, `work-2`, … (`profile` for names with no ASCII
/// letters or digits).
pub fn new_id(name: &str, list: &List) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(20);
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "profile" } else { slug };
    let taken = |id: &str| id == DEFAULT || list.get(id).is_some();
    if !taken(slug) {
        return slug.to_string();
    }
    (2..).map(|n| format!("{slug}-{n}")).find(|id| !taken(id)).expect("a free id")
}

fn default_profile() -> Profile {
    Profile { id: DEFAULT.into(), name: "Default".into(), color: COLORS[0].into(), created: 0, shortcut: None, avatar: None, icon_rev: 0 }
}

/// Fixes a list read from disk: drops bad or repeated ids, cleans names and colours, makes sure
/// the Default profile exists and comes first, and points `last_used` at an existing profile.
pub fn repair(mut list: List) -> List {
    let mut seen = std::collections::HashSet::new();
    let mut profiles = Vec::new();
    for (i, mut p) in std::mem::take(&mut list.profiles).into_iter().enumerate() {
        if !valid_id(&p.id) || !seen.insert(p.id.clone()) {
            continue;
        }
        p.name = clean_name(&p.name).unwrap_or_else(|| if p.id == DEFAULT { "Default".into() } else { "Profile".into() });
        if !valid_color(&p.color) {
            p.color = COLORS[i % COLORS.len()].into();
        }
        if p.shortcut.as_deref().is_some_and(|s| !is_shortcut_path(s)) {
            p.shortcut = None;
        }
        if p.avatar.as_deref().is_some_and(|a| !valid_avatar(a)) {
            p.avatar = None;
        }
        if p.avatar.is_none() || p.icon_rev < 0 {
            p.icon_rev = 0;
        }
        profiles.push(p);
    }
    match profiles.iter().position(|p| p.id == DEFAULT) {
        Some(0) => {}
        Some(i) => {
            let d = profiles.remove(i);
            profiles.insert(0, d);
        }
        None => profiles.insert(0, default_profile()),
    }
    list.profiles = profiles;
    if list.get(&list.last_used).is_none() {
        list.last_used = DEFAULT.into();
    }
    list
}

// ---------------------------------------------------------------------------------------------
// The list on disk

/// Serialises read-modify-write of the list within this process.
static LIST_LOCK: Mutex<()> = Mutex::new(());

pub fn load(base: &Base) -> List {
    let raw = storage::read_text(&base.list_file()).and_then(|s| serde_json::from_str::<List>(&s).ok());
    repair(raw.unwrap_or_default())
}

fn save(base: &Base, list: &List) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(list).map_err(|e| e.to_string())?;
    storage::write_atomic(&base.list_file(), &json).map_err(|e| format!("Could not save the profile list: {e}"))
}

/// Loads the current list, applies `f` and saves the result when `f` succeeds.
fn modify<T>(base: &Base, f: impl FnOnce(&mut List) -> Result<T, String>) -> Result<T, String> {
    let _guard = LIST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = load(base);
    let out = f(&mut list)?;
    save(base, &list)?;
    Ok(out)
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
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
    fn check(&self) -> Result<(), String> {
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
fn write_icon(base: &Base, id: &str, after: i64, look: &Look) -> Result<i64, String> {
    let (Some(_), Some(png)) = (look.avatar, look.icon) else { return Ok(0) };
    let rev = now_ms().max(after + 1);
    let fail = |e: std::io::Error| format!("Could not save the profile picture: {e}");
    storage::write_atomic(&base.icon_file(id, rev, "png"), png).map_err(fail)?;
    storage::write_atomic(&base.icon_file(id, rev, "ico"), &ico_from_png(png)).map_err(fail)?;
    Ok(rev)
}

fn remove_icon(base: &Base, id: &str, rev: i64) {
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
fn icon_ico(base: &Base, p: &Profile) -> Option<PathBuf> {
    if p.avatar.is_none() || p.icon_rev == 0 || !valid_id(&p.id) {
        return None;
    }
    Some(base.icon_file(&p.id, p.icon_rev, "ico")).filter(|f| f.is_file())
}

pub fn create(base: &Base, name: &str, look: Look) -> Result<Profile, String> {
    let name = clean_name(name).ok_or("Give the profile a name.")?;
    look.check()?;
    modify(base, |list| {
        if list.profiles.len() >= MAX_PROFILES {
            return Err(format!("Zenith supports up to {MAX_PROFILES} profiles."));
        }
        let id = new_id(&name, list);
        let icon_rev = write_icon(base, &id, 0, &look)?;
        let profile = Profile {
            id,
            name,
            color: look.color.to_string(),
            created: now_ms(),
            shortcut: None,
            avatar: look.avatar.map(str::to_string),
            icon_rev,
        };
        list.profiles.push(profile.clone());
        Ok(profile)
    })
}

pub fn update(base: &Base, id: &str, name: &str, look: Look) -> Result<Profile, String> {
    let name = clean_name(name).ok_or("Give the profile a name.")?;
    look.check()?;
    let (profile, old_rev) = modify(base, |list| {
        let p = list.profiles.iter_mut().find(|p| p.id == id).ok_or("That profile no longer exists.")?;
        let old_rev = p.icon_rev;
        p.icon_rev = write_icon(base, id, old_rev, &look)?;
        p.name = name;
        p.color = look.color.to_string();
        p.avatar = look.avatar.map(str::to_string);
        Ok((p.clone(), old_rev))
    })?;
    if old_rev != profile.icon_rev {
        remove_icon(base, id, old_rev);
    }
    Ok(profile)
}

fn set_last_used(base: &Base, id: &str) {
    let _ = modify(base, |list| {
        if list.last_used == id || list.get(id).is_none() {
            return Err(String::new());
        }
        list.last_used = id.to_string();
        Ok(())
    });
}

/// Deletes a profile that is not running: its entry, its desktop shortcut and (in the
/// background, retrying while WebView2 lets go of its files) its folders.
pub fn delete(base: &Base, id: &str, active: &str) -> Result<(), String> {
    if id == DEFAULT {
        return Err("The Default profile can't be deleted.".into());
    }
    if id == active {
        return Err("Switch to another profile before deleting this one.".into());
    }
    let removed = modify(base, |list| {
        let i = list.profiles.iter().position(|p| p.id == id).ok_or("That profile no longer exists.")?;
        if is_running(base, id) {
            return Err(format!("Close the windows of “{}” first.", list.profiles[i].name));
        }
        let p = list.profiles.remove(i);
        if list.last_used == id {
            list.last_used = DEFAULT.into();
        }
        Ok(p)
    })?;
    if let Some(link) = removed.shortcut.as_deref().filter(|s| is_shortcut_path(s)) {
        let _ = fs::remove_file(link);
    }
    remove_icon(base, id, removed.icon_rev);
    let base = base.clone();
    let id = id.to_string();
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !remove_dirs(&base, &id) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    Ok(())
}

/// Removes both folders of a (non-default) profile. True once both are gone.
fn remove_dirs(base: &Base, id: &str) -> bool {
    if id == DEFAULT || !valid_id(id) {
        return true;
    }
    let paths = base.paths(id);
    [paths.roaming, paths.local].iter().all(|dir| {
        if !is_profile_dir(base, dir) {
            return true;
        }
        match fs::remove_dir_all(dir) {
            Ok(()) => true,
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        }
    })
}

/// `<base>\Profiles\<valid id>` (never the Default profile's base folders).
fn is_profile_dir(base: &Base, dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else { return false };
    let parent = dir.parent();
    valid_id(name)
        && name != DEFAULT
        && (parent == Some(base.roaming.join(SUBDIR).as_path()) || parent == Some(base.local.join(SUBDIR).as_path()))
}

/// Deletes folders of profiles that are no longer listed (a delete interrupted by a crash or
/// by files WebView2 held too long). Runs on a background thread.
pub fn sweep_orphans(base: &Base) {
    let base = base.clone();
    std::thread::spawn(move || {
        let list = load(&base);
        for root in [base.roaming.join(SUBDIR), base.local.join(SUBDIR)] {
            let Ok(entries) = fs::read_dir(&root) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if valid_id(&name) && name != DEFAULT && list.get(&name).is_none() && !is_running(&base, &name) {
                    let dir = entry.path();
                    if is_profile_dir(&base, &dir) {
                        let _ = fs::remove_dir_all(dir);
                    }
                }
            }
        }
        sweep_icons(&base, &list);
    });
}

/// Deletes pictures in `Avatars` that no profile uses any more (replaced while a save failed,
/// or left by a crash). Only `<id>-<rev>.png|ico|tmp` files are ever removed.
fn sweep_icons(base: &Base, list: &List) {
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

// ---------------------------------------------------------------------------------------------
// Running instances

static HELD_LOCK: Mutex<Option<File>> = Mutex::new(None);

/// Marks this process as the one running the profile whose roaming folder is `roaming`.
fn hold_lock(roaming: &Path) {
    let _ = fs::create_dir_all(roaming);
    let lock = OpenOptions::new().write(true).create(true).truncate(false).share_mode(0).open(roaming.join(LOCK_FILE));
    if let (Ok(file), Ok(mut held)) = (lock, HELD_LOCK.lock()) {
        *held = Some(file);
    }
}

/// True while some Zenith process runs the profile.
pub fn is_running(base: &Base, id: &str) -> bool {
    if !valid_id(id) {
        return false;
    }
    let lock = base.paths(id).roaming.join(LOCK_FILE);
    match OpenOptions::new().read(true).share_mode(0).open(&lock) {
        Ok(_) => false,
        // Sharing violation: the profile's process holds it.
        Err(e) => e.kind() != std::io::ErrorKind::NotFound && e.raw_os_error() == Some(32),
    }
}

// ---------------------------------------------------------------------------------------------
// This process's profile

/// The profile this process runs.
pub struct Active {
    pub id: String,
    pub base: Base,
    /// The app identifier from tauri.conf.json (before `identifier` suffixes it).
    pub base_identifier: String,
}

static ACTIVE: OnceLock<Active> = OnceLock::new();

/// The profile this process runs. Panics before `init`.
pub fn active() -> &'static Active {
    ACTIVE.get().expect("profile initialised")
}

impl Active {
    pub fn paths(&self) -> Paths {
        self.base.paths(&self.id)
    }
    pub fn is_default(&self) -> bool {
        self.id == DEFAULT
    }
    /// This profile's entry (name and colour), read fresh from disk.
    pub fn profile(&self) -> Profile {
        load(&self.base).get(&self.id).cloned().unwrap_or_else(default_profile)
    }
}

/// The `--profile` value on a command line, if any.
pub fn requested(args: &[String]) -> Option<String> {
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == FLAG {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix("--profile=") {
            return Some(v.to_string());
        }
    }
    None
}

/// The command line without `--profile <id>` / `--profile=<id>` (so the id is never taken for a
/// web address).
pub fn without_flag(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut skip_next = false;
    for (i, a) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if i > 0 && a == FLAG {
            skip_next = true;
            continue;
        }
        if i > 0 && a.starts_with("--profile=") {
            continue;
        }
        out.push(a.clone());
    }
    out
}

/// Which profile a launch opens: the requested one when it exists, else the last one used.
/// A request for a profile that no longer exists (an old shortcut) opens the Default profile.
pub fn choose(list: &List, requested: Option<&str>) -> String {
    match requested {
        Some(id) if valid_id(id) && list.get(id).is_some() => id.to_string(),
        Some(id) => {
            eprintln!("[zenith] unknown profile {id:?}; opening the Default profile");
            DEFAULT.to_string()
        }
        None => list.last_used.clone(),
    }
}

/// Picks this process's profile (before Tauri starts) and records it as the last used.
/// `override_dir` is `ZENITH_PROFILE_DIR`: profiles then live inside that folder.
pub fn init(base_identifier: &str, args: &[String], override_dir: Option<PathBuf>) -> &'static Active {
    let base = match override_dir {
        Some(dir) => Base { roaming: dir.join("roaming"), local: dir.join("local") },
        None => Base {
            roaming: known_folder(&windows::Win32::UI::Shell::FOLDERID_RoamingAppData, "APPDATA").join(base_identifier),
            local: known_folder(&windows::Win32::UI::Shell::FOLDERID_LocalAppData, "LOCALAPPDATA").join(base_identifier),
        },
    };
    let list = load(&base);
    let id = choose(&list, requested(args).as_deref());
    set_last_used(&base, &id);
    let active = Active { id, base, base_identifier: base_identifier.to_string() };
    let paths = active.paths();
    let _ = fs::create_dir_all(&paths.roaming);
    let _ = fs::create_dir_all(&paths.local);
    hold_lock(&paths.roaming);
    let _ = ACTIVE.set(active);
    let active = self::active();
    if !active.is_default() {
        set_process_aumid(&aumid(&active.base_identifier, &active.id));
    }
    sweep_orphans(&active.base);
    active
}

/// The app identifier a profile's process runs under. The single-instance lock is keyed on it,
/// so each profile has its own. The Default profile keeps the original one (installed builds,
/// file associations and older versions all expect it).
pub fn identifier(base_identifier: &str, id: &str) -> String {
    if id == DEFAULT {
        base_identifier.to_string()
    } else {
        format!("{base_identifier}.profile.{id}")
    }
}

/// Taskbar identity (AppUserModelID) of a non-default profile, also set on its desktop shortcut.
pub fn aumid(base_identifier: &str, id: &str) -> String {
    identifier(base_identifier, id)
}

/// Whether another profile has a Zenith process running (an update must not be installed then:
/// the installer would close it without saving).
pub fn others_running() -> bool {
    let Some(active) = ACTIVE.get() else { return false };
    load(&active.base).profiles.iter().any(|p| p.id != active.id && is_running(&active.base, &p.id))
}

/// Starts (or, through its single-instance redirect, focuses) the process of profile `id`.
pub fn launch(id: &str) -> Result<(), String> {
    let active = active();
    if !valid_id(id) || load(&active.base).get(id).is_none() {
        return Err("That profile no longer exists.".into());
    }
    // A separate-profile (test) instance has no single-instance redirect: never start a second
    // process on the same profile.
    if crate::host::profile_override().is_some() && is_running(&active.base, id) {
        return Err("That profile is already open.".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Let the new (or the running) process bring its window to the front.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(windows::Win32::UI::WindowsAndMessaging::ASFW_ANY);
    }
    std::process::Command::new(exe)
        .arg(FLAG)
        .arg(id)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start Zenith: {e}"))
}

// ---------------------------------------------------------------------------------------------
// Windows shell

fn known_folder(id: &windows::core::GUID, env: &str) -> PathBuf {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, KNOWN_FOLDER_FLAG};
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None) {
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            if let Some(s) = s.filter(|s| !s.is_empty()) {
                return PathBuf::from(s);
            }
        }
    }
    std::env::var_os(env).map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

fn set_process_aumid(id: &str) {
    unsafe {
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(&windows::core::HSTRING::from(id));
    }
}

/// A `.lnk` file: the only kind of file `delete` removes on a profile's behalf.
fn is_shortcut_path(p: &Path) -> bool {
    p.is_absolute() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
}

/// File name of a profile's desktop shortcut: `Zenith - <name>.lnk`, without characters
/// Windows forbids in file names.
pub fn shortcut_file_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { ' ' } else { c })
        .collect();
    let safe = safe.split_whitespace().collect::<Vec<_>>().join(" ");
    let safe = safe.trim_end_matches(['.', ' ']);
    if safe.is_empty() {
        "Zenith.lnk".into()
    } else {
        format!("Zenith - {safe}.lnk")
    }
}

/// Creates (or replaces) a desktop shortcut that opens profile `id`, and remembers it so it is
/// deleted with the profile. Returns its path.
pub fn create_shortcut(id: &str) -> Result<PathBuf, String> {
    let active = active();
    let profile = load(&active.base).get(id).cloned().ok_or("That profile no longer exists.")?;
    let desktop = known_folder(&windows::Win32::UI::Shell::FOLDERID_Desktop, "USERPROFILE");
    let link = desktop.join(shortcut_file_name(&profile.name));
    write_profile_shortcut(active, &profile, &link)?;
    let old = modify(&active.base, |list| {
        let p = list.profiles.iter_mut().find(|p| p.id == id).ok_or("That profile no longer exists.")?;
        Ok(p.shortcut.replace(link.clone()))
    })?;
    // A rename since the last shortcut leaves the old one pointing at the same profile: replace it.
    if let Some(old) = old.filter(|o| *o != link && is_shortcut_path(o)) {
        let _ = fs::remove_file(old);
    }
    Ok(link)
}

/// Rewrites the profile's existing desktop shortcut (same file) so it shows the profile's
/// current picture. Does nothing when the profile has no shortcut on disk.
pub fn refresh_shortcut(id: &str) -> Result<(), String> {
    let active = active();
    let Some(profile) = load(&active.base).get(id).cloned() else { return Ok(()) };
    let Some(link) = profile.shortcut.clone().filter(|s| is_shortcut_path(s) && s.is_file()) else { return Ok(()) };
    write_profile_shortcut(active, &profile, &link)
}

fn write_profile_shortcut(active: &Active, profile: &Profile, link: &Path) -> Result<(), String> {
    let id = profile.id.as_str();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let icon = icon_ico(&active.base, profile).unwrap_or_else(|| exe.clone());
    let aumid = (id != DEFAULT).then(|| aumid(&active.base_identifier, id));
    write_shortcut(link, &exe, &icon, &format!("{FLAG} {id}"), &format!("Zenith ({})", profile.name), aumid.as_deref())
        .map_err(|e| format!("Could not create the shortcut: {e}"))?;
    notify_shell_changed(link);
    Ok(())
}

/// Tells Explorer a file changed, so the desktop redraws a shortcut's new icon.
fn notify_shell_changed(path: &Path) {
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_UPDATEITEM, SHCNF_PATHW};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        SHChangeNotify(SHCNE_UPDATEITEM, SHCNF_PATHW, Some(wide.as_ptr() as *const _), None);
    }
}

fn write_shortcut(link: &Path, exe: &Path, icon: &Path, args: &str, description: &str, aumid: Option<&str>) -> windows::core::Result<()> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    unsafe {
        let inited = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| {
            let shell: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            shell.SetPath(&HSTRING::from(exe.as_os_str()))?;
            shell.SetArguments(&HSTRING::from(args))?;
            shell.SetDescription(&HSTRING::from(description))?;
            shell.SetIconLocation(&HSTRING::from(icon.as_os_str()), 0)?;
            if let Some(dir) = exe.parent() {
                shell.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
            }
            if let Some(id) = aumid {
                let props: IPropertyStore = shell.cast()?;
                props.SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::from(id))?;
                props.Commit()?;
            }
            let file: IPersistFile = shell.cast()?;
            file.Save(&HSTRING::from(link.as_os_str()), true)
        })();
        if inited {
            CoUninitialize();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base(tag: &str) -> Base {
        let dir = std::env::temp_dir().join(format!("zenith-profiles-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Base { roaming: dir.join("roaming"), local: dir.join("local") }
    }

    fn cleanup(base: &Base) {
        if let Some(dir) = base.roaming.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }

    fn look(color: &str) -> Look<'_> {
        Look { color, ..Default::default() }
    }

    fn profile(id: &str, name: &str) -> Profile {
        Profile { id: id.into(), name: name.into(), color: COLORS[1].into(), created: 1, shortcut: None, avatar: None, icon_rev: 0 }
    }

    /// The start of a PNG of `w` × `h` pixels (enough for `valid_icon_png`).
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0, 1, 2, 3, 4]);
        v
    }

    #[test]
    fn ids() {
        assert!(valid_id("default"));
        assert!(valid_id("work-2"));
        assert!(!valid_id(""));
        assert!(!valid_id("-a"));
        assert!(!valid_id("a-"));
        assert!(!valid_id("Work"));
        assert!(!valid_id(".."));
        assert!(!valid_id("a\\b"));
        assert!(!valid_id(&"a".repeat(33)));
    }

    #[test]
    fn names_colors_and_new_ids() {
        assert_eq!(clean_name("  Work \n  stuff\t"), Some("Work stuff".into()));
        assert_eq!(clean_name(" \t "), None);
        assert_eq!(clean_name(&"x".repeat(60)).unwrap().chars().count(), 40);
        assert!(valid_color("#a1B2c3"));
        assert!(!valid_color("red"));
        assert!(!valid_color("#12345g"));
        let list = repair(List::default());
        assert_eq!(new_id("Work", &list), "work");
        assert_eq!(new_id("Default", &list), "default-2");
        assert_eq!(new_id("École & Café!", &list), "cole-caf");
        assert_eq!(new_id("工作", &list), "profile");
        let mut list = list;
        list.profiles.push(profile("work", "Work"));
        assert_eq!(new_id("work", &list), "work-2");
        assert!(valid_id(&new_id(&"long name ".repeat(10), &list)));
    }

    #[test]
    fn repair_fixes_bad_lists() {
        let raw = r##"{"profiles":[
            {"id":"work","name":"  Work ","color":"blue"},
            {"id":"../evil","name":"x","color":"#000000"},
            {"id":"work","name":"dupe","color":"#000000"},
            {"id":"default","name":"","color":"#111111","shortcut":"relative.lnk"}
        ],"lastUsed":"gone"}"##;
        let list = repair(serde_json::from_str(raw).unwrap());
        let ids: Vec<&str> = list.profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["default", "work"]);
        assert_eq!(list.profiles[0].name, "Default");
        assert_eq!(list.profiles[0].shortcut, None);
        assert_eq!(list.profiles[1].name, "Work");
        assert!(valid_color(&list.profiles[1].color));
        assert_eq!(list.last_used, DEFAULT);
        // An empty or missing list is just the Default profile.
        let empty = repair(List::default());
        assert_eq!(empty.profiles, vec![default_profile()]);
    }

    #[test]
    fn default_profile_keeps_existing_folders() {
        let base = temp_base("paths");
        let d = base.paths(DEFAULT);
        assert_eq!(d.roaming, base.roaming);
        assert_eq!(d.local, base.local);
        assert_eq!(d.db(), base.roaming.join("zenith.db"));
        let w = base.paths("work");
        assert_eq!(w.roaming, base.roaming.join("Profiles").join("work"));
        assert_eq!(w.webview_data(), base.local.join("Profiles").join("work").join("WebView2"));
        assert!(is_profile_dir(&base, &w.roaming));
        assert!(is_profile_dir(&base, &w.local));
        assert!(!is_profile_dir(&base, &base.roaming));
        assert!(!is_profile_dir(&base, &base.roaming.join("Profiles").join("default")));
        assert!(!is_profile_dir(&base, &base.roaming.join("other").join("work")));
    }

    #[test]
    fn choosing_a_profile() {
        let mut list = repair(List::default());
        list.profiles.push(profile("work", "Work"));
        list.last_used = "work".into();
        assert_eq!(choose(&list, None), "work");
        assert_eq!(choose(&list, Some("default")), DEFAULT);
        assert_eq!(choose(&list, Some("gone")), DEFAULT);
        assert_eq!(choose(&list, Some("..\\x")), DEFAULT);
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(requested(&args(&["zenith.exe", "--profile", "work"])), Some("work".into()));
        assert_eq!(requested(&args(&["zenith.exe", "https://a.test", "--profile=work"])), Some("work".into()));
        assert_eq!(requested(&args(&["zenith.exe", "https://a.test"])), None);
        assert_eq!(requested(&args(&["--profile", "work"])), None);
        assert_eq!(
            without_flag(&args(&["zenith.exe", "--profile", "work", "https://a.test", "--profile=x"])),
            args(&["zenith.exe", "https://a.test"])
        );
    }

    #[test]
    fn identifiers() {
        assert_eq!(identifier("dev.zenith.browser", DEFAULT), "dev.zenith.browser");
        assert_eq!(identifier("dev.zenith.browser", "work"), "dev.zenith.browser.profile.work");
    }

    #[test]
    fn create_update_delete() {
        let base = temp_base("crud");
        let work = create(&base, " Work ", look("#3b82f6")).unwrap();
        assert_eq!((work.id.as_str(), work.name.as_str()), ("work", "Work"));
        assert!(create(&base, "  ", look("#3b82f6")).is_err());
        assert!(create(&base, "X", look("blue")).is_err());
        let renamed = update(&base, "work", "Job", look("#ef4444")).unwrap();
        assert_eq!(renamed.name, "Job");
        assert_eq!(load(&base).get("work").unwrap().color, "#ef4444");

        // Data folders appear once the profile runs.
        let paths = base.paths("work");
        fs::create_dir_all(&paths.roaming).unwrap();
        fs::create_dir_all(paths.local.join("WebView2")).unwrap();
        fs::write(paths.roaming.join("settings.json"), "{}").unwrap();

        assert!(delete(&base, DEFAULT, "work").is_err());
        assert!(delete(&base, "work", "work").is_err());

        // A running profile can't be deleted.
        let lock = OpenOptions::new().write(true).create(true).truncate(false).share_mode(0).open(paths.roaming.join(LOCK_FILE)).unwrap();
        assert!(is_running(&base, "work"));
        assert!(delete(&base, "work", DEFAULT).is_err());
        assert!(load(&base).get("work").is_some());
        drop(lock);
        assert!(!is_running(&base, "work"));

        set_last_used(&base, "work");
        assert_eq!(load(&base).last_used, "work");
        delete(&base, "work", DEFAULT).unwrap();
        let list = load(&base);
        assert!(list.get("work").is_none());
        assert_eq!(list.last_used, DEFAULT);
        // The folders go on a background thread.
        let deadline = Instant::now() + Duration::from_secs(5);
        while (paths.roaming.exists() || paths.local.exists()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!paths.roaming.exists() && !paths.local.exists());
        assert!(remove_dirs(&base, "work"));
        // The base (Default profile) folders are never touched.
        assert!(base.roaming.join(LIST_FILE).exists());
        cleanup(&base);
    }

    #[test]
    fn orphans_are_swept_but_listed_and_default_folders_stay() {
        let base = temp_base("sweep");
        create(&base, "Kept", look("#3b82f6")).unwrap();
        let kept = base.paths("kept");
        let orphan = base.paths("orphan");
        for d in [&kept.roaming, &kept.local, &orphan.roaming, &orphan.local] {
            fs::create_dir_all(d).unwrap();
        }
        let odd = base.roaming.join("Profiles").join("Not Valid");
        fs::create_dir_all(&odd).unwrap();
        sweep_orphans(&base);
        let deadline = Instant::now() + Duration::from_secs(5);
        while (orphan.roaming.exists() || orphan.local.exists()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!orphan.roaming.exists() && !orphan.local.exists());
        assert!(kept.roaming.exists() && kept.local.exists());
        assert!(odd.exists());
        cleanup(&base);
    }

    #[test]
    fn writes_a_shortcut_with_its_own_taskbar_identity() {
        let base = temp_base("lnk");
        fs::create_dir_all(&base.roaming).unwrap();
        let link = base.roaming.join("Zenith - Work.lnk");
        let exe = std::env::current_exe().unwrap();
        write_shortcut(&link, &exe, &exe, "--profile work", "Zenith (Work)", Some("dev.zenith.browser.profile.work")).unwrap();
        let bytes = fs::read(&link).unwrap();
        // Shell link header (0x4C) and the arguments stored as UTF-16.
        assert_eq!(bytes[0], 0x4C);
        let wide: Vec<u8> = "--profile work".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert!(bytes.windows(wide.len()).any(|w| w == wide.as_slice()));
        cleanup(&base);
    }

    #[test]
    fn avatars_and_icons_are_validated() {
        assert!(valid_avatar("image"));
        assert!(valid_avatar("emoji:cat-face"));
        assert!(!valid_avatar("emoji:"));
        assert!(!valid_avatar("emoji:Cat"));
        assert!(!valid_avatar("emoji:../x"));
        assert!(!valid_avatar("letter"));
        assert!(valid_icon_png(&png(256, 256)));
        assert!(!valid_icon_png(&png(255, 256)));
        assert!(!valid_icon_png(&png(256, 4096)));
        assert!(!valid_icon_png(b"GIF89a not a png at all, but long enough"));
        let mut huge = png(256, 256);
        huge.resize(MAX_ICON_BYTES + 1, 0);
        assert!(!valid_icon_png(&huge));

        let p = png(256, 256);
        let ico = ico_from_png(&p);
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 1, 0]);
        assert_eq!(u32::from_le_bytes(ico[14..18].try_into().unwrap()) as usize, p.len());
        assert_eq!(u32::from_le_bytes(ico[18..22].try_into().unwrap()), 22);
        assert_eq!(&ico[22..], p.as_slice());

        // Bad pictures in a list read from disk fall back to the initial.
        let raw = r##"{"profiles":[{"id":"work","name":"W","color":"#000000","avatar":"emoji:../x","iconRev":5}]}"##;
        let list = repair(serde_json::from_str(raw).unwrap());
        assert_eq!((list.profiles[1].avatar.as_deref(), list.profiles[1].icon_rev), (None, 0));
    }

    #[test]
    fn pictures_are_saved_replaced_and_removed() {
        let base = temp_base("icons");
        let icon = png(256, 256);
        let emoji = Look { color: "#3b82f6", avatar: Some("emoji:fox"), icon: Some(&icon) };
        assert!(create(&base, "X", Look { color: "#3b82f6", avatar: Some("image"), icon: None }).is_err());
        assert!(create(&base, "X", Look { icon: Some(&png(64, 64)), ..emoji }).is_err());

        let work = create(&base, "Work", emoji).unwrap();
        assert_eq!(work.avatar.as_deref(), Some("emoji:fox"));
        let first = base.icon_file("work", work.icon_rev, "png");
        assert!(first.is_file() && base.icon_file("work", work.icon_rev, "ico").is_file());
        assert_eq!(icon_png(&base, &work), Some(icon.clone()));
        assert!(icon_ico(&base, &work).is_some());

        // A new picture gets a new revision; the old files go.
        let image = update(&base, "work", "Work", Look { avatar: Some("image"), ..emoji }).unwrap();
        assert!(image.icon_rev > work.icon_rev);
        assert!(!first.exists());
        assert!(base.icon_file("work", image.icon_rev, "png").is_file());

        // Back to the initial: no picture is kept.
        let plain = update(&base, "work", "Work", look("#3b82f6")).unwrap();
        assert_eq!((plain.avatar, plain.icon_rev), (None, 0));
        assert!(!base.icon_file("work", image.icon_rev, "png").exists());
        assert_eq!(icon_png(&base, &load(&base).get("work").unwrap().clone()), None);

        // Stale pictures are swept; current ones, fresh ones and other files stay.
        let again = update(&base, "work", "Work", emoji).unwrap();
        let current = base.icon_file("work", again.icon_rev, "png");
        let stale = base.icon_file("work", 1, "png");
        let gone = base.icon_file("gone", 2, "ico");
        let fresh = base.icon_file("other", now_ms(), "png");
        let odd = base.avatar_dir().join("notes.txt");
        for f in [&stale, &gone, &fresh, &odd] {
            fs::write(f, b"x").unwrap();
        }
        sweep_icons(&base, &load(&base));
        assert!(current.exists() && fresh.exists() && odd.exists());
        assert!(!stale.exists() && !gone.exists());

        delete(&base, "work", DEFAULT).unwrap();
        assert!(!current.exists());
        cleanup(&base);
    }

    #[test]
    fn shortcut_names() {
        assert_eq!(shortcut_file_name("Work"), "Zenith - Work.lnk");
        assert_eq!(shortcut_file_name("a/b:c*?"), "Zenith - a b c.lnk");
        assert_eq!(shortcut_file_name("dots..."), "Zenith - dots.lnk");
        assert_eq!(shortcut_file_name("???"), "Zenith.lnk");
        assert!(is_shortcut_path(Path::new("C:\\Users\\x\\Desktop\\Zenith - Work.lnk")));
        assert!(!is_shortcut_path(Path::new("C:\\Users\\x\\Desktop\\file.txt")));
        assert!(!is_shortcut_path(Path::new("rel.lnk")));
    }
}
