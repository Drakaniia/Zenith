//! Logins captured on a page waiting to be saved, and imports waiting for a decision or for a
//! backup's password. Both are kept in memory only.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use super::*;

// -- save offers ------------------------------------------------------------------------------

/// A login captured on a page, waiting for the user to save it.
pub struct Offer {
    pub tab: String,
    pub origin: String,
    pub username: String,
    pub password: Zeroizing<String>,
    pub update: Option<String>,
    pub(super) at: Instant,
}

pub(super) static OFFERS: LazyLock<Mutex<HashMap<u64, Offer>>> = LazyLock::new(Default::default);
pub(super) static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
pub(super) const OFFER_TTL: Duration = Duration::from_secs(15 * 60);

pub fn add_offer(tab: &str, origin: &str, username: &str, password: Zeroizing<String>, update: Option<String>) -> u64 {
    let id = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut o) = OFFERS.lock() {
        // One offer per tab: a newer login replaces the previous one.
        o.retain(|_, x| x.tab != tab);
        o.insert(id, Offer { tab: tab.into(), origin: origin.into(), username: username.into(), password, update, at: Instant::now() });
    }
    id
}

pub fn take_offer(id: u64) -> Option<Offer> {
    OFFERS.lock().ok()?.remove(&id)
}

pub fn peek_offer(id: u64) -> Option<(String, String, Option<String>)> {
    OFFERS.lock().ok()?.get(&id).map(|o| (o.origin.clone(), o.username.clone(), o.update.clone()))
}

/// Puts an offer back (saving needed the master password first).
pub fn restore_offer(id: u64, offer: Offer) {
    if let Ok(mut o) = OFFERS.lock() {
        o.insert(id, offer);
    }
}

pub fn drop_tab_offers(tab: &str) {
    if let Ok(mut o) = OFFERS.lock() {
        o.retain(|_, x| x.tab != tab);
    }
}

// -- pending imports ----------------------------------------------------------------------------

pub enum PendingImport {
    Rows(Vec<ImportRow>),
    /// An encrypted backup waiting for its password.
    Backup(Vec<u8>),
}

static IMPORTS: LazyLock<Mutex<HashMap<u64, PendingImport>>> = LazyLock::new(Default::default);

pub fn stash_import(p: PendingImport) -> u64 {
    let id = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut m) = IMPORTS.lock() {
        m.insert(id, p);
    }
    id
}

pub fn take_import(id: u64) -> Option<PendingImport> {
    IMPORTS.lock().ok()?.remove(&id)
}

pub fn restore_import(id: u64, p: PendingImport) {
    if let Ok(mut m) = IMPORTS.lock() {
        m.insert(id, p);
    }
}

pub fn clear_imports() {
    if let Ok(mut m) = IMPORTS.lock() {
        m.clear();
    }
}
