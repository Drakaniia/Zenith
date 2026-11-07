//! Windows platform services: default-browser registration, shell integration, memory stats.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Registry::*;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Shell::{
    SHChangeNotify, ShellExecuteW, SHCNE_ASSOCCHANGED, SHCNF_IDLIST,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub const APP_NAME: &str = "Zenith";
const PROG_HTML: &str = "ZenithHTML";
const PROG_URL: &str = "ZenithURL";

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn create_key(path: &str) -> Result<Key, String> {
    let mut hkey = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
    };
    status
        .ok()
        .map(|_| Key(hkey))
        .map_err(|e| format!("create HKCU\\{path}: {e}"))
}

fn set_str(key: &Key, name: Option<&str>, value: &str) -> Result<(), String> {
    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
    let name = name.map(HSTRING::from);
    let name_ptr = name
        .as_ref()
        .map(|h| PCWSTR(h.as_ptr()))
        .unwrap_or(PCWSTR::null());
    unsafe { RegSetValueExW(key.0, name_ptr, None, REG_SZ, Some(bytes)) }
        .ok()
        .map_err(|e| format!("set registry value: {e}"))
}

fn read_str(path: &str, name: &str) -> Option<String> {
    let mut buf = vec![0u16; 512];
    let mut len = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            &HSTRING::from(name),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut len),
        )
    };
    if status.is_err() {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..chars.min(buf.len())]))
}

/// Registers Zenith as a browser candidate for the current user (no elevation required):
/// URL protocols http/https, HTML-ish file types, and the Capabilities keys that make it
/// appear under Settings > Apps > Default apps.
pub fn register_browser(exe: &Path) -> Result<(), String> {
    let exe = exe.display().to_string();
    let open = format!("\"{exe}\" \"%1\"");
    let icon = format!("{exe},0");
    for (prog, desc, is_url) in [
        (PROG_HTML, "Zenith HTML Document", false),
        (PROG_URL, "Zenith URL", true),
    ] {
        let base = format!("Software\\Classes\\{prog}");
        let k = create_key(&base)?;
        set_str(&k, None, desc)?;
        set_str(&k, Some("FriendlyTypeName"), desc)?;
        if is_url {
            set_str(&k, Some("URL Protocol"), "")?;
        }
        let k = create_key(&format!("{base}\\Application"))?;
        set_str(&k, Some("ApplicationName"), APP_NAME)?;
        set_str(&k, Some("ApplicationIcon"), &icon)?;
        set_str(&k, Some("ApplicationDescription"), "A lightweight Zen-style web browser")?;
        let k = create_key(&format!("{base}\\DefaultIcon"))?;
        set_str(&k, None, &icon)?;
        let k = create_key(&format!("{base}\\shell\\open\\command"))?;
        set_str(&k, None, &open)?;
    }

    let client = format!("Software\\Clients\\StartMenuInternet\\{APP_NAME}");
    let k = create_key(&client)?;
    set_str(&k, None, APP_NAME)?;
    let k = create_key(&format!("{client}\\DefaultIcon"))?;
    set_str(&k, None, &icon)?;
    let k = create_key(&format!("{client}\\shell\\open\\command"))?;
    set_str(&k, None, &format!("\"{exe}\""))?;

    let caps = format!("{client}\\Capabilities");
    let k = create_key(&caps)?;
    set_str(&k, Some("ApplicationName"), APP_NAME)?;
    set_str(&k, Some("ApplicationDescription"), "A lightweight Zen-style web browser")?;
    set_str(&k, Some("ApplicationIcon"), &icon)?;
    let k = create_key(&format!("{caps}\\StartMenu"))?;
    set_str(&k, Some("StartMenuInternet"), APP_NAME)?;
    let k = create_key(&format!("{caps}\\URLAssociations"))?;
    for scheme in ["http", "https"] {
        set_str(&k, Some(scheme), PROG_URL)?;
    }
    let k = create_key(&format!("{caps}\\FileAssociations"))?;
    for ext in [".htm", ".html", ".shtml", ".xhtml", ".svg", ".pdf"] {
        set_str(&k, Some(ext), PROG_HTML)?;
    }
    let k = create_key("Software\\RegisteredApplications")?;
    set_str(&k, Some(APP_NAME), &caps)?;

    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    Ok(())
}

/// Removes everything `register_browser` wrote.
pub fn unregister_browser() {
    unsafe {
        for path in [
            format!("Software\\Classes\\{PROG_HTML}"),
            format!("Software\\Classes\\{PROG_URL}"),
            format!("Software\\Clients\\StartMenuInternet\\{APP_NAME}"),
        ] {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path.as_str()));
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(path.as_str()));
        }
        if let Ok(k) = open_key("Software\\RegisteredApplications") {
            let _ = RegDeleteValueW(k.0, &HSTRING::from(APP_NAME));
        }
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

fn open_key(path: &str) -> Result<Key, String> {
    let mut hkey = HKEY::default();
    unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), None, KEY_WRITE, &mut hkey) }
        .ok()
        .map(|_| Key(hkey))
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultBrowserStatus {
    pub registered: bool,
    pub is_default: bool,
}

pub fn default_browser_status() -> DefaultBrowserStatus {
    let registered = read_str("Software\\RegisteredApplications", APP_NAME).is_some();
    let choice = |scheme: &str| {
        read_str(
            &format!(
                "Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\{scheme}\\UserChoice"
            ),
            "ProgId",
        )
    };
    let is_default = choice("https").as_deref() == Some(PROG_URL) && choice("http").as_deref() == Some(PROG_URL);
    DefaultBrowserStatus { registered, is_default }
}

