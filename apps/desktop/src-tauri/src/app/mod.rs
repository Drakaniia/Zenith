//! Application lifecycle: CEF subprocess entry, profile initialization, single-instance lock,
//! CEF initialization with Windows sandbox, window closing, and clean exit.

pub mod bridge;
pub mod command;
pub mod commands;
pub mod handlers;
pub mod runtime;
pub mod security;
pub mod single_instance;
pub mod startup;
pub mod window;

use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cef::*;

use runtime::{AppHandle, APP_IDENTIFIER};
use window::{create_window, save_window_state, NEXT_INCOGNITO};
use crate::{host, incognito, profiles, session, updates, vault};

static QUITTING: AtomicBool = AtomicBool::new(false);
/// Windows whose UI was asked to save before closing.
static CLOSING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

pub fn is_quitting() -> bool {
    QUITTING.load(Ordering::SeqCst)
}

/// Called from `WM_CLOSE` or `plugin:window|close`: asks the window's UI to save its session
/// first (`app-close-requested`), then closes even if the UI does not answer.
pub fn on_window_close_requested(label: &str) {
    if QUITTING.load(Ordering::SeqCst) {
        return;
    }
    let label = label.to_string();
    let first = CLOSING
        .lock()
        .map(|mut c| c.get_or_insert_with(HashSet::new).insert(label.clone()))
        .unwrap_or(true);
    if first {
        host::emit_to_window(&label, "app-close-requested", ());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let pending = CLOSING
                .lock()
                .map(|c| c.as_ref().is_some_and(|s| s.contains(&label)))
                .unwrap_or(false);
            if pending {
                close_window(&AppHandle, &label);
            }
        });
    }
}

/// Closes one window once its UI has saved what it keeps. The last window quits Zenith.
pub fn close_window(app: &AppHandle, label: &str) {
    if QUITTING.load(Ordering::SeqCst) {
        return;
    }
    let others = app.windows().keys().filter(|l| l.as_str() != label).count();
    if others == 0 {
        request_quit(app);
        return;
    }
    if label == host::MAIN {
        save_window_state();
    }
    session::window_closed(label);
    let app2 = app.clone();
    let label = label.to_string();
    let _ = app.run_on_main_thread(move || {
        host::detach_window(&label);
        if let Some(w) = app2.get_window(&label) {
            let _ = w.destroy();
        }
        if let Ok(mut c) = CLOSING.lock() {
            if let Some(set) = c.as_mut() {
                set.remove(&label);
            }
        }
    });
}

/// Saves window geometry, closes all CEF browsers and exits cleanly.
pub fn request_quit(app: &AppHandle) {
    if QUITTING.swap(true, Ordering::SeqCst) {
        return;
    }
    save_window_state();
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        host::shutdown();
        crate::torrents::shutdown();
        if updates::install_on_exit() {
            app2.restart();
            return;
        }
        app2.exit(0);
    });
}

