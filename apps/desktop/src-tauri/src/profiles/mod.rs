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

pub mod commands;
mod icons;
mod launch;
mod shortcuts;
mod store;

pub use icons::*;
pub use launch::*;
pub use shortcuts::*;
pub use store::*;

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::storage::Paths;

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

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::Path;
    use std::time::{Duration, Instant};

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
