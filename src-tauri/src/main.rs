#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Zenith: a lightweight Zen-style browser for Windows (Tauri 2 + WebView2).

mod blocker;
mod bookmarks;
mod commands;
mod host;
mod hotkey;
mod importer;
mod incognito;
mod keys;
mod media;
mod net;
mod password_commands;
mod profile_commands;
mod profiles;
mod sessions;
mod store;
mod updates;
mod urls;
mod vault;
mod win;

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::utils::config::WindowEffectsConfig;
use tauri::webview::ScrollBarStyle;
use tauri::window::Effect;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

static QUITTING: AtomicBool = AtomicBool::new(false);
/// Windows whose UI was asked to save before closing (see `CloseRequested`).
static CLOSING: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static NEXT_INCOGNITO: AtomicU32 = AtomicU32::new(1);
static NEXT_WINDOW: AtomicU32 = AtomicU32::new(1);

/// The saved state was maximized. Applied when the window is first shown (`ui_ready`):
/// maximizing a still-hidden window leaves it flagged maximized at its restored size, so it
/// never fills the screen and the webviews never get the maximized client size.
pub static RESTORE_MAXIMIZED: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    maximized: bool,
}

static WINDOW_STATE: Mutex<Option<WindowState>> = Mutex::new(None);

/// Geometry of the normal window. Incognito windows open next to it and never save their own.
fn window_state_file() -> std::path::PathBuf {
    host::persistent_paths().roaming.join("window.json")
}

fn remember_window_state(window: &tauri::Window) {
    let maximized = window.is_maximized().unwrap_or(false);
    if window.is_minimized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }
    let Ok(mut guard) = WINDOW_STATE.lock() else { return };
    if maximized {
        if let Some(s) = guard.as_mut() {
            s.maximized = true;
        }
        return;
    }
    if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
        *guard = Some(WindowState {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            maximized: false,
        });
    }
}

fn save_window_state() {
    if let Some(state) = WINDOW_STATE.lock().ok().and_then(|s| *s) {
        if let Ok(json) = serde_json::to_vec(&state) {
            let _ = store::write_atomic(&window_state_file(), &json);
        }
    }
}

fn restore_window_state(window: &WebviewWindow) {
    let Some(state) = store::read_text(&window_state_file())
        .and_then(|s| serde_json::from_str::<WindowState>(&s).ok())
    else {
        let _ = window.center();
        return;
    };
    // Only restore onto a monitor that still exists.
    let visible = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|m| {
            let (p, s) = (m.position(), m.size());
            state.x + 80 < p.x + s.width as i32
                && state.x + state.width as i32 - 80 > p.x
                && state.y >= p.y - 8
                && state.y + 40 < p.y + s.height as i32
        });
    if visible && state.width >= 400 && state.height >= 300 {
        let _ = window.set_size(PhysicalSize::new(state.width, state.height));
        let _ = window.set_position(PhysicalPosition::new(state.x, state.y));
    } else {
        let _ = window.center();
    }
    if state.maximized {
        RESTORE_MAXIMIZED.store(true, Ordering::SeqCst);
    }
    if let Ok(mut guard) = WINDOW_STATE.lock() {
        *guard = Some(state);
    }
}

fn browser_args() -> String {
    // msWebOOUI: Edge "mini menu" on text selection; msPdfOOUI: Edge PDF toolbar pop-ups.
    #[allow(unused_mut)]
    let mut args = String::from("--disable-features=msWebOOUI,msPdfOOUI");
    #[cfg(any(debug_assertions, feature = "remote-debug"))]
    if let Ok(port) = std::env::var("ZENITH_REMOTE_DEBUG_PORT") {
        if port.parse::<u16>().is_ok() {
            args.push_str(" --remote-debugging-port=");
            args.push_str(&port);
        }
    }
    args
}

/// An incognito or extra window opens over the window the user is in (else the main one),
/// offset like a cascaded window.
fn place_cascaded(app: &AppHandle, window: &WebviewWindow) {
    let others: Vec<WebviewWindow> = app.webview_windows().into_values().filter(|w| w.label() != window.label()).collect();
    let base = others
        .iter()
        .find(|w| w.is_focused().unwrap_or(false))
        .or_else(|| others.iter().find(|w| w.label() == host::MAIN))
        .or_else(|| others.first())
        .cloned();
    let Some(main) = base else {
        let _ = window.center();
        return;
    };
    if main.is_maximized().unwrap_or(false) {
        let _ = window.center();
        return;
    }
    if let (Ok(pos), Ok(size)) = (main.outer_position(), main.inner_size()) {
        let step = (32.0 * main.scale_factor().unwrap_or(1.0)) as i32;
        let _ = window.set_size(size);
        let _ = window.set_position(PhysicalPosition::new(pos.x + step, pos.y + step));
    }
}

