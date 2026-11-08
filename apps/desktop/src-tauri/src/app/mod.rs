//! The application: the Tauri builder (plugins, commands, setup, window events), quitting and
//! closing windows.

pub mod command;
pub mod commands;
mod handlers;
pub mod startup;
pub mod window;

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Manager, WindowEvent};

use crate::{host, incognito, profiles, session, updates, vault};
use window::{create_window, remember_window_state, save_window_state, NEXT_INCOGNITO};

static QUITTING: AtomicBool = AtomicBool::new(false);
/// Windows whose UI was asked to save before closing (see `CloseRequested`).
static CLOSING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Closes one window once its UI has saved what it keeps. The last window quits Zenith.
pub fn close_window(app: &AppHandle, label: &str) {
    if QUITTING.load(Ordering::SeqCst) {
        return;
    }
    let others = app.webview_windows().keys().filter(|l| l.as_str() != label).count();
    if others == 0 {
        request_quit(app);
        return;
    }
    if label == host::MAIN {
        save_window_state();
    }
    // Ctrl+Shift+N can bring it back.
    session::window_closed(label);
    let app2 = app.clone();
    let label = label.to_string();
    let _ = app.run_on_main_thread(move || {
        host::detach_window(&label);
        if let Some(w) = app2.get_webview_window(&label) {
            let _ = w.destroy();
        }
        if let Ok(mut c) = CLOSING.lock() {
            if let Some(set) = c.as_mut() {
                set.remove(&label);
            }
        }
    });
}

/// Saves window geometry, frees every page webview and exits. A downloaded update is installed
/// silently on the way out (and relaunches Zenith when "Restart to update" was chosen).
pub fn request_quit(app: &AppHandle) {
    QUITTING.store(true, Ordering::SeqCst);
    save_window_state();
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        host::shutdown();
        // Only returns when there was nothing to install (the installer ends the process).
        if updates::install_on_exit() {
            app2.restart();
        }
        app2.exit(0);
    });
}

/// Runs Zenith (called from `main`).
pub fn run() {
    let _ = host::STARTED.set(std::time::Instant::now());
    if let Some(code) = startup::maintenance_command() {
        std::process::exit(code);
    }
    let args: Vec<String> = std::env::args().collect();
    let start_incognito = incognito::requested(&args);
    let startup = startup::url_args(&args);

    // Pick the profile before anything touches the disk. Each non-default profile runs under its
    // own app identifier, which keys the single-instance lock below (see `profiles`).
    let mut context = tauri::generate_context!();
    let base_identifier = context.config().identifier.clone();
    let profile = profiles::init(&base_identifier, &args, host::profile_override());
    if !profile.is_default() {
        context.config_mut().identifier = profiles::identifier(&base_identifier, &profile.id);
    }

    let mut builder = tauri::Builder::default();
    // A second launch (e.g. a link clicked in another app, or the jump list's incognito entry)
    // hands its request to the running instance of the same profile. Instances on a separate
    // (ZENITH_PROFILE_DIR) folder stay independent.
    if host::profile_override().is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| startup::on_second_instance(app, argv)));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(handlers::handler())
        .setup(move |app| {
            host::init_services(app.handle())?;
            // Locks the password vault after the configured inactivity.
            vault::start_timer();
            // `zenith --incognito` with no Zenith running opens only an incognito window.
            if start_incognito {
                let label = format!("{}{}", incognito::LABEL_PREFIX, NEXT_INCOGNITO.fetch_add(1, Ordering::Relaxed));
                startup::set_startup_urls(&label, startup);
                create_window(app.handle(), &label)?;
            } else {
                startup::set_startup_urls(host::MAIN, startup);
                create_window(app.handle(), host::MAIN)?;
            }
            // Test instances (separate profile) never update themselves.
            let updatable = host::profile_override().is_none() || std::env::var_os("ZENITH_UPDATE_ENDPOINT").is_some();
            if updatable {
                updates::start(app.handle().clone());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                if QUITTING.load(Ordering::SeqCst) {
                    return;
                }
                // Let the UI save its session first (it answers with `window_close`); close
                // anyway if it does not answer.
                api.prevent_close();
                let label = window.label().to_string();
                let first = CLOSING
                    .lock()
                    .map(|mut c| c.get_or_insert_with(HashSet::new).insert(label.clone()))
                    .unwrap_or(true);
                if first {
                    host::emit_to_window(&label, "app-close-requested", ());
                    let app = window.app_handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(2500));
                        let pending = CLOSING.lock().map(|c| c.as_ref().is_some_and(|s| s.contains(&label))).unwrap_or(false);
                        if pending {
                            close_window(&app, &label);
                        }
                    });
                }
            }
            WindowEvent::Resized(_) | WindowEvent::Moved(_) if window.label() == host::MAIN => remember_window_state(window),
            _ => {}
        })
        .run(context)
        .expect("error while running Zenith");
}
