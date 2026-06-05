pub mod commands;
pub mod models;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::bookmarks::get_bookmarks,
            commands::bookmarks::save_bookmark,
            commands::bookmarks::delete_bookmark,
            commands::bookmarks::is_bookmarked,
            commands::downloads::get_downloads,
            commands::downloads::start_download,
            commands::downloads::cancel_download,
            commands::downloads::clear_completed_downloads,
            commands::history::get_history,
            commands::history::add_history_entry,
            commands::history::search_history,
            commands::history::clear_history,
            commands::history::delete_history_entry,
            commands::navigation::normalize_url,
            commands::navigation::fetch_page_metadata,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::webviews::create_browser_webview,
            commands::webviews::navigate_browser_webview,
            commands::webviews::reload_browser_webview,
            commands::webviews::close_browser_webview,
            commands::webviews::show_browser_webview,
            commands::webviews::hide_browser_webview,
            commands::webviews::set_browser_webview_bounds,
            commands::webviews::get_browser_webview_url,
            commands::webviews::eval_browser_webview,
            commands::webviews::find_in_browser_webview,
            commands::webviews::clear_browser_webview_find,
            commands::window::get_window_state,
            commands::window::minimize_window,
            commands::window::maximize_window,
            commands::window::restore_window,
            commands::window::close_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rust Browser");
}
