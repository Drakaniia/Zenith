//! The profile list: validating and repairing it, loading and saving `profiles.json`, and
//! creating, updating and deleting profiles.

use std::fs;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::storage;

use super::*;

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

pub(super) fn default_profile() -> Profile {
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
pub(super) fn modify<T>(base: &Base, f: impl FnOnce(&mut List) -> Result<T, String>) -> Result<T, String> {
    let _guard = LIST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = load(base);
    let out = f(&mut list)?;
    save(base, &list)?;
    Ok(out)
}

pub(super) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
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

pub(super) fn set_last_used(base: &Base, id: &str) {
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
