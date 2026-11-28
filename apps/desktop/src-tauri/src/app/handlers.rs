//! Typed JSON dispatcher for every command the trusted chrome UI can invoke.
//! Preserves all existing command names, camelCase argument keys, and JSON return shapes.

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::runtime::{AppHandle, WindowHandle};

fn to_json<T: Serialize>(res: Result<T, String>) -> Result<serde_json::Value, String> {
    res.and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
}

fn decode<T: DeserializeOwned>(args: serde_json::Value) -> Result<T, String> {
    let val = if args.is_null() { serde_json::json!({}) } else { args };
    serde_json::from_value(val).map_err(|e| format!("invalid arguments: {e}"))
}

macro_rules! call {
    // No JSON args, no handles
    ($func:path, (), $app:expr, $win:expr, $args:expr) => {
        to_json($func().await)
    };
    // No JSON args, app only
    ($func:path, (@app), $app:expr, $win:expr, $args:expr) => {
        to_json($func($app).await)
    };
    // No JSON args, win only
    ($func:path, (@win), $app:expr, $win:expr, $args:expr) => {
        to_json($func($win).await)
    };
    // No JSON args, app + win
    ($func:path, (@app, @win), $app:expr, $win:expr, $args:expr) => {
        to_json($func($app, $win).await)
    };
    // No JSON args, win + app
    ($func:path, (@win, @app), $app:expr, $win:expr, $args:expr) => {
        to_json($func($win, $app).await)
    };
    // JSON args only
    ($func:path, ($($p:ident : $t:ty),+ $(,)?), $app:expr, $win:expr, $args:expr) => {{
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct A { $($p: $t),+ }
        let a: A = decode($args)?;
        to_json($func($(a.$p),+).await)
    }};
    // app + JSON args
    ($func:path, (@app, $($p:ident : $t:ty),+ $(,)?), $app:expr, $win:expr, $args:expr) => {{
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct A { $($p: $t),+ }
        let a: A = decode($args)?;
        to_json($func($app, $(a.$p),+).await)
    }};
    // win + JSON args
    ($func:path, (@win, $($p:ident : $t:ty),+ $(,)?), $app:expr, $win:expr, $args:expr) => {{
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct A { $($p: $t),+ }
        let a: A = decode($args)?;
        to_json($func($win, $(a.$p),+).await)
    }};
    // app + win + JSON args
    ($func:path, (@app, @win, $($p:ident : $t:ty),+ $(,)?), $app:expr, $win:expr, $args:expr) => {{
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct A { $($p: $t),+ }
        let a: A = decode($args)?;
        to_json($func($app, $win, $(a.$p),+).await)
    }};
}

