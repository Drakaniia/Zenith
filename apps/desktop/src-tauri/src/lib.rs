//! Zenith: a lightweight Zen-style browser for Windows (Win32 + bundled CEF).
//!
//! Built as a `cdylib` (`zenith.dll`) loaded by CEF's `bootstrap.exe` (renamed to `zenith.exe`)
//! so Chromium's Windows sandbox runs with `no_sandbox = 0`.

pub mod app;
pub mod blocker;
pub mod bookmarks;
pub mod downloads;
pub mod extensions;
pub mod history;
pub mod host;
pub mod importer;
pub mod incognito;
pub mod media;
pub mod notes;
pub mod pdf;
pub mod platform;
pub mod profiles;
pub mod search;
pub mod security;
pub mod session;
pub mod settings;
pub mod shortcuts;
pub mod storage;
pub mod torrents;
pub mod updates;
pub mod vault;

use std::ffi::c_void;

/// Entry point called by CEF's Windows GUI bootstrapper (`bootstrap.exe` -> `zenith.exe`) with
/// the initialized Windows sandbox handle (`sandbox_info`).
#[no_mangle]
pub unsafe extern "C" fn RunWinMain(
    h_instance: *mut c_void,
    _lp_cmd_line: *mut u16,
    _n_cmd_show: i32,
    sandbox_info: *mut c_void,
    _version_info: *mut c_void,
) -> i32 {
    app::run_cef(h_instance, sandbox_info)
}

/// Entry point called by CEF's console bootstrapper (`bootstrapc.exe`).
#[no_mangle]
pub unsafe extern "C" fn RunConsoleMain(
    _argc: i32,
    _argv: *mut *mut std::os::raw::c_char,
    sandbox_info: *mut c_void,
    _version_info: *mut c_void,
) -> i32 {
    let h_instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
        .map(|h| h.0)
        .unwrap_or(std::ptr::null_mut());
    app::run_cef(h_instance, sandbox_info)
}