/// Main entry point invoked by CEF's Windows bootstrapper (`RunWinMain` / `RunConsoleMain`).
pub fn run_cef(h_instance: *mut c_void, sandbox_info: *mut c_void) -> i32 {
    let _ = host::STARTED.set(std::time::Instant::now());
    let _ = cef::api_hash(sys::CEF_API_VERSION_LAST, 0);

    let main_args = MainArgs {
        instance: cef::sys::HINSTANCE(h_instance.cast()),
    };

    // CEF subprocesses (renderer, GPU, utility, network) carry `--type=...` and must enter
    // `execute_process` before profile locking or UI initialization.
    let mut cef_app = host::cef_app::create_app();
    let sub_code = cef::execute_process(Some(&main_args), Some(&mut cef_app), sandbox_info.cast());
    if sub_code >= 0 {
        return sub_code;
    }

    // Browser process startup.
    if let Some(code) = startup::maintenance_command() {
        return code;
    }

    // Fail closed if a bundled runtime-manifest.json detects corrupt or missing CEF binaries.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Err(e) = security::verify_runtime_manifest(dir) {
                eprintln!("[zenith] runtime verification failed: {e}");
                show_startup_error(&format!(
                    "Zenith's installation appears incomplete or damaged ({e}).\n\nReinstall Zenith or run the previous installer in %LOCALAPPDATA%\\dev.zenith.browser\\update-rollback\\."
                ));
                return 2;
            }
        }
    }

    let args: Vec<String> = std::env::args().collect();
    let start_incognito = incognito::requested(&args);
    let startup_urls = startup::url_args(&args);

    let profile = profiles::init(APP_IDENTIFIER, &args, host::profile_override());
    let instance_id = if profile.is_default() {
        APP_IDENTIFIER.to_string()
    } else {
        profiles::identifier(APP_IDENTIFIER, &profile.id)
    };

    // Per-profile single-instance redirect (skipped for isolated `ZENITH_PROFILE_DIR` instances).
    let _instance_guard = if host::profile_override().is_none() {
        match single_instance::acquire_or_forward(
            &instance_id,
            &args,
            Arc::new(|argv| startup::on_second_instance(&AppHandle, argv)),
        ) {
            Ok(Some(guard)) => Some(guard),
            Ok(None) => return 0,
            Err(e) => {
                eprintln!("[zenith] single-instance lock failed: {e}");
                None
            }
        }
    } else {
        None
    };

    if let Err(e) = host::init_services(&AppHandle) {
        eprintln!("[zenith] service initialization failed: {e}");
        return 1;
    }
    updates::finalize_pending_install(&host::paths().local, env!("CARGO_PKG_VERSION"));
    vault::start_timer();
    crate::torrents::open_args(&args);

    let initial_label = if start_incognito {
        let label = format!("{}{}", incognito::LABEL_PREFIX, NEXT_INCOGNITO.fetch_add(1, Ordering::Relaxed));
        startup::set_startup_urls(&label, startup_urls);
        label
    } else {
        startup::set_startup_urls(host::MAIN, startup_urls);
        host::MAIN.to_string()
    };

    let updatable = host::profile_override().is_none() || std::env::var_os("ZENITH_UPDATE_ENDPOINT").is_some();
    if updatable {
        updates::start(AppHandle);
    }

    // Configure CEF with versioned CEF profile cache path and full Windows sandbox.
    let cef_cache = host::paths().cef_data();
    let _ = std::fs::create_dir_all(&cef_cache);
    let mut settings = Settings::default();
    // Use real Windows sandbox whenever launched via bootstrap.exe (`sandbox_info != null`).
    settings.no_sandbox = if sandbox_info.is_null() { 1 } else { 0 };
    settings.root_cache_path = CefString::from(cef_cache.to_string_lossy().as_ref());
    settings.cache_path = CefString::from(cef_cache.to_string_lossy().as_ref());
    settings.persist_session_cookies = 1;
    settings.background_color = 0x0000_0000;
    settings.log_severity = LogSeverity::WARNING;
    #[cfg(any(debug_assertions, feature = "remote-debug"))]
    if let Ok(port_str) = std::env::var("ZENITH_REMOTE_DEBUG_PORT") {
        if let Ok(port) = port_str.parse::<u16>() {
            settings.remote_debugging_port = i32::from(port);
        }
    }

    host::init_browser_cdp_pipes();
    host::cef_app::set_initial_window(initial_label);

    let mut init_app = host::cef_app::create_app();
    if cef::initialize(Some(&main_args), Some(&settings), Some(&mut init_app), sandbox_info.cast()) == 0 {
        host::close_browser_cdp_pipes();
        eprintln!("[zenith] cef::initialize failed");
        return 1;
    }

    cef::run_message_loop();
    host::close_browser_cdp_pipes();
    cef::shutdown();
    0
}

pub fn create_initial_window(label: &str) {
    if let Err(e) = create_window(&AppHandle, label) {
        eprintln!("[zenith] failed to create window {label}: {e}");
    }
}

fn show_startup_error(msg: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let text: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "Zenith\0".encode_utf16().collect();
    unsafe {
        MessageBoxW(None, PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR);
    }
}