/// Dispatches an authorized trusted-UI command to its domain implementation.
pub async fn dispatch(
    app: AppHandle,
    win: WindowHandle,
    cmd: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    use std::collections::HashMap;
    match cmd {
        // App & window lifecycle
        "ui_ready" => call!(super::commands::ui_ready, (@app, @win), app, win, args),
        "app_quit" => call!(super::commands::app_quit, (@app), app, win, args),
        "window_close" => call!(super::commands::window_close, (@app, @win), app, win, args),
        "incognito_open" => call!(crate::incognito::commands::incognito_open, (urls: Option<Vec<String>>), app, win, args),
        "window_open" => call!(crate::session::commands::window_open, (@app, session: Option<String>), app, win, args),
        "window_undo_close" => call!(crate::session::commands::window_undo_close, (@app), app, win, args),
        "windows_restore" => call!(crate::session::commands::windows_restore, (@app, @win, keep: bool), app, win, args),
        "chrome_devtools" => call!(super::commands::chrome_devtools, (@app, @win), app, win, args),
        "pick_file" => call!(super::commands::pick_file, (@app), app, win, args),
        "open_path" => call!(super::commands::open_path, (path: String), app, win, args),
        "show_in_folder" => call!(super::commands::show_in_folder, (path: String), app, win, args),
        "pick_folder" => call!(super::commands::pick_folder, (@app, title: Option<String>), app, win, args),
        "clipboard_write" => call!(super::commands::clipboard_write, (text: String), app, win, args),
        "clipboard_read" => call!(super::commands::clipboard_read, (), app, win, args),
        "engine_info" => call!(super::commands::engine_info, (), app, win, args),
        "migration_notice_dismiss" => call!(super::commands::migration_notice_dismiss, (), app, win, args),

        // Tab host & engine
        "tab_create" => call!(crate::host::commands::tab_create, (@app, @win, args: crate::host::commands::TabCreateArgs), app, win, args),
        "tab_close" => call!(crate::host::commands::tab_close, (@app, id: String), app, win, args),
        "tab_navigate" => call!(crate::host::commands::tab_navigate, (@app, id: String, url: String), app, win, args),
        "tab_command" => call!(crate::host::commands::tab_command, (@app, id: String, cmd: String, arg: Option<serde_json::Value>), app, win, args),
        "tab_sleep" => call!(crate::host::commands::tab_sleep, (@app, id: String), app, win, args),
        "tab_content_size" => call!(crate::host::commands::tab_content_size, (@app, id: String), app, win, args),
        "tab_set_glance" => call!(crate::host::commands::tab_set_glance, (@app, id: String, glance: bool), app, win, args),
        "set_layout" => call!(crate::host::commands::set_layout, (@app, @win, spec: crate::host::LayoutSpec), app, win, args),
        "focus_view" => call!(crate::host::commands::focus_view, (@app, @win, target: Option<String>), app, win, args),
        "find_in_page" => call!(crate::host::commands::find_in_page, (@app, id: String, op: String, text: Option<String>, case_sensitive: Option<bool>, whole_word: Option<bool>), app, win, args),
        "permission_respond" => call!(crate::host::commands::permission_respond, (@app, request_id: u32, allow: bool, remember: bool), app, win, args),
        "clear_browsing_data" => call!(crate::host::commands::clear_browsing_data, (@app, @win, kinds: Vec<String>, since_ms: Option<i64>, history: bool), app, win, args),
        "debug_layout" => call!(crate::host::commands::debug_layout, (@app, @win), app, win, args),
        "debug_cdp" => call!(crate::host::commands::debug_cdp, (@app, method: String, params: Option<String>), app, win, args),

        // Shortcuts, settings & session
        "set_shortcuts" => call!(crate::shortcuts::commands::set_shortcuts, (@app, bindings: Vec<crate::shortcuts::keys::BindingSpec>), app, win, args),
        "apply_settings" => call!(crate::settings::commands::apply_settings, (@app, @win, config: crate::host::RuntimeConfig, appearance: crate::settings::commands::Appearance), app, win, args),
        "session_load" => call!(crate::session::commands::session_load, (@win), app, win, args),
        "session_save" => call!(crate::session::commands::session_save, (@win, data: String), app, win, args),
        "settings_load" => call!(crate::settings::commands::settings_load, (), app, win, args),
        "settings_save" => call!(crate::settings::commands::settings_save, (@win, data: String), app, win, args),
        "wallpaper_save" => call!(crate::settings::commands::wallpaper_save, (@win, id: String, data: String), app, win, args),
        "wallpaper_load" => call!(crate::settings::commands::wallpaper_load, (id: String), app, win, args),
        "wallpaper_delete" => call!(crate::settings::commands::wallpaper_delete, (@win, id: String), app, win, args),

        // History
        "history_search" => call!(crate::history::commands::history_search, (text: String, limit: Option<i64>, offset: Option<i64>), app, win, args),
        "history_top" => call!(crate::history::commands::history_top, (limit: Option<i64>), app, win, args),
        "history_delete" => call!(crate::history::commands::history_delete, (ids: Vec<i64>), app, win, args),
        "history_clear" => call!(crate::history::commands::history_clear, (since_ms: Option<i64>), app, win, args),
        "history_prune" => call!(crate::history::commands::history_prune, (days: i64), app, win, args),

        // Bookmarks
        "bookmarks_list" => call!(crate::bookmarks::commands::bookmarks_list, (), app, win, args),
        "bookmarks_add" => call!(crate::bookmarks::commands::bookmarks_add, (@win, parent: i64, index: Option<i64>, nodes: Vec<crate::bookmarks::NewNode>), app, win, args),
        "bookmark_update" => call!(crate::bookmarks::commands::bookmark_update, (@win, id: i64, patch: crate::bookmarks::NodePatch), app, win, args),
        "bookmark_move" => call!(crate::bookmarks::commands::bookmark_move, (@win, id: i64, parent: i64, index: i64), app, win, args),
        "bookmarks_reorder" => call!(crate::bookmarks::commands::bookmarks_reorder, (@win, parent: i64, order: Vec<i64>), app, win, args),
        "bookmarks_remove" => call!(crate::bookmarks::commands::bookmarks_remove, (@win, ids: Vec<i64>), app, win, args),
        "bookmarks_remove_url" => call!(crate::bookmarks::commands::bookmarks_remove_url, (@win, url: String), app, win, args),
        "bookmarks_export" => call!(crate::bookmarks::commands::bookmarks_export, (@app), app, win, args),
        "bookmarks_import" => call!(crate::bookmarks::commands::bookmarks_import, (@win, @app), app, win, args),
        "bookmarks_backup" => call!(crate::bookmarks::commands::bookmarks_backup, (@app), app, win, args),
        "bookmarks_restore" => call!(crate::bookmarks::commands::bookmarks_restore, (@win, @app), app, win, args),
        "bookmark_backups" => call!(crate::bookmarks::commands::bookmark_backups, (), app, win, args),
        "bookmarks_restore_backup" => call!(crate::bookmarks::commands::bookmarks_restore_backup, (@win, name: String), app, win, args),
        "bookmarks_auto_backup" => call!(crate::bookmarks::commands::bookmarks_auto_backup, (@win), app, win, args),
        "favicon_fetch" => call!(crate::bookmarks::commands::favicon_fetch, (url: String), app, win, args),

        // Notes & importer
        "notes_list" => call!(crate::notes::commands::notes_list, (), app, win, args),
        "note_save" => call!(crate::notes::commands::note_save, (@win, note: crate::notes::Note), app, win, args),
        "note_delete" => call!(crate::notes::commands::note_delete, (@win, id: String), app, win, args),
        "import_sources" => call!(crate::importer::commands::import_sources, (), app, win, args),
        "import_run" => call!(crate::importer::commands::import_run, (@win, source: String, bookmarks: bool, history: bool, history_days: i64), app, win, args),

        // Downloads, torrents & media
        "downloads_active" => call!(crate::downloads::commands::downloads_active, (@app), app, win, args),
        "downloads_history" => call!(crate::downloads::commands::downloads_history, (@app, limit: Option<i64>), app, win, args),
        "download_action" => call!(crate::downloads::commands::download_action, (@app, id: i64, action: String), app, win, args),
        "download_again" => call!(crate::downloads::commands::download_again, (@app, id: i64), app, win, args),
        "downloads_remove" => call!(crate::downloads::commands::downloads_remove, (id: Option<i64>), app, win, args),
        "download_drag" => call!(crate::downloads::commands::download_drag, (@app, @win, id: i64), app, win, args),
        "torrent_add_magnet" => call!(crate::torrents::commands::torrent_add_magnet, (@win, uri: String), app, win, args),
        "torrent_add_bytes" => call!(crate::torrents::commands::torrent_add_bytes, (@win, bytes: Vec<u8>), app, win, args),
        "torrent_open_file" => call!(crate::torrents::commands::torrent_open_file, (@app, @win), app, win, args),
        "torrent_files" => call!(crate::torrents::commands::torrent_files, (id: i64), app, win, args),
        "torrent_choose_files" => call!(crate::torrents::commands::torrent_choose_files, (id: i64, files: Vec<usize>), app, win, args),
        "torrent_stream_url" => call!(crate::torrents::commands::torrent_stream_url, (id: i64, file: usize), app, win, args),
        "media_list" => call!(crate::media::commands::media_list, (@app, tab_id: String, scan: Option<bool>), app, win, args),
        "media_download" => call!(crate::media::commands::media_download, (@app, tab_id: String, item_id: u32, variant: Option<String>, audio: Option<String>, audio_format: Option<String>), app, win, args),
        "media_tools" => call!(crate::media::commands::media_tools, (), app, win, args),
        "media_pick_ffmpeg" => call!(crate::media::commands::media_pick_ffmpeg, (@app), app, win, args),
        "media_pick_ytdlp" => call!(crate::media::commands::media_pick_ytdlp, (@app), app, win, args),
        "media_install_tool" => call!(crate::media::commands::media_install_tool, (tool: String), app, win, args),
        "media_remove_tool" => call!(crate::media::commands::media_remove_tool, (tool: String), app, win, args),
        "media_probe_url" => call!(crate::media::commands::media_probe_url, (url: String, prefer_ytdlp: Option<bool>), app, win, args),
        "media_download_probed" => call!(crate::media::commands::media_download_probed, (probe_id: String, item_id: u32, variant: Option<String>, audio: Option<String>, audio_format: Option<String>, tab_id: Option<String>), app, win, args),

        // Blocker & extensions
        "blocker_configure" => call!(crate::blocker::commands::blocker_configure, (lists: Vec<crate::blocker::commands::FilterList>, update: bool, disabled_sites: Vec<String>), app, win, args),
        "blocker_set_disabled_sites" => call!(crate::blocker::commands::blocker_set_disabled_sites, (sites: Vec<String>), app, win, args),
        "blocker_stats" => call!(crate::blocker::commands::blocker_stats, (), app, win, args),
        "ext_list" => call!(crate::extensions::commands::ext_list, (@app), app, win, args),
        "ext_install_folder" => call!(crate::extensions::commands::ext_install_folder, (@app, @win, path: Option<String>), app, win, args),
        "ext_install_crx" => call!(crate::extensions::commands::ext_install_crx, (@app, @win), app, win, args),
        "ext_install_store" => call!(crate::extensions::commands::ext_install_store, (@app, @win, input: String), app, win, args),
        "ext_offer_accept" => call!(crate::extensions::commands::ext_offer_accept, (@app, @win, token: u64), app, win, args),
        "ext_offer_discard" => call!(crate::extensions::commands::ext_offer_discard, (token: u64), app, win, args),
        "ext_check_updates" => call!(crate::extensions::commands::ext_check_updates, (), app, win, args),
        "ext_update" => call!(crate::extensions::commands::ext_update, (@app, @win, id: String), app, win, args),
        "ext_remove" => call!(crate::extensions::commands::ext_remove, (@app, id: String), app, win, args),
        "ext_set_enabled" => call!(crate::extensions::commands::ext_set_enabled, (@app, id: String, enabled: bool), app, win, args),
        "ext_set_incognito" => call!(crate::extensions::commands::ext_set_incognito, (@win, id: String, allowed: bool), app, win, args),
        "ext_sync_state" => call!(crate::extensions::commands::ext_sync_state, (snapshot: crate::extensions::bridge::WindowSnapshot), app, win, args),
        "ext_badges" => call!(crate::extensions::commands::ext_badges, (), app, win, args),
        "ext_set_badge" => call!(crate::extensions::commands::ext_set_badge, (extension_id: String, tab_id: Option<String>, badge: crate::extensions::bridge::ActionBadge), app, win, args),
        "ext_query_tabs" => call!(crate::extensions::commands::ext_query_tabs, (@win, query: crate::extensions::bridge::TabQuery), app, win, args),
        "ext_native_hosts" => call!(crate::extensions::commands::ext_native_hosts, (), app, win, args),
        "ext_native_host_consent" => call!(crate::extensions::commands::ext_native_host_consent, (@win, host_name: String, allowed: bool), app, win, args),

        // Security, Safe Browsing, certificate errors, external protocols & download warnings
        "security_respond_cert" => call!(crate::security::commands::security_respond_cert, (@app, request_id: u32, proceed: bool), app, win, args),
        "security_respond_protocol" => call!(crate::security::commands::security_respond_protocol, (@app, request_id: u32, allow: bool), app, win, args),
        "security_respond_download" => call!(crate::security::commands::security_respond_download, (@app, request_id: u32, allow: bool), app, win, args),
        "security_safebrowsing_status" => call!(crate::security::commands::security_safebrowsing_status, (), app, win, args),
        "security_safebrowsing_update" => call!(crate::security::commands::security_safebrowsing_update, (), app, win, args),
        "security_safebrowsing_bypass" => call!(crate::security::commands::security_safebrowsing_bypass, (@app, tab_id: String, url: String), app, win, args),

        // Windows integration, search, updates
        "default_browser_status" => call!(crate::platform::win::commands::default_browser_status, (), app, win, args),
        "default_browser_register" => call!(crate::platform::win::commands::default_browser_register, (open_settings: Option<bool>), app, win, args),
        "default_browser_unregister" => call!(crate::platform::win::commands::default_browser_unregister, (), app, win, args),
        "search_suggest" => call!(crate::search::commands::search_suggest, (engine: String, query: String), app, win, args),
        "memory_stats" => call!(crate::platform::win::commands::memory_stats, (), app, win, args),
        "system_memory_load" => call!(crate::platform::win::commands::system_memory_load, (), app, win, args),
        "update_status" => call!(crate::updates::commands::update_status, (), app, win, args),
        "update_check" => call!(crate::updates::commands::update_check, (@app, @win), app, win, args),
        "update_restart" => call!(crate::updates::commands::update_restart, (@app, @win), app, win, args),

        // Vault & cards
        "vault_status" => call!(crate::vault::commands::vault_status, (), app, win, args),
        "vault_setup" => call!(crate::vault::commands::vault_setup, (master: String), app, win, args),
        "vault_unlock" => call!(crate::vault::commands::vault_unlock, (master: String), app, win, args),
        "vault_lock" => call!(crate::vault::commands::vault_lock, (), app, win, args),
        "vault_change_master" => call!(crate::vault::commands::vault_change_master, (current: String, new: String), app, win, args),
        "vault_recover" => call!(crate::vault::commands::vault_recover, (recovery_key: String, new_master: String), app, win, args),
        "vault_new_recovery_key" => call!(crate::vault::commands::vault_new_recovery_key, (master: String), app, win, args),
        "vault_reset" => call!(crate::vault::commands::vault_reset, (confirm: String), app, win, args),
        "passwords_list" => call!(crate::vault::commands::passwords_list, (), app, win, args),
        "password_reveal" => call!(crate::vault::commands::password_reveal, (id: String, master: Option<String>), app, win, args),
        "password_copy" => call!(crate::vault::commands::password_copy, (id: String, field: String, master: Option<String>), app, win, args),
        "password_add" => call!(crate::vault::commands::password_add, (entry: crate::vault::EntryInput), app, win, args),
        "password_update" => call!(crate::vault::commands::password_update, (id: String, entry: crate::vault::EntryInput), app, win, args),
        "passwords_delete" => call!(crate::vault::commands::passwords_delete, (ids: Vec<String>), app, win, args),
        "password_generate" => call!(crate::vault::commands::password_generate, (length: Option<usize>), app, win, args),
        "passwords_never_list" => call!(crate::vault::commands::passwords_never_list, (), app, win, args),
        "passwords_never_remove" => call!(crate::vault::commands::passwords_never_remove, (site: String), app, win, args),
        "passwords_export_csv" => call!(crate::vault::commands::passwords_export_csv, (@app, master: String), app, win, args),
        "passwords_backup" => call!(crate::vault::commands::passwords_backup, (@app, master: String), app, win, args),
        "passwords_import_open" => call!(crate::vault::commands::passwords_import_open, (@app), app, win, args),
        "passwords_import_unlock" => call!(crate::vault::commands::passwords_import_unlock, (token: u64, password: String), app, win, args),
        "passwords_import_commit" => call!(crate::vault::commands::passwords_import_commit, (token: u64, choices: HashMap<usize, String>), app, win, args),
        "passwords_import_cancel" => call!(crate::vault::commands::passwords_import_cancel, (token: u64), app, win, args),
        "autofill_fill" => call!(crate::vault::commands::autofill_fill, (@app, tab_id: String, entry_id: String, master: Option<String>), app, win, args),
        "autofill_fill_generated" => call!(crate::vault::commands::autofill_fill_generated, (@app, tab_id: String), app, win, args),
        "autofill_dismiss" => call!(crate::vault::commands::autofill_dismiss, (@app, tab_id: String), app, win, args),
        "password_offer_respond" => call!(crate::vault::commands::password_offer_respond, (offer_id: u64, action: String, master: Option<String>), app, win, args),
        "cards_list" => call!(crate::vault::card_commands::cards_list, (), app, win, args),
        "card_reveal" => call!(crate::vault::card_commands::card_reveal, (id: String, master: Option<String>), app, win, args),
        "card_copy" => call!(crate::vault::card_commands::card_copy, (id: String, field: String, master: Option<String>), app, win, args),
        "card_add" => call!(crate::vault::card_commands::card_add, (card: crate::vault::cards::CardInput), app, win, args),
        "card_update" => call!(crate::vault::card_commands::card_update, (id: String, card: crate::vault::cards::CardInput), app, win, args),
        "cards_delete" => call!(crate::vault::card_commands::cards_delete, (ids: Vec<String>), app, win, args),
        "card_offer_respond" => call!(crate::vault::card_commands::card_offer_respond, (offer_id: u64, action: String, save_cvc: Option<bool>, master: Option<String>), app, win, args),
        "autofill_fill_card" => call!(crate::vault::card_commands::autofill_fill_card, (@app, tab_id: String, card_id: String, master: Option<String>), app, win, args),

        // Profiles
        "profiles_list" => call!(crate::profiles::commands::profiles_list, (), app, win, args),
        "profile_create" => call!(crate::profiles::commands::profile_create, (@win, name: String, color: String, avatar: Option<String>, icon: Option<String>, shortcut: bool), app, win, args),
        "profile_update" => call!(crate::profiles::commands::profile_update, (@app, @win, id: String, name: String, color: String, avatar: Option<String>, icon: Option<String>), app, win, args),
        "profile_open" => call!(crate::profiles::commands::profile_open, (@app, @win, id: String), app, win, args),
        "profile_delete" => call!(crate::profiles::commands::profile_delete, (@win, id: String), app, win, args),
        "profile_shortcut" => call!(crate::profiles::commands::profile_shortcut, (@win, id: String), app, win, args),

        _ => Err(format!("unknown_command: {cmd}")),
    }
}
