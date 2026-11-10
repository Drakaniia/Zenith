//! Window lifecycle: process-wide initialisation, attaching a window's chrome webview, detaching
//! a closing window and shutdown.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;

use tauri::{AppHandle, WebviewWindow};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::Interface;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::SetWindowSubclass;

use super::devtools::watch_page_targets;
use super::state::{APP, DB, PATHS};
use super::tab::Tab;
use super::window_proc::subclass_proc;
use super::{app, attach_accelerators, cdp_call, db, post, target_id_of, with_host, with_win};
use super::{Focus, Host, LayoutSpec, Win, HOST, MAIN_HWND, SUBCLASS_ID};
use crate::storage::{Db, Paths};

// ---------------------------------------------------------------------------------------------
// Initialisation

/// Started-at instant, for the startup-time measurement (`ZENITH_PERF=1`).
pub static STARTED: OnceLock<std::time::Instant> = OnceLock::new();

/// `ZENITH_PROFILE_DIR` runs Zenith on a separate, self-contained profile (tests, measurements,
/// portable use). Such an instance also skips the single-instance redirect.
pub fn profile_override() -> Option<PathBuf> {
    std::env::var_os("ZENITH_PROFILE_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

pub fn init_services(app: &AppHandle) -> tauri::Result<()> {
    // The profile this process runs (chosen in `app::run`, see `profiles`) owns every data folder.
    let paths: Paths = crate::profiles::active().paths();
    let _ = std::fs::create_dir_all(&paths.roaming);
    let _ = std::fs::create_dir_all(&paths.local);
    let _ = APP.set(app.clone());
    let _ = DB.set(Db::open(paths.db()));
    let _ = PATHS.set(paths);
    // Throwaway profiles of the separate-process incognito windows of earlier versions.
    crate::incognito::sweep_stale();
    // Media jobs do not survive a restart; drop their partial data.
    crate::media::job::cleanup_temp();
    // Torrents continue where they stopped.
    crate::torrents::restore();
    db().exec(|c| {
        let _ = crate::history::history_prune(c, 0, 100_000);
    });
    Ok(())
}

/// Called once a window and its chrome webview exist (on the main thread).
pub fn attach_chrome(window: WebviewWindow, controller: ICoreWebView2Controller, env: ICoreWebView2Environment) {
    let label = window.label().to_string();
    let hwnd = window.hwnd().map(|h| HWND(h.0)).unwrap_or_default();
    let mut chrome_hwnd = HWND::default();
    let chrome_core = unsafe {
        let _ = controller.ParentWindow(&mut chrome_hwnd);
        controller.CoreWebView2().expect("chrome CoreWebView2")
    };
    unsafe {
        if let Ok(settings) = chrome_core.Settings() {
            // The chrome is an app surface: no browser reload/print/find accelerators,
            // no default context menu in release builds, no zoom.
            if let Ok(s3) = settings.cast::<ICoreWebView2Settings3>() {
                let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
            }
            let _ = settings.SetAreDefaultContextMenusEnabled(cfg!(debug_assertions));
            let _ = settings.SetIsZoomControlEnabled(false);
            let _ = settings.SetIsStatusBarEnabled(false);
            if let Ok(s5) = settings.cast::<ICoreWebView2Settings5>() {
                let _ = s5.SetIsPinchZoomEnabled(false);
            }
            if let Ok(s6) = settings.cast::<ICoreWebView2Settings6>() {
                let _ = s6.SetIsSwipeNavigationEnabled(false);
            }
        }
        attach_accelerators(&controller, None, &label);
        let mut token = 0i64;
        let focus_label = label.clone();
        let _ = controller.add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                let label = focus_label.clone();
                post(move || {
                    with_win(&label, |w| w.focus = Focus::Chrome);
                });
                Ok(())
            })),
            &mut token,
        );
        let crash_label = label.clone();
        // If the whole WebView2 browser process dies every view is gone; relaunch and let
        // session restore bring the tabs back.
        let _ = chrome_core.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    args.ProcessFailedKind(&mut kind)?;
                    if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                        eprintln!("[zenith] WebView2 browser process exited; restarting");
                        post(|| app().restart());
                    } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED {
                        let label = crash_label.clone();
                        post(move || {
                            with_win(&label, |w| {
                                let _ = w.chrome_core.Reload();
                            });
                        });
                    }
                }
                Ok(())
            })),
            &mut token,
        );
    }
    if MAIN_HWND.load(Ordering::SeqCst) == 0 {
        MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    }
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
    }
    // One chrome watches for stray page targets: discovery reports every target of the browser
    // process, so a second watcher would adopt each stray page twice.
    let first = HOST.with(|cell| cell.borrow().is_none());
    let target_watch = if first { watch_page_targets(&chrome_core) } else { None };
    let win = Win {
        window,
        hwnd,
        chrome: controller,
        chrome_core: chrome_core.clone(),
        chrome_hwnd,
        layout: LayoutSpec::default(),
        focus: Focus::Chrome,
        fullscreen_tab: None,
        chrome_raised: None,
        chrome_target_id: String::new(),
        _target_watch: target_watch,
    };
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        match guard.as_mut() {
            Some(h) => {
                h.wins.insert(label.clone(), win);
            }
            None => {
                *guard = Some(Host {
                    wins: HashMap::from([(label.clone(), win)]),
                    env,
                    tabs: HashMap::new(),
                    zombies: HashMap::new(),
                    page_bg: (32, 32, 36),
                    content_scheme: COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO,
                    permissions: HashMap::new(),
                    downloads: HashMap::new(),
                    target_owner: label.clone(),
                })
            }
        }
    });
    let id_label = label.clone();
    cdp_call(&chrome_core, "Target.getTargetInfo", "{}", move |r| {
        if let Some(id) = r.ok().as_deref().and_then(target_id_of) {
            with_win(&id_label, |w| w.chrome_target_id = id);
        }
    });
    if first {
        cdp_call(&chrome_core, "Target.setDiscoverTargets", r#"{"discover":true}"#, |r| {
            if let Err(e) = r {
                eprintln!("[zenith] target discovery unavailable: {e}");
            }
        });
    }
}

