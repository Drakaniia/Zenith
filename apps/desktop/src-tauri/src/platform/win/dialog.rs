//! Native Windows file and folder pickers via `IFileOpenDialog` / `IFileSaveDialog`.

use std::path::PathBuf;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT,
    FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

fn with_com<R>(f: impl FnOnce() -> Option<R>) -> Option<R> {
    let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let res = f();
    if init.is_ok() {
        unsafe { CoUninitialize() };
    }
    res
}

fn build_filters(filters: &[(&str, &[&str])]) -> (Vec<(HSTRING, HSTRING)>, Vec<COMDLG_FILTERSPEC>) {
    let owned: Vec<(HSTRING, HSTRING)> = filters
        .iter()
        .map(|(name, exts)| {
            let pattern = exts
                .iter()
                .map(|e| if *e == "*" { "*.*".to_string() } else { format!("*.{e}") })
                .collect::<Vec<_>>()
                .join(";");
            (HSTRING::from(*name), HSTRING::from(pattern))
        })
        .collect();
    let specs: Vec<COMDLG_FILTERSPEC> = owned
        .iter()
        .map(|(name, spec)| COMDLG_FILTERSPEC {
            pszName: PCWSTR(name.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        })
        .collect();
    (owned, specs)
}

pub fn pick_file(title: &str, filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    with_com(|| unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        let _ = dialog.SetTitle(&HSTRING::from(title));
        let opts = dialog.GetOptions().unwrap_or_default();
        let _ = dialog.SetOptions(opts | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST);
        let (_owned, specs) = build_filters(filters);
        if !specs.is_empty() {
            let _ = dialog.SetFileTypes(&specs);
        }
        if dialog.Show(None).is_err() {
            return None;
        }
        let item = dialog.GetResult().ok()?;
        let pwstr = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = pwstr.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(pwstr.0 as *const _));
        path
    })
}

pub fn pick_folder(title: Option<&str>) -> Option<PathBuf> {
    with_com(|| unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        if let Some(t) = title {
            let _ = dialog.SetTitle(&HSTRING::from(t));
        }
        let opts = dialog.GetOptions().unwrap_or_default();
        let _ = dialog.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST);
        if dialog.Show(None).is_err() {
            return None;
        }
        let item = dialog.GetResult().ok()?;
        let pwstr = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = pwstr.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(pwstr.0 as *const _));
        path
    })
}

pub fn save_file(title: &str, default_name: Option<&str>, filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    with_com(|| unsafe {
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL).ok()?;
        let _ = dialog.SetTitle(&HSTRING::from(title));
        if let Some(name) = default_name {
            let _ = dialog.SetFileName(&HSTRING::from(name));
        }
        let opts = dialog.GetOptions().unwrap_or_default();
        let _ = dialog.SetOptions(opts | FOS_FORCEFILESYSTEM | FOS_OVERWRITEPROMPT);
        let (_owned, specs) = build_filters(filters);
        if !specs.is_empty() {
            let _ = dialog.SetFileTypes(&specs);
            if let Some((_, exts)) = filters.first() {
                if let Some(ext) = exts.first().filter(|e| **e != "*") {
                    let _ = dialog.SetDefaultExtension(&HSTRING::from(*ext));
                }
            }
        }
        if dialog.Show(None).is_err() {
            return None;
        }
        let item = dialog.GetResult().ok()?;
        let pwstr = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = pwstr.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(pwstr.0 as *const _));
        path
    })
}
