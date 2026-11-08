#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Zenith: a lightweight Zen-style browser for Windows (Tauri 2 + WebView2).

mod app;
mod blocker;
mod bookmarks;
mod card_commands;
mod downloads;
mod extensions;
mod history;
mod host;
mod importer;
mod incognito;
mod media;
mod notes;
mod password_commands;
mod platform;
mod profile_commands;
mod profiles;
mod search;
mod session;
mod settings;
mod shortcuts;
mod storage;
mod updates;
mod vault;

fn main() {
    app::run();
}
