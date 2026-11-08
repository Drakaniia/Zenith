//! Startup: installer/maintenance flags handled before anything else, the web addresses a
//! command line asks for, and a second launch handing its request to the running instance.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use super::window::create_window;
use crate::{host, incognito, profiles, urls, win};

/// Web addresses on a command line (skipping the program itself and `--profile <id>`).
pub(crate) fn url_args(argv: &[String]) -> Vec<String> {
    profiles::without_flag(argv).iter().skip(1).filter_map(|a| urls::external_arg_to_url(a)).collect()
}

/// A second launch (e.g. a link clicked in another app, or the jump list's incognito entry)
/// hands its command line to the running instance of the same profile.
pub(crate) fn on_second_instance(app: &AppHandle, argv: Vec<String>) {
    let urls = url_args(&argv);
    if incognito::requested(&argv) {
        incognito::open_window(urls);
        return;
    }
    match app.get_webview_window(&host::primary_window()) {
        Some(w) => {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            host::emit_to_window(w.label(), "open-urls", urls);
        }
        // Only incognito windows are open: bring the normal window back.
        None => {
            set_startup_urls(host::MAIN, urls);
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(e) = create_window(&app, host::MAIN) {
                    eprintln!("[zenith] could not reopen the window: {e}");
                }
            });
        }
    }
}

/// Installer/maintenance entry points that must run before the single-instance check.
pub(crate) fn maintenance_command() -> Option<i32> {
    let arg = std::env::args().nth(1)?;
    let code = match arg.as_str() {
        incognito::PURGE_FLAG => incognito::purge_command(&std::env::args().nth(2).unwrap_or_default()),
        "--register-browser" => match std::env::current_exe().map_err(|e| e.to_string()).and_then(|exe| win::register_browser(&exe)) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("registration failed: {e}");
                1
            }
        },
        "--unregister-browser" => {
            win::unregister_browser();
            0
        }
        _ => return None,
    };
    Some(code)
}

/// Addresses each window opens once its UI is ready, by window label.
static STARTUP_URLS: Mutex<Option<HashMap<String, Vec<String>>>> = Mutex::new(None);

pub fn set_startup_urls(label: &str, urls: Vec<String>) {
    if let Ok(mut s) = STARTUP_URLS.lock() {
        s.get_or_insert_with(HashMap::new).insert(label.to_string(), urls);
    }
}

pub(crate) fn take_startup_urls(label: &str) -> Vec<String> {
    STARTUP_URLS
        .lock()
        .ok()
        .and_then(|mut s| s.as_mut().and_then(|m| m.remove(label)))
        .unwrap_or_default()
}
