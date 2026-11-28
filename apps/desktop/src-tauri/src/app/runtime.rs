//! Pure Win32 + Tokio application runtime replacing Tauri's `AppHandle` and `WebviewWindow`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, RwLock};

use cef::*;
use windows::core::{BOOL, HSTRING};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW,
    DWM_SYSTEMBACKDROP_TYPE,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const APP_IDENTIFIER: &str = "dev.zenith.browser";

static TOKIO: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("zenith-async")
        .build()
        .expect("tokio runtime")
});

pub fn spawn<F>(fut: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    TOKIO.spawn(fut)
}

pub fn spawn_blocking<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    TOKIO.spawn_blocking(f)
}

wrap_task! {
    struct ClosureTask {
        job: Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>,
    }

    impl Task {
        fn execute(&self) {
            if let Ok(mut guard) = self.job.lock() {
                if let Some(f) = guard.take() {
                    f();
                }
            }
        }
    }
}

/// Posts a closure asynchronously to the CEF UI (Win32 main) thread.
pub fn post_ui(f: impl FnOnce() + Send + 'static) -> Result<(), String> {
    let mut task = ClosureTask::new(Arc::new(Mutex::new(Some(Box::new(f)))));
    if cef::post_task(ThreadId::UI, Some(&mut task)) != 0 {
        Ok(())
    } else {
        Err("UI thread is not running".into())
    }
}

/// Posts a delayed closure to the CEF UI thread.
pub fn post_ui_delayed(delay_ms: i64, f: impl FnOnce() + Send + 'static) {
    let mut task = ClosureTask::new(Arc::new(Mutex::new(Some(Box::new(f)))));
    let _ = cef::post_delayed_task(ThreadId::UI, Some(&mut task), delay_ms);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug)]
struct SavedPlacement {
    style: i32,
    ex_style: i32,
    rect: RECT,
    maximized: bool,
}

static WINDOWS: LazyLock<RwLock<HashMap<String, WindowHandle>>> = LazyLock::new(Default::default);
static FULLSCREEN_SAVED: LazyLock<Mutex<HashMap<isize, SavedPlacement>>> = LazyLock::new(Default::default);
static DEFAULT_ICON: LazyLock<Option<isize>> = LazyLock::new(|| {
    let bytes = include_bytes!("../../icons/128x128.png");
    crate::platform::win::window_icon::hicon_from_png(bytes).map(|h| h.0 as isize)
});

pub fn register_window(win: WindowHandle) {
    if let Ok(mut map) = WINDOWS.write() {
        map.insert(win.label.clone(), win);
    }
}

pub fn unregister_window(label: &str) {
    if let Ok(mut map) = WINDOWS.write() {
        if let Some(w) = map.remove(label) {
            if let Ok(mut fs) = FULLSCREEN_SAVED.lock() {
                fs.remove(&w.hwnd_raw);
            }
        }
    }
}

pub fn window_by_hwnd(hwnd: HWND) -> Option<WindowHandle> {
    let raw = hwnd.0 as isize;
    WINDOWS.read().ok()?.values().find(|w| w.hwnd_raw == raw).cloned()
}

/// Lightweight process-wide handle matching the operations domains previously called on `tauri::AppHandle`.
#[derive(Clone, Default)]
pub struct AppHandle;

impl AppHandle {
    pub fn run_on_main_thread(&self, f: impl FnOnce() + Send + 'static) -> Result<(), String> {
        post_ui(f)
    }

    pub fn get_window(&self, label: &str) -> Option<WindowHandle> {
        WINDOWS.read().ok()?.get(label).cloned()
    }

    pub fn windows(&self) -> HashMap<String, WindowHandle> {
        WINDOWS.read().map(|m| m.clone()).unwrap_or_default()
    }

    pub fn default_window_icon(&self) -> Option<HICON> {
        DEFAULT_ICON.map(|raw| HICON(raw as *mut _))
    }

    pub fn download_dir(&self) -> Option<PathBuf> {
        unsafe {
            let pwstr = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
            let path = pwstr.to_string().ok().map(PathBuf::from);
            CoTaskMemFree(Some(pwstr.0 as *const _));
            path
        }
    }

    pub fn exit(&self, _code: i32) {
        let _ = post_ui(|| {
            crate::host::close_all_for_exit();
        });
    }

    pub fn restart(&self) {
        if let Ok(exe) = std::env::current_exe() {
            let args: Vec<String> = std::env::args().skip(1).collect();
            let _ = std::process::Command::new(exe).args(args).spawn();
        }
        self.exit(0);
    }
}

