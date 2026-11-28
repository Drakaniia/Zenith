//! Runtime settings pushed by the UI, and the content blocker state.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};

use serde::Deserialize;

use crate::app::runtime::post_ui;
use crate::blocker::Engine;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct RuntimeConfig {
    pub download_dir: Option<PathBuf>,
    pub record_history: bool,
    pub block_popups: bool,
    pub blocker_enabled: bool,
    pub media_button: bool,
    pub media_disabled_sites: Vec<String>,
    pub use_ffmpeg: bool,
    pub ffmpeg_path: Option<PathBuf>,
    pub use_ytdlp: bool,
    pub ytdlp_path: Option<PathBuf>,
    pub auto_update: bool,
    pub preview_updates: bool,
    pub torrents_enabled: bool,
    pub torrent_seed_ratio: f64,
    pub torrent_down_kib: u32,
    pub torrent_up_kib: u32,
    pub torrent_choose_files: bool,
    pub fast_download: bool,
    pub passwords_enabled: bool,
    pub passwords_offer_save: bool,
    pub cards_offer_save: bool,
    pub autofill_cards: bool,
    pub autofill_suggest: bool,
    pub autofill_usernames: bool,
    pub autofill_passwords: bool,
    pub autofill_on_load: bool,
    pub autofill_same_site: bool,
    pub password_generator: bool,
    pub vault_ask_every_time: bool,
    pub vault_lock_minutes: u32,
    pub extensions_in_incognito: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            download_dir: None,
            record_history: true,
            block_popups: true,
            blocker_enabled: true,
            media_button: true,
            media_disabled_sites: Vec::new(),
            use_ffmpeg: true,
            ffmpeg_path: None,
            use_ytdlp: true,
            ytdlp_path: None,
            auto_update: true,
            preview_updates: false,
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
            extensions_in_incognito: true,
        }
    }
}

pub static CONFIG: LazyLock<RwLock<RuntimeConfig>> = LazyLock::new(|| RwLock::new(RuntimeConfig::default()));

pub fn config() -> RuntimeConfig {
    CONFIG.read().map(|c| c.clone()).unwrap_or_default()
}

static BLOCKER_ON: AtomicBool = AtomicBool::new(true);

pub fn blocker_enabled() -> bool {
    BLOCKER_ON.load(Ordering::Relaxed)
}

pub fn apply_config(cfg: RuntimeConfig) {
    let blocking = cfg.blocker_enabled;
    if BLOCKER_ON.swap(blocking, Ordering::Relaxed) != blocking {
        let _ = post_ui(move || super::tab::apply_blocking(blocking));
    }
    crate::torrents::apply_limits(&cfg);
    let button = cfg.media_button;
    let ext_private = cfg.extensions_in_incognito;
    let mut ext_private_changed = false;
    let changed = match CONFIG.write() {
        Ok(mut c) => {
            let changed = c.media_button != button || c.media_disabled_sites != cfg.media_disabled_sites;
            ext_private_changed = c.extensions_in_incognito != ext_private;
            *c = cfg;
            changed
        }
        Err(_) => false,
    };
    if changed {
        let _ = post_ui(move || crate::media::sniff::apply_button_setting(button));
    }
    if ext_private_changed {
        crate::extensions::private_profile::sync();
    }
}

pub fn set_extensions_in_incognito(on: bool) {
    let changed = match CONFIG.write() {
        Ok(mut c) => std::mem::replace(&mut c.extensions_in_incognito, on) != on,
        Err(_) => false,
    };
    if changed {
        crate::extensions::private_profile::sync();
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