/// Opens Windows Settings at Zenith's default-apps page (Windows does not allow apps to
/// change the default browser silently).
pub fn open_default_apps_settings() -> Result<(), String> {
    shell_open(&format!("ms-settings:defaultapps?registeredAppUser={APP_NAME}"))
}

pub fn shell_open(target: &str) -> Result<(), String> {
    let result = unsafe {
        ShellExecuteW(
            None::<HWND>,
            &HSTRING::from("open"),
            &HSTRING::from(target),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!("could not open {target}"))
    }
}

pub fn show_in_folder(path: &str) -> Result<(), String> {
    if !Path::new(path).exists() {
        return Err("file no longer exists".into());
    }
    let args = format!("/select,\"{path}\"");
    let result = unsafe {
        ShellExecuteW(
            None::<HWND>,
            &HSTRING::from("open"),
            &HSTRING::from("explorer.exe"),
            &HSTRING::from(args.as_str()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("could not open Explorer".into())
    }
}

// ---------------------------------------------------------------------------------------------
// Memory

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMemory {
    pub pid: u32,
    pub name: String,
    pub private_bytes: u64,
    pub working_set: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStats {
    pub private_bytes: u64,
    pub working_set: u64,
    pub processes: Vec<ProcessMemory>,
    pub system_load: u32,
}

/// Sums memory for Zenith and every process it spawned (the WebView2 browser process and its
/// renderer, GPU and utility children).
pub fn memory_stats() -> MemoryStats {
    let me = std::process::id();
    let mut parents: HashMap<u32, (u32, String)> = HashMap::new();
    unsafe {
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                    parents.insert(entry.th32ProcessID, (entry.th32ParentProcessID, name));
                    if Process32NextW(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
    }
    let mut family: HashSet<u32> = HashSet::from([me]);
    loop {
        let before = family.len();
        for (pid, (ppid, _)) in &parents {
            if family.contains(ppid) && *pid != 0 {
                family.insert(*pid);
            }
        }
        if family.len() == before {
            break;
        }
    }
    let mut processes = Vec::new();
    for pid in family {
        let name = parents.get(&pid).map(|p| p.1.clone()).unwrap_or_default();
        unsafe {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                continue;
            };
            let mut counters = PROCESS_MEMORY_COUNTERS_EX {
                cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                ..Default::default()
            };
            if GetProcessMemoryInfo(
                handle,
                &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                counters.cb,
            )
            .is_ok()
            {
                processes.push(ProcessMemory {
                    pid,
                    name,
                    private_bytes: counters.PrivateUsage as u64,
                    working_set: counters.WorkingSetSize as u64,
                });
            }
            let _ = CloseHandle(handle);
        }
    }
    processes.sort_by(|a, b| b.private_bytes.cmp(&a.private_bytes));
    MemoryStats {
        private_bytes: processes.iter().map(|p| p.private_bytes).sum(),
        working_set: processes.iter().map(|p| p.working_set).sum(),
        processes,
        system_load: system_memory_load(),
    }
}

/// Percentage of physical memory in use system-wide (0-100).
pub fn system_memory_load() -> u32 {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe {
        if GlobalMemoryStatusEx(&mut status).is_ok() {
            status.dwMemoryLoad
        } else {
            0
        }
    }
}

pub fn is_windows_11() -> bool {
    windows_version::OsVersion::current().build >= 22000
}

/// Puts Unicode text on the clipboard (works even when a page webview has focus).
pub fn set_clipboard_text(text: &str) -> Result<(), String> {
    put_clipboard(text, false).map(|_| ())
}

/// Copies a password: kept out of Windows clipboard history and cloud sync, and cleared after
/// `clear_after` unless something else was copied meanwhile.
pub fn copy_secret(text: &str, clear_after: std::time::Duration) -> Result<(), String> {
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard};
    let seq = put_clipboard(text, true)?;
    std::thread::spawn(move || {
        std::thread::sleep(clear_after);
        unsafe {
            if GetClipboardSequenceNumber() == seq && OpenClipboard(None).is_ok() {
                let _ = EmptyClipboard();
                let _ = CloseClipboard();
            }
        }
    });
    Ok(())
}

/// Returns the clipboard sequence number after the write.
fn put_clipboard(text: &str, secret: bool) -> Result<u32, String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    const CF_UNICODETEXT: u32 = 13;
    let mut wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe fn global(bytes: &[u8]) -> Result<HANDLE, String> {
        unsafe {
            let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).map_err(|e| e.to_string())?;
            let ptr = GlobalLock(mem) as *mut u8;
            if ptr.is_null() {
                return Err("could not lock clipboard memory".to_string());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            let _ = GlobalUnlock(mem);
            Ok(HANDLE(mem.0))
        }
    }
    let result = unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        let result = (|| {
            EmptyClipboard().map_err(|e| e.to_string())?;
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            SetClipboardData(CF_UNICODETEXT, Some(global(bytes)?)).map_err(|e| e.to_string())?;
            if secret {
                // Documented opt-outs for clipboard history, cloud clipboard and clipboard monitors.
                for (name, value) in [
                    ("CanIncludeInClipboardHistory", Some(0u32)),
                    ("CanUploadToCloudClipboard", Some(0u32)),
                    ("ExcludeClipboardContentFromMonitorProcessing", None),
                ] {
                    let format = RegisterClipboardFormatW(&windows::core::HSTRING::from(name));
                    if format != 0 {
                        let data = value.map(|v| v.to_le_bytes().to_vec()).unwrap_or_default();
                        if let Ok(h) = global(&data) {
                            let _ = SetClipboardData(format, Some(h));
                        }
                    }
                }
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result.map(|_| GetClipboardSequenceNumber())
    };
    wide.fill(0);
    result
}
