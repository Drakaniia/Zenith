//! Incognito windows.
//!
//! An incognito window is another window of the running Zenith process (label `incognito-N`).
//! Its chrome UI shares the process and the WebView2 browser process with the normal window, so
//! it opens about as fast as a new window can paint. Its page webviews use WebView2's InPrivate
//! mode: cookies, cache and site storage stay in memory, are shared by all incognito windows
//! (as in other browsers) and are discarded by WebView2 once the last incognito tab closes. It
//! never records history or downloads, never saves its session or settings, and shares only the
//! bookmarks and the cached filter lists with the user's profile.
//!
//! Earlier versions ran each incognito window as a separate process on a throwaway profile in
//! `%TEMP%\zenith-incognito`. Folders left behind by those are still swept at startup, and the
//! `--purge-incognito` maintenance flag still works.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Command-line flag: open an incognito window (e.g. from the taskbar jump list).
pub const FLAG: &str = "--incognito";
/// Maintenance flag: delete a leftover incognito profile folder.
pub const PURGE_FLAG: &str = "--purge-incognito";
/// Window labels of incognito windows start with this.
pub const LABEL_PREFIX: &str = "incognito-";

const LOCK_FILE: &str = "instance.lock";

/// Parent folder of the old per-process incognito profiles.
pub fn root() -> PathBuf {
    std::env::temp_dir().join("zenith-incognito")
}

/// Whether the command line asks for an incognito window.
pub fn requested(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == FLAG)
}

/// Opens a new incognito window with the given web addresses. Windows are built off the UI
/// thread (Tauri requires that outside the event loop; WebView2 forbids it inside its event
/// handlers).
pub fn open_window(urls: Vec<String>) {
    let urls: Vec<String> = urls
        .into_iter()
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        .collect();
    let app = crate::host::app().clone();
    std::thread::spawn(move || {
        if let Err(e) = crate::open_incognito_window(&app, urls) {
            eprintln!("[zenith] could not open an incognito window: {e}");
        }
    });
}

/// `--purge-incognito <dir>`: deletes an old incognito profile, retrying while WebView2's
/// processes still hold its files. Only folders directly inside `root()` are ever touched.
pub fn purge_command(dir: &str) -> i32 {
    let dir = PathBuf::from(dir);
    if !is_profile_folder(&dir) {
        return 2;
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if try_remove(&dir) {
            return 0;
        }
        if Instant::now() > deadline {
            return 1;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Deletes old incognito profiles that no running instance holds. Runs on a background thread.
pub fn sweep_stale() {
    std::thread::spawn(|| {
        let Ok(entries) = fs::read_dir(root()) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_profile_folder(&path) {
                try_remove(&path);
            }
        }
    });
}

/// `<root>\<pid>-<millis>`: the only shape of folder the purge and the sweep delete.
fn is_profile_folder(dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else { return false };
    let shaped = name
        .split_once('-')
        .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit()));
    shaped && dir.parent() == Some(root().as_path()) && dir.is_dir()
}

/// Removes a profile unless an (older-version) instance still holds it. True once it is gone.
fn try_remove(dir: &Path) -> bool {
    match fs::remove_file(dir.join(LOCK_FILE)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        // Still locked: that window is open.
        Err(_) => return false,
    }
    match fs::remove_dir_all(dir) {
        Ok(()) => true,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    #[test]
    fn only_profile_shaped_folders_qualify() {
        let base = root();
        let _ = fs::create_dir_all(&base);
        let good = base.join("123-456");
        let _ = fs::create_dir_all(&good);
        assert!(is_profile_folder(&good));
        assert!(!is_profile_folder(&base.join("123-456-x")));
        assert!(!is_profile_folder(&base.join("..")));
        assert!(!is_profile_folder(&std::env::temp_dir()));
        assert!(!is_profile_folder(Path::new("C:\\Windows")));
        let nested = good.join("7-8");
        let _ = fs::create_dir_all(&nested);
        assert!(!is_profile_folder(&nested));
        assert!(try_remove(&good));
        assert!(!good.exists());
    }

    #[test]
    fn locked_profile_is_kept() {
        let dir = root().join("999999-1");
        fs::create_dir_all(&dir).unwrap();
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .share_mode(0)
            .open(dir.join(LOCK_FILE))
            .unwrap();
        assert!(!try_remove(&dir));
        assert!(dir.exists());
        drop(lock);
        assert!(try_remove(&dir));
    }

    #[test]
    fn flag_detection() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(requested(&args(&["zenith.exe", "--incognito"])));
        assert!(requested(&args(&["zenith.exe", "--incognito", "https://a.test"])));
        assert!(!requested(&args(&["--incognito"])));
        assert!(!requested(&args(&["zenith.exe", "https://a.test"])));
    }

    #[test]
    fn incognito_labels() {
        assert!(crate::host::is_private_window("incognito-1"));
        assert!(!crate::host::is_private_window(crate::host::MAIN));
    }
}
