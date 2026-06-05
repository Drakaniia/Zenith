pub mod bridge;
pub mod core;
pub mod features;
pub mod persistence;
pub mod runtime;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(generate_ampui_handler!())
        .run(tauri::generate_context!())
        .expect("error while running Rust Browser");
}
