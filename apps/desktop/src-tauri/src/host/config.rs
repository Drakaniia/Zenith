//! Runtime settings pushed by the UI, and the content blocker state.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};

use serde::Deserialize;

use super::state::APP;
use crate::blocker::Engine;

/// Settings the backend needs at runtime; pushed by the UI with `apply_settings`.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct RuntimeConfig {
    pub download_dir: Option<PathBuf>,
    pub record_history: bool,
    pub block_popups: bool,
    pub blocker_enabled: bool,
    /// Show the media downloader's button over videos in pages.
    pub media_button: bool,
    /// Use FFmpeg (when found) to convert .ts recordings and merge streams.
    pub use_ffmpeg: bool,
    /// FFmpeg chosen by the user; otherwise it is looked up on PATH.
    pub ffmpeg_path: Option<PathBuf>,
    /// Check for updates in the background and install them when Zenith closes.
    pub auto_update: bool,
    /// Torrents: open magnet links and downloaded .torrent files in Zenith.
    pub torrents_enabled: bool,
    /// Stop seeding a finished torrent once it uploaded this many times its size (0: don't seed).
    pub torrent_seed_ratio: f64,
    /// Torrent speed limits in KiB/s (0: unlimited).
    pub torrent_down_kib: u32,
    pub torrent_up_kib: u32,
    /// Ask which files to download when a torrent has several.
    pub torrent_choose_files: bool,
    /// Hidden fast mode: files are fetched over several connections at once (byte ranges), and
    /// page downloads go through Zenith's own downloader to get there.
    pub fast_download: bool,
    /// Password manager: off means no saving, suggestions or filling.
    pub passwords_enabled: bool,
    /// Offer to save logins submitted on pages.
    pub passwords_offer_save: bool,
    /// Offer to save cards submitted on payment forms.
    pub cards_offer_save: bool,
    /// Show saved cards when a card field is clicked (filling always asks for the master password).
    pub autofill_cards: bool,
    /// Show saved accounts when a login field is clicked.
    pub autofill_suggest: bool,
    pub autofill_usernames: bool,
    pub autofill_passwords: bool,
    /// Fill the only saved account as soon as a login form appears (vault unlocked, exact origin).
    pub autofill_on_load: bool,
    /// Offer credentials on other subdomains of the site they were saved on.
    pub autofill_same_site: bool,
    /// Suggest a generated password in sign-up and change-password forms.
    pub password_generator: bool,
    /// Ask for the master password before every fill, reveal or copy, even when unlocked.
    pub vault_ask_every_time: bool,
    /// Lock after this many minutes without using a password (0: only when Zenith closes).
    pub vault_lock_minutes: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            download_dir: None,
            record_history: true,
            block_popups: true,
            blocker_enabled: true,
            media_button: true,
            use_ffmpeg: true,
            ffmpeg_path: None,
            auto_update: true,
            torrents_enabled: true,
            torrent_seed_ratio: 1.0,
            torrent_down_kib: 0,
            torrent_up_kib: 0,
            torrent_choose_files: true,
            fast_download: false,
            passwords_enabled: true,
            passwords_offer_save: true,
            cards_offer_save: true,
            autofill_cards: true,
            autofill_suggest: true,
            autofill_usernames: true,
            autofill_passwords: true,
            autofill_on_load: false,
            autofill_same_site: true,
            password_generator: true,
            vault_ask_every_time: false,
            vault_lock_minutes: 15,
        }
    }
}

pub static CONFIG: LazyLock<RwLock<RuntimeConfig>> = LazyLock::new(|| RwLock::new(RuntimeConfig::default()));

pub fn config() -> RuntimeConfig {
    CONFIG.read().map(|c| c.clone()).unwrap_or_default()
}

static BLOCKER_ON: AtomicBool = AtomicBool::new(true);

/// Hot-path check used for every network request.
pub fn blocker_enabled() -> bool {
    BLOCKER_ON.load(Ordering::Relaxed)
}

pub fn apply_config(cfg: RuntimeConfig) {
    let blocking = cfg.blocker_enabled;
    if BLOCKER_ON.swap(blocking, Ordering::Relaxed) != blocking {
        // Tabs only intercept every request while the blocker is on.
        if let Some(app) = APP.get() {
            let _ = app.run_on_main_thread(move || super::tab::apply_blocking(blocking));
        }
    }
    crate::torrents::apply_limits(&cfg);
    let button = cfg.media_button;
    let changed = match CONFIG.write() {
        Ok(mut c) => {
            let changed = c.media_button != button;
            *c = cfg;
            changed
        }
        Err(_) => false,
    };
    if changed {
        if let Some(app) = APP.get() {
            let _ = app.run_on_main_thread(move || crate::media::sniff::apply_button_setting(button));
        }
    }
}

pub struct BlockerState {
    pub engine: Engine,
    pub disabled_sites: std::collections::HashSet<String>,
}

pub static BLOCKER: LazyLock<RwLock<BlockerState>> = LazyLock::new(|| {
    RwLock::new(BlockerState {
        engine: Engine::with_builtin(),
        disabled_sites: Default::default(),
    })
});