/// Handle to one top-level Win32 browser window (`main`, `window-N`, `incognito-N`).
#[derive(Clone, Debug)]
pub struct WindowHandle {
    label: String,
    hwnd_raw: isize,
}

impl WindowHandle {
    pub fn new(label: impl Into<String>, hwnd: HWND) -> Self {
        Self { label: label.into(), hwnd_raw: hwnd.0 as isize }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn hwnd(&self) -> HWND {
        HWND(self.hwnd_raw as *mut _)
    }

    pub fn show(&self) -> Result<(), String> {
        unsafe {
            let _ = ShowWindow(self.hwnd(), SW_SHOW);
        }
        Ok(())
    }

    pub fn hide(&self) -> Result<(), String> {
        unsafe {
            let _ = ShowWindow(self.hwnd(), SW_HIDE);
        }
        Ok(())
    }

    pub fn minimize(&self) -> Result<(), String> {
        unsafe {
            let _ = ShowWindow(self.hwnd(), SW_MINIMIZE);
        }
        Ok(())
    }

    pub fn unminimize(&self) -> Result<(), String> {
        unsafe {
            if IsIconic(self.hwnd()).as_bool() {
                let _ = ShowWindow(self.hwnd(), SW_RESTORE);
            }
        }
        Ok(())
    }

    pub fn maximize(&self) -> Result<(), String> {
        unsafe {
            let _ = ShowWindow(self.hwnd(), SW_MAXIMIZE);
        }
        Ok(())
    }

    pub fn unmaximize(&self) -> Result<(), String> {
        unsafe {
            let _ = ShowWindow(self.hwnd(), SW_RESTORE);
        }
        Ok(())
    }

    pub fn toggle_maximize(&self) -> Result<(), String> {
        if self.is_maximized() {
            self.unmaximize()
        } else {
            self.maximize()
        }
    }

    pub fn is_maximized(&self) -> bool {
        unsafe { IsZoomed(self.hwnd()).as_bool() }
    }

    pub fn is_minimized(&self) -> bool {
        unsafe { IsIconic(self.hwnd()).as_bool() }
    }

    pub fn is_focused(&self) -> bool {
        unsafe { GetForegroundWindow() == self.hwnd() }
    }

    pub fn is_fullscreen(&self) -> bool {
        FULLSCREEN_SAVED.lock().map(|m| m.contains_key(&self.hwnd_raw)).unwrap_or(false)
    }

    pub fn set_fullscreen(&self, on: bool) -> Result<(), String> {
        let hwnd = self.hwnd();
        let currently = self.is_fullscreen();
        if on == currently {
            return Ok(());
        }
        unsafe {
            if on {
                let style = GetWindowLongW(hwnd, GWL_STYLE);
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                let maximized = IsZoomed(hwnd).as_bool();
                if let Ok(mut map) = FULLSCREEN_SAVED.lock() {
                    map.insert(self.hwnd_raw, SavedPlacement { style, ex_style, rect, maximized });
                }
                let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                let mut mi = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if GetMonitorInfoW(mon, &mut mi).as_bool() {
                    let r = mi.rcMonitor;
                    let _ = SetWindowLongW(hwnd, GWL_STYLE, style & !(WS_THICKFRAME.0 as i32));
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOP),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
                    );
                }
            } else if let Some(saved) = FULLSCREEN_SAVED.lock().ok().and_then(|mut m| m.remove(&self.hwnd_raw)) {
                let _ = SetWindowLongW(hwnd, GWL_STYLE, saved.style);
                let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, saved.ex_style);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    saved.rect.left,
                    saved.rect.top,
                    saved.rect.right - saved.rect.left,
                    saved.rect.bottom - saved.rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
                if saved.maximized {
                    let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                }
            }
        }
        Ok(())
    }

    pub fn set_focus(&self) -> Result<(), String> {
        unsafe {
            let _ = SetForegroundWindow(self.hwnd());
        }
        Ok(())
    }

    pub fn set_title(&self, title: &str) -> Result<(), String> {
        unsafe { SetWindowTextW(self.hwnd(), &HSTRING::from(title)).map_err(|e| e.to_string()) }
    }

    pub fn set_theme(&self, theme: Option<Theme>) -> Result<(), String> {
        let dark: BOOL = match theme {
            Some(Theme::Dark) => true.into(),
            Some(Theme::Light) => false.into(),
            None => system_uses_dark_theme().into(),
        };
        unsafe {
            let _ = DwmSetWindowAttribute(
                self.hwnd(),
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const BOOL as *const _,
                std::mem::size_of::<BOOL>() as u32,
            );
        }
        Ok(())
    }

    /// Configures Windows 11 DWM backdrop material (`mica`, `acrylic`, `solid`). Returns `false`
    /// when DWM rejects a translucent backdrop so the UI falls back to opaque solid CSS colours.
    pub fn set_material(&self, material: &str) -> bool {
        let hwnd = self.hwnd();
        let (backdrop, margins) = match material {
            "mica" => (
                DWMSBT_MAINWINDOW,
                MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 },
            ),
            "acrylic" => (
                DWMSBT_TRANSIENTWINDOW,
                MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 },
            ),
            _ => (
                DWMSBT_NONE,
                MARGINS { cxLeftWidth: 1, cxRightWidth: 1, cyTopHeight: 1, cyBottomHeight: 1 },
            ),
        };
        unsafe {
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
            let ok = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const DWM_SYSTEMBACKDROP_TYPE as *const _,
                std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
            )
            .is_ok();
            if !ok && backdrop != DWMSBT_NONE {
                let solid = MARGINS { cxLeftWidth: 1, cxRightWidth: 1, cyTopHeight: 1, cyBottomHeight: 1 };
                let _ = DwmExtendFrameIntoClientArea(hwnd, &solid);
                return false;
            }
            true
        }
    }

    pub fn set_icon(&self, icon: HICON) -> Result<(), String> {
        crate::platform::win::window_icon::apply_to_window(self.hwnd(), icon);
        Ok(())
    }

    pub fn outer_position(&self) -> Result<(i32, i32), String> {
        let mut r = RECT::default();
        unsafe { GetWindowRect(self.hwnd(), &mut r).map_err(|e| e.to_string())? };
        Ok((r.left, r.top))
    }

    pub fn inner_size(&self) -> Result<(u32, u32), String> {
        let mut r = RECT::default();
        unsafe { GetClientRect(self.hwnd(), &mut r).map_err(|e| e.to_string())? };
        Ok(((r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32))
    }

    pub fn set_position(&self, x: i32, y: i32) -> Result<(), String> {
        unsafe {
            SetWindowPos(self.hwnd(), None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)
                .map_err(|e| e.to_string())
        }
    }

    pub fn set_size(&self, w: u32, h: u32) -> Result<(), String> {
        unsafe {
            SetWindowPos(self.hwnd(), None, 0, 0, w as i32, h as i32, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE)
                .map_err(|e| e.to_string())
        }
    }

    pub fn scale_factor(&self) -> f64 {
        let dpi = unsafe { GetDpiForWindow(self.hwnd()) };
        if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 }
    }

    pub fn center(&self) -> Result<(), String> {
        unsafe {
            let mon = MonitorFromWindow(self.hwnd(), MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(mon, &mut mi).as_bool() {
                let mut r = RECT::default();
                let _ = GetWindowRect(self.hwnd(), &mut r);
                let w = r.right - r.left;
                let h = r.bottom - r.top;
                let wa = mi.rcWork;
                let x = wa.left + ((wa.right - wa.left) - w) / 2;
                let y = wa.top + ((wa.bottom - wa.top) - h) / 2;
                let _ = SetWindowPos(self.hwnd(), None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
        Ok(())
    }

    pub fn start_dragging(&self) -> Result<(), String> {
        unsafe {
            let _ = ReleaseCapture();
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let lparam = LPARAM(((pt.y as u16 as usize) << 16 | (pt.x as u16 as usize)) as isize);
            let _ = PostMessageW(Some(self.hwnd()), WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), lparam);
        }
        Ok(())
    }

    pub fn available_monitors(&self) -> Vec<MonitorRect> {
        unsafe extern "system" fn enum_cb(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
            let out = &mut *(data.0 as *mut Vec<MonitorRect>);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(hmon, &mut mi).as_bool() {
                let r = mi.rcMonitor;
                out.push(MonitorRect {
                    x: r.left,
                    y: r.top,
                    width: (r.right - r.left).max(0) as u32,
                    height: (r.bottom - r.top).max(0) as u32,
                });
            }
            true.into()
        }
        let mut out = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(enum_cb), LPARAM(&mut out as *mut Vec<MonitorRect> as isize));
        }
        out
    }

    pub fn destroy(&self) -> Result<(), String> {
        unregister_window(&self.label);
        unsafe {
            let _ = DestroyWindow(self.hwnd());
        }
        Ok(())
    }
}

fn system_uses_dark_theme() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut val = 1u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            &HSTRING::from("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut val as *mut u32 as *mut _),
            Some(&mut len),
        )
    };
    status.is_ok() && val == 0
}