/// Builds a browser window: the normal window (`main`) or an incognito one. Every window
/// passes the same WebView2 options, so all of them share one WebView2 browser process.
fn create_window(app: &AppHandle, label: &str) -> tauri::Result<()> {
    let private = host::is_private_window(label);
    let extra = sessions::is_extra_window(label);
    let title = window_title(private);
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(&title)
        .inner_size(1280.0, 820.0)
        .min_inner_size(560.0, 400.0)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .visible(false);
    // Mica needs Windows 11; Windows 10 gets the solid theme (the UI paints its own background).
    if win::is_windows_11() {
        builder = builder.effects(WindowEffectsConfig {
            effects: vec![Effect::Mica],
            ..Default::default()
        });
    }
    if private {
        // Known to the UI before its first render, so it never restores or saves a session.
        builder = builder.initialization_script("window.__ZENITH_INCOGNITO__ = true;");
    }
    let window = builder
        .data_directory(host::paths().webview_data())
        .additional_browser_args(&browser_args())
        .browser_extensions_enabled(true)
        .scroll_bar_style(ScrollBarStyle::FluentOverlay)
        .disable_drag_drop_handler()
        .zoom_hotkeys_enabled(false)
        .devtools(cfg!(debug_assertions))
        .build()?;
    if private || extra {
        place_cascaded(app, &window);
    } else {
        restore_window_state(&window);
    }
    let w = window.clone();
    window.with_webview(move |pw| host::attach_chrome(w, pw.controller(), pw.environment()))?;
    Ok(())
}

/// Opens a new incognito window that loads `urls` once its UI is ready. Call off the UI thread.
/// Window title: "Zenith", "Zenith — Work" for a non-default profile, "Zenith Incognito".
pub fn window_title(private: bool) -> String {
    let active = profiles::active();
    let name = (!active.is_default()).then(|| active.profile().name);
    match (private, host::profile_override().is_some(), name) {
        (true, _, None) => "Zenith Incognito".into(),
        (true, _, Some(n)) => format!("Zenith Incognito — {n}"),
        (false, true, _) => "Zenith (test instance)".into(),
        (false, false, None) => "Zenith".into(),
        (false, false, Some(n)) => format!("Zenith — {n}"),
    }
}

/// Web addresses on a command line (skipping the program itself and `--profile <id>`).
fn url_args(argv: &[String]) -> Vec<String> {
    profiles::without_flag(argv).iter().skip(1).filter_map(|a| urls::external_arg_to_url(a)).collect()
}

pub fn open_incognito_window(app: &AppHandle, urls: Vec<String>) -> tauri::Result<()> {
    let label = format!("{}{}", incognito::LABEL_PREFIX, NEXT_INCOGNITO.fetch_add(1, Ordering::Relaxed));
    commands::set_startup_urls(&label, urls);
    create_window(app, &label)
}

/// Opens another normal window (`window-N`) that starts with `start`. Call off the UI thread.
pub fn open_normal_window(app: &AppHandle, start: sessions::Start) -> tauri::Result<()> {
    let label = format!("{}{}", sessions::LABEL_PREFIX, NEXT_WINDOW.fetch_add(1, Ordering::Relaxed));
    sessions::set_pending(&label, start);
    create_window(app, &label)
}

/// Reopens the main window (closed while other windows stayed open) with all of its tabs.
pub fn reopen_main_window(app: &AppHandle) -> tauri::Result<()> {
    sessions::set_pending(host::MAIN, sessions::Start { data: None, keep: Some(true), fresh: false });
    create_window(app, host::MAIN)
}

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
    sessions::window_closed(label);
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