/// Forgets a closing window and frees its pages (tabs with running downloads are kept until
/// they finish). The caller destroys the window itself afterwards.
pub fn detach_window(label: &str) {
    let views: Vec<Tab> = with_host(|h| {
        let Some(win) = h.wins.remove(label) else { return Vec::new() };
        let ids: Vec<String> = h.tabs.iter().filter(|(_, t)| t.window() == label).map(|(id, _)| id.clone()).collect();
        let mut freed = Vec::new();
        for id in ids {
            let Some(tab) = h.tabs.remove(&id) else { continue };
            h.permissions.retain(|_, p| p.tab != id);
            if h.downloads.values().any(|d| d.tab == id && d.record.state == "progress") {
                let _ = tab.view.set_visible(false);
                h.zombies.insert(id, tab);
            } else {
                freed.push(tab);
            }
        }
        if MAIN_HWND.load(Ordering::SeqCst) == win.hwnd.0 as isize {
            let next = h.wins.values().next().map(|w| w.hwnd.0 as isize).unwrap_or(0);
            MAIN_HWND.store(next, Ordering::SeqCst);
        }
        if h.target_owner == label {
            // The watcher went with the window; its replacement starts watching.
            if let Some((l, w)) = h.wins.iter_mut().next() {
                h.target_owner = l.clone();
                w._target_watch = watch_page_targets(&w.chrome_core);
                cdp_call(&w.chrome_core, "Target.setDiscoverTargets", r#"{"discover":true}"#, |_| {});
            }
        }
        freed
    })
    .unwrap_or_default();
    drop(views);
}

/// Drops every page webview. Called before exit so WebView2 shuts down cleanly.
pub fn shutdown() {
    let views: Vec<Tab> = with_host(|h| {
        let mut all: Vec<Tab> = h.tabs.drain().map(|(_, t)| t).collect();
        all.extend(h.zombies.drain().map(|(_, t)| t));
        h.permissions.clear();
        all
    })
    .unwrap_or_default();
    drop(views);
}
