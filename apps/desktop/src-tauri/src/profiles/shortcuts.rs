//! The Windows shell: desktop shortcuts (`.lnk`) for profiles with their own taskbar identity
//! (AppUserModelID), the process's AUMID, and telling Explorer a shortcut changed.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::*;

// ---------------------------------------------------------------------------------------------
// Windows shell

pub(super) fn known_folder(id: &windows::core::GUID, env: &str) -> PathBuf {
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

pub(super) fn set_process_aumid(id: &str) {
    unsafe {
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(&windows::core::HSTRING::from(id));
    }
}

/// A `.lnk` file: the only kind of file `delete` removes on a profile's behalf.
pub(super) fn is_shortcut_path(p: &Path) -> bool {
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

pub(super) fn write_shortcut(link: &Path, exe: &Path, icon: &Path, args: &str, description: &str, aumid: Option<&str>) -> windows::core::Result<()> {
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
