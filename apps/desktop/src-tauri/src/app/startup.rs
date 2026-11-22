//! Startup: maintenance flags handled before single-instance check, command-line URLs, and
//! handing a second launch's arguments to the running profile instance.

use std::collections::HashMap;
use std::sync::Mutex;

use super::runtime::AppHandle;
use super::window::create_window;
use crate::platform::{urls, win};
use crate::{host, incognito, profiles};

/// Web addresses on a command line (skipping the program itself and `--profile <id>`).
pub(crate) fn url_args(argv: &[String]) -> Vec<String> {
    let torrents = crate::torrents::torrent_args(argv);
    profiles::without_flag(argv)
        .iter()
        .skip(1)
        .filter(|a| !torrents.iter().any(|t| t == a.trim().trim_matches('"')))
        .filter_map(|a| urls::external_arg_to_url(a))
        .collect()
}

/// A second launch hands its command line to the running instance of the same profile.
pub(crate) fn on_second_instance(app: &AppHandle, argv: Vec<String>) {
    let urls = url_args(&argv);
    crate::torrents::open_args(&argv);
    if incognito::requested(&argv) {
        incognito::open_window(urls);
        return;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || match app2.get_window(&host::primary_window()) {
        Some(w) => {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            host::emit_to_window(w.label(), "open-urls", urls);
        }
        None => {
            set_startup_urls(host::MAIN, urls);
            if let Err(e) = create_window(&app2, host::MAIN) {
                eprintln!("[zenith] could not reopen the window: {e}");
            }
        }
    });
}

/// Installer/maintenance entry points that run before single-instance locking or CEF startup.
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