/// Installer/maintenance entry points that must run before the single-instance check.
fn maintenance_command() -> Option<i32> {
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

fn main() {
    let _ = host::STARTED.set(std::time::Instant::now());
    if let Some(code) = maintenance_command() {
        std::process::exit(code);
    }
    let args: Vec<String> = std::env::args().collect();
    let start_incognito = incognito::requested(&args);
    let startup = url_args(&args);

    // Pick the profile before anything touches the disk. Each non-default profile runs under its
    // own app identifier, which keys the single-instance lock below (see profiles.rs).
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
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
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
                    commands::set_startup_urls(host::MAIN, urls);
                    let app = app.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = create_window(&app, host::MAIN) {
                            eprintln!("[zenith] could not reopen the window: {e}");
                        }
                    });
                }
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::ui_ready,
            commands::app_quit,
            commands::window_close,
            commands::incognito_open,
            commands::window_open,
            commands::window_undo_close,
            commands::windows_restore,
            commands::chrome_devtools,
            commands::pick_file,
            commands::tab_create,
            commands::tab_close,
            commands::tab_navigate,
            commands::tab_command,
            commands::tab_sleep,
            commands::tab_set_glance,
            commands::tab_content_size,
            commands::set_layout,
            commands::focus_view,
            commands::find_in_page,
            commands::permission_respond,
            commands::set_shortcuts,
            commands::apply_settings,
            commands::clear_browsing_data,
            commands::session_load,
            commands::session_save,
            commands::settings_load,
            commands::wallpaper_save,
            commands::wallpaper_load,
            commands::wallpaper_delete,
            commands::settings_save,
            commands::history_search,
            commands::history_top,
            commands::history_delete,
            commands::history_clear,
            commands::history_prune,
            commands::bookmarks_list,
            commands::bookmarks_add,
            commands::bookmark_update,
            commands::bookmark_move,
            commands::bookmarks_reorder,
            commands::bookmarks_remove,
            commands::bookmarks_remove_url,
            commands::bookmarks_export,
            commands::bookmarks_import,
            commands::bookmarks_backup,
            commands::bookmarks_restore,
            commands::bookmark_backups,
            commands::bookmarks_restore_backup,
            commands::bookmarks_auto_backup,
            commands::favicon_fetch,
            commands::notes_list,
            commands::note_save,
            commands::note_delete,
            commands::import_sources,
            commands::import_run,
            commands::downloads_active,
            commands::downloads_history,
            commands::download_action,
            commands::downloads_remove,
            commands::media_list,
            commands::media_download,
            commands::media_tools,
            commands::media_pick_ffmpeg,
            commands::open_path,
            commands::show_in_folder,
            commands::pick_folder,
            commands::blocker_configure,
            commands::blocker_set_disabled_sites,
            commands::blocker_stats,
            commands::ext_list,
            commands::ext_install_folder,
            commands::ext_install_crx,
            commands::ext_install_store,
            commands::ext_offer_accept,
            commands::ext_offer_discard,
            commands::ext_check_updates,
            commands::ext_update,
            commands::ext_remove,
            commands::ext_set_enabled,
            commands::default_browser_status,
            commands::default_browser_register,
            commands::default_browser_unregister,
            commands::search_suggest,
            commands::memory_stats,
            commands::system_memory_load,
            commands::clipboard_write,
            commands::debug_layout,
            commands::debug_cdp,
            commands::update_status,
            commands::update_check,
            commands::update_restart,
            password_commands::vault_status,
            password_commands::vault_setup,
            password_commands::vault_unlock,
            password_commands::vault_lock,
            password_commands::vault_change_master,
            password_commands::vault_recover,
            password_commands::vault_new_recovery_key,
            password_commands::vault_reset,
            password_commands::passwords_list,
            password_commands::password_reveal,
            password_commands::password_copy,
            password_commands::password_add,
            password_commands::password_update,
            password_commands::passwords_delete,
            password_commands::password_generate,
            password_commands::passwords_never_list,
            password_commands::passwords_never_remove,
            password_commands::passwords_export_csv,
            password_commands::passwords_backup,
            password_commands::passwords_import_open,
            password_commands::passwords_import_unlock,
            password_commands::passwords_import_commit,
            password_commands::passwords_import_cancel,
            password_commands::autofill_fill,
            password_commands::autofill_fill_generated,
            password_commands::autofill_dismiss,
            password_commands::password_offer_respond,
        ])
            profile_commands::profiles_list,
            profile_commands::profile_create,
            profile_commands::profile_update,
            profile_commands::profile_open,
            profile_commands::profile_delete,
            profile_commands::profile_shortcut,
        .setup(move |app| {
            host::init_services(app.handle())?;
            // Locks the password vault after the configured inactivity.
            vault::start_timer();
            // `zenith --incognito` with no Zenith running opens only an incognito window.
            if start_incognito {
                let label = format!("{}{}", incognito::LABEL_PREFIX, NEXT_INCOGNITO.fetch_add(1, Ordering::Relaxed));
                commands::set_startup_urls(&label, startup);
                create_window(app.handle(), &label)?;
            } else {
                commands::set_startup_urls(host::MAIN, startup);
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
