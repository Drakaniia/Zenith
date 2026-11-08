//! Profile processes: removing a deleted profile's folders and sweeping orphaned ones, the lock
//! file that marks a profile as running, and starting another profile's process.

use std::fs::{self, File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Mutex;

use super::*;

/// Removes both folders of a (non-default) profile. True once both are gone.
pub(super) fn remove_dirs(base: &Base, id: &str) -> bool {
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
pub(super) fn is_profile_dir(base: &Base, dir: &Path) -> bool {
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

// ---------------------------------------------------------------------------------------------
// Running instances

static HELD_LOCK: Mutex<Option<File>> = Mutex::new(None);

/// Marks this process as the one running the profile whose roaming folder is `roaming`.
pub(super) fn hold_lock(roaming: &Path) {
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
