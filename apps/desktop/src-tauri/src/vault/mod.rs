//! The password vault.
//!
//! Key hierarchy:
//! - A random 256-bit vault key encrypts every password and note (AES-256-GCM, a fresh nonce
//!   each time, authenticated together with the entry's id, site and username).
//! - The vault key is stored only wrapped: once with a key derived from the master password
//!   (Argon2id), and once with a key derived from the recovery key shown at setup. The master
//!   password itself is never stored; a wrong one simply fails to unwrap.
//! - Metadata (sites, usernames, the "never save" list) is encrypted with Windows DPAPI, so it is
//!   unreadable outside this Windows account but available while the vault is locked. That lets
//!   Zenith suggest accounts on a login page before the master password is entered.
//!
//! Unlocking keeps the vault key in memory (wiped on lock). Restarting Zenith always locks.

pub mod cards;
pub mod crypto;
pub mod csv;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::Url;
use zeroize::{Zeroize, Zeroizing};

use crypto::{Key, KdfParams, Sealed};
use csv::ImportRow;

/// Version 2 files hold saved cards. A vault without cards is still written as version 1, so an
/// older Zenith can open it; one with cards is refused by older versions, which would otherwise
/// drop the cards the next time they saved.
const FILE_VERSION: u32 = 2;
const AAD_MASTER: &[u8] = b"zenith-vault:key:master";
const AAD_RECOVERY: &[u8] = b"zenith-vault:key:recovery";
const AAD_BACKUP: &[u8] = b"zenith-passwords-backup:v1";
const BACKUP_FORMAT: &str = "zenith-passwords-backup";

pub const MIN_MASTER_LEN: usize = 8;
const MAX_MASTER_LEN: usize = 1024;
const MAX_USERNAME: usize = 512;
const MAX_PASSWORD: usize = 1024;
const MAX_LABEL: usize = 200;
const MAX_NOTES: usize = 10_000;
/// Failed attempts allowed before each further attempt has to wait (doubling, up to a minute).
const FREE_ATTEMPTS: u32 = 5;

pub const LOCKED: &str = "The password manager is locked.";
/// The UI answers this by asking for the master password and retrying.
pub const NEED_MASTER: &str = "Enter your master password to continue.";
const NOT_SET_UP: &str = "Set up a master password first.";

/// Hosting domains where every subdomain belongs to someone else: credentials are never shared
/// between their subdomains, even when matching "the same site" is on.
const SHARED_SUFFIXES: &[&str] = &[
    "github.io", "gitlab.io", "herokuapp.com", "blogspot.com", "wordpress.com", "vercel.app", "netlify.app",
    "pages.dev", "workers.dev", "web.app", "firebaseapp.com", "appspot.com", "azurewebsites.net", "cloudfront.net",
    "amazonaws.com", "onrender.com", "glitch.me", "repl.co", "ngrok.io", "ngrok-free.app", "fly.dev",
    "surge.sh", "neocities.org", "tumblr.com", "wixsite.com", "weebly.com", "myshopify.com", "substack.com",
    "notion.site", "carrd.co", "000webhostapp.com", "duckdns.org", "no-ip.org",
];

// ---------------------------------------------------------------------------------------------
// On-disk format

/// The vault key wrapped by a key derived from a secret (master password or recovery key).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
struct Wrap {
    kdf: KdfParams,
    #[serde(with = "crypto::b64")]
    salt: Vec<u8>,
    key: Sealed,
}

#[derive(Serialize, Deserialize, Clone)]
struct VaultFile {
    version: u32,
    #[serde(default)]
    master: Option<Wrap>,
    #[serde(default)]
    recovery: Option<Wrap>,
    /// DPAPI-protected JSON of `Index`.
    #[serde(with = "crypto::b64", default)]
    index: Vec<u8>,
    /// Sealed `Secret`s by entry id.
    #[serde(default)]
    secrets: BTreeMap<String, Sealed>,
    /// Sealed card numbers (and security codes) by card id.
    #[serde(default)]
    card_secrets: BTreeMap<String, Sealed>,
}

impl VaultFile {
    fn empty() -> Self {
        Self { version: 1, master: None, recovery: None, index: Vec::new(), secrets: BTreeMap::new(), card_secrets: BTreeMap::new() }
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct Index {
    #[serde(default)]
    entries: Vec<Meta>,
    /// Saved payment cards (see `cards`).
    #[serde(default)]
    cards: Vec<cards::CardMeta>,
    /// Sites the user chose "Never save" for.
    #[serde(default)]
    never: Vec<String>,
    /// Key for password fingerprints (see `crypto::fingerprint`).
    #[serde(with = "crypto::b64", default)]
    check_key: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Meta {
    id: String,
    origin: String,
    username: String,
    #[serde(default)]
    label: String,
    created: i64,
    updated: i64,
    #[serde(default)]
    last_used: i64,
    #[serde(default)]
    times_used: u32,
    #[serde(default)]
    check: u32,
    #[serde(default)]
    has_notes: bool,
}

#[derive(Serialize, Deserialize)]
struct Secret {
    password: String,
    #[serde(default)]
    notes: String,
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.password.zeroize();
        self.notes.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
struct BackupFile {
    format: String,
    version: u32,
    created: i64,
    count: usize,
    kdf: KdfParams,
    #[serde(with = "crypto::b64")]
    salt: Vec<u8>,
    data: Sealed,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupEntry {
    url: String,
    username: String,
    password: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    notes: String,
}

impl Drop for BackupEntry {
    fn drop(&mut self) {
        self.password.zeroize();
        self.notes.zeroize();
    }
}

// ---------------------------------------------------------------------------------------------
// Values for the UI

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// A master password has been set up.
    pub exists: bool,
    pub unlocked: bool,
    pub has_recovery: bool,
    pub count: usize,
    /// Saved payment cards.
    pub card_count: usize,
    /// The file exists but can't be used (damaged, or from another Windows account).
    pub damaged: Option<String>,
    /// Seconds until another master password attempt is accepted.
    pub retry_after: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EntryInfo {
    pub id: String,
    pub origin: String,
    /// Host for display, without "www.".
    pub site: String,
    pub username: String,
    pub label: String,
    pub created: i64,
    pub updated: i64,
    pub last_used: i64,
    pub times_used: u32,
    pub has_notes: bool,
}

/// A saved account offered on a login page.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub username: String,
    pub label: String,
    pub site: String,
    /// Saved for exactly this origin (not just the same site).
    pub exact: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Revealed {
    pub password: String,
    pub notes: String,
}

impl Drop for Revealed {
    fn drop(&mut self) {
        self.password.zeroize();
        self.notes.zeroize();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub url: String,
    pub username: String,
    /// `None` keeps the saved password (when editing).
    pub password: Option<String>,
    #[serde(default)]
    pub label: String,
    /// `None` keeps the saved notes (when editing).
    pub notes: Option<String>,
}

impl Drop for EntryInput {
    fn drop(&mut self) {
        if let Some(p) = &mut self.password {
            p.zeroize();
        }
        if let Some(n) = &mut self.notes {
            n.zeroize();
        }
    }
}

/// What a captured login is, compared with the vault.
#[derive(Debug, PartialEq, Eq)]
pub enum Capture {
    /// Already saved with this password.
    Saved,
    /// Saved with a different password: offer to update this entry.
    Update(String),
    New,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    pub index: usize,
    pub site: String,
    pub origin: String,
    pub username: String,
    pub label: String,
    /// "new" | "duplicate" | "conflict" | "invalid"
    pub status: &'static str,
    pub reason: Option<String>,
    pub existing_id: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub added: usize,
    pub replaced: usize,
    pub skipped: usize,
}

// ---------------------------------------------------------------------------------------------
// Addresses

/// The origin (`https://host[:port]`) a credential belongs to, from a URL or a bare domain.
pub fn normalize_origin(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() || s.len() > 2048 {
        return None;
    }
    let full = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
    let url = Url::parse(&full).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?;
    if host.is_empty() {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

fn host_of_origin(origin: &str) -> String {
    Url::parse(origin).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default()
}

/// Host for display.
pub fn display_site(origin: &str) -> String {
    let host = host_of_origin(origin);
    host.strip_prefix("www.").map(str::to_string).unwrap_or(host)
}

/// The site a "never save" choice applies to.
pub fn never_key(origin: &str) -> String {
    let host = host_of_origin(origin);
    crate::platform::urls::site_of(&host).to_ascii_lowercase()
}

/// Whether a credential saved for `saved` may be used on a page at `page` (both origins).
/// `Some(true)`: the same origin. `Some(false)`: another origin of the same site (only with
/// `same_site`). HTTPS credentials are never offered to plain-HTTP pages, IP addresses and
/// single-label hosts must match exactly, and shared hosting domains never share credentials.
pub fn origin_match(saved: &str, page: &str, same_site: bool) -> Option<bool> {
    if saved == page {
        return Some(true);
    }
    if !same_site {
        return None;
    }
    let (s, p) = (Url::parse(saved).ok()?, Url::parse(page).ok()?);
    if !matches!(p.scheme(), "http" | "https") || (s.scheme() == "https" && p.scheme() != "https") {
        return None;
    }
    let (sh, ph) = (s.host_str()?.to_ascii_lowercase(), p.host_str()?.to_ascii_lowercase());
    let simple = |h: &str| !h.contains('.') || h.parse::<std::net::IpAddr>().is_ok() || h.starts_with('[');
    if simple(&sh) || simple(&ph) {
        return None;
    }
    let (ss, ps) = (crate::platform::urls::site_of(&sh), crate::platform::urls::site_of(&ph));
    if ss != ps {
        return None;
    }
    if SHARED_SUFFIXES.iter().any(|x| *x == ss) {
        return None;
    }
    Some(false)
}

fn entry_aad(id: &str, origin: &str, username: &str) -> Vec<u8> {
    format!("zenith-vault:entry:v1\n{id}\n{origin}\n{username}").into_bytes()
}

fn new_id() -> String {
    crypto::random::<8>().iter().map(|b| format!("{b:02x}")).collect()
}

fn now_ms() -> i64 {
    crate::storage::now_ms()
}

fn check_len(value: &str, max: usize, what: &str) -> Result<(), String> {
    if value.chars().count() > max {
        return Err(format!("The {what} is too long."));
    }
    Ok(())
}

pub fn validate_new_master(master: &str) -> Result<(), String> {
    if master.chars().count() < MIN_MASTER_LEN {
        return Err(format!("Use at least {MIN_MASTER_LEN} characters for the master password."));
    }
    check_len(master, MAX_MASTER_LEN, "master password")
}

fn make_wrap(secret: &[u8], kdf: KdfParams, key: &Key, aad: &[u8]) -> Result<Wrap, String> {
    let salt = crypto::random::<16>().to_vec();
    let kek = crypto::derive(secret, &salt, kdf)?;
    Ok(Wrap { kdf, salt, key: crypto::seal(&kek, &key[..], aad) })
}

fn unwrap_key(secret: &[u8], wrap: &Wrap, aad: &[u8]) -> Result<Option<Key>, String> {
    let kek = crypto::derive(secret, &wrap.salt, wrap.kdf)?;
    Ok(crypto::open_key(&kek, &wrap.key, aad))
}

// ---------------------------------------------------------------------------------------------
// The vault

pub struct Vault {
    path: PathBuf,
    file: VaultFile,
    index: Index,
    key: Option<Key>,
    last_used: Instant,
    failures: u32,
    last_failure: Option<Instant>,
    damaged: Option<String>,
    kdf: KdfParams,
}

impl Vault {
    /// Loads the vault at `path` (an empty one when the file does not exist). A file that can't
    /// be read is reported as damaged and never overwritten.
    pub fn open(path: PathBuf, kdf: KdfParams) -> Self {
        let mut v = Self {
            path,
            file: VaultFile::empty(),
            index: Index::default(),
            key: None,
            last_used: Instant::now(),
            failures: 0,
            last_failure: None,
            damaged: None,
            kdf,
        };
        match std::fs::read(&v.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => v.damaged = Some(format!("The password file could not be read ({e}).")),
            Ok(bytes) => match serde_json::from_slice::<VaultFile>(&bytes) {
                Ok(file) if file.version > FILE_VERSION => {
                    v.damaged = Some("The password file was made by a newer version of Zenith.".into());
                }
                Ok(file) => {
                    let index = if file.index.is_empty() {
                        Ok(Index::default())
                    } else {
                        crypto::unprotect(&file.index)
                            .and_then(|plain| serde_json::from_slice::<Index>(&plain).map_err(|e| e.to_string()))
                    };
                    match index {
                        Ok(index) => {
                            v.file = file;
                            v.index = index;
                        }
                        Err(_) => {
                            v.damaged = Some(
                                "The saved passwords can't be opened by this Windows account. They may have been copied from another PC or user.".into(),
                            )
                        }
                    }
                }
                Err(_) => v.damaged = Some("The password file is damaged.".into()),
            },
        }
        if v.index.check_key.len() != 32 {
            v.index.check_key = crypto::random::<32>().to_vec();
        }
        v
    }

    fn save(&mut self) -> Result<(), String> {
        if let Some(d) = &self.damaged {
            return Err(d.clone());
        }
        let plain = Zeroizing::new(serde_json::to_vec(&self.index).map_err(|e| e.to_string())?);
        self.file.index = crypto::protect(&plain)?;
        self.file.version = if self.index.cards.is_empty() && self.file.card_secrets.is_empty() { 1 } else { FILE_VERSION };
        let json = serde_json::to_vec_pretty(&self.file).map_err(|e| e.to_string())?;
        crate::storage::write_atomic(&self.path, &json).map_err(|e| format!("Could not save passwords: {e}"))
    }

    pub fn exists(&self) -> bool {
        self.file.master.is_some()
    }

    pub fn unlocked(&self) -> bool {
        self.key.is_some()
    }

    pub fn status(&self) -> Status {
        Status {
            exists: self.exists(),
            unlocked: self.unlocked(),
            has_recovery: self.file.recovery.is_some(),
            count: if self.exists() { self.index.entries.len() } else { 0 },
            card_count: if self.exists() { self.index.cards.len() } else { 0 },
            damaged: self.damaged.clone(),
            retry_after: self.retry_after(),
        }
    }

    fn retry_after(&self) -> u64 {
        if self.failures < FREE_ATTEMPTS {
            return 0;
        }
        let wait = (1u64 << (self.failures - FREE_ATTEMPTS).min(6)).min(60);
        self.last_failure.map(|t| wait.saturating_sub(t.elapsed().as_secs())).unwrap_or(0)
    }

    fn check_retry(&self) -> Result<(), String> {
        match self.retry_after() {
            0 => Ok(()),
            n => Err(format!("Too many wrong attempts. Try again in {n} second{}.", if n == 1 { "" } else { "s" })),
        }
    }

    fn failed(&mut self, msg: &str) -> String {
        self.failures += 1;
        self.last_failure = Some(Instant::now());
        msg.to_string()
    }

    fn key(&self) -> Result<&Key, String> {
        if let Some(d) = &self.damaged {
            return Err(d.clone());
        }
        self.key.as_ref().ok_or_else(|| LOCKED.to_string())
    }

    pub fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    pub fn lock(&mut self) {
        self.key = None;
    }

    /// Locks when unlocked and unused for `minutes` (0: never). Returns whether it locked.
    pub fn lock_if_idle(&mut self, minutes: u32) -> bool {
        if minutes > 0 && self.unlocked() && self.last_used.elapsed() >= Duration::from_secs(minutes as u64 * 60) {
            self.lock();
            return true;
        }
        false
    }

    // -- master password ----------------------------------------------------------------------

    /// Creates the vault key and protects it with the master password. Returns the recovery key,
    /// which is shown once and never stored.
    pub fn setup(&mut self, master: &str) -> Result<Zeroizing<String>, String> {
        if let Some(d) = &self.damaged {
            return Err(d.clone());
        }
        if self.exists() {
            return Err("A master password is already set up.".into());
        }
        validate_new_master(master)?;
        let key = crypto::random_key();
        let recovery = crypto::new_recovery_key();
        self.file.master = Some(make_wrap(master.as_bytes(), self.kdf, &key, AAD_MASTER)?);
        self.file.recovery = Some(make_wrap(crypto::normalize_recovery_key(&recovery).as_bytes(), self.kdf, &key, AAD_RECOVERY)?);
        // Entries can only exist with a key; drop leftovers of an interrupted reset.
        self.index.entries.clear();
        self.file.secrets.clear();
        self.index.cards.clear();
        self.file.card_secrets.clear();
        if let Err(e) = self.save() {
            self.file.master = None;
            self.file.recovery = None;
            return Err(e);
        }
        self.key = Some(key);
        self.failures = 0;
        self.touch();
        Ok(recovery)
    }

    /// The vault key for a master password, counting wrong attempts.
    fn key_for_master(&mut self, master: &str) -> Result<Key, String> {
        if let Some(d) = &self.damaged {
            return Err(d.clone());
        }
        let wrap = self.file.master.clone().ok_or(NOT_SET_UP)?;
        self.check_retry()?;
        match unwrap_key(master.as_bytes(), &wrap, AAD_MASTER)? {
            Some(key) => {
                self.failures = 0;
                Ok(key)
            }
            None => Err(self.failed("That master password isn't right.")),
        }
    }

    pub fn unlock(&mut self, master: &str) -> Result<(), String> {
        let key = self.key_for_master(master)?;
        self.key = Some(key);
        self.touch();
        Ok(())
    }

    /// Confirms the master password again (export, or "ask every time"). Also unlocks.
    pub fn verify(&mut self, master: &str) -> Result<(), String> {
        self.unlock(master)
    }

    pub fn change_master(&mut self, current: &str, new: &str) -> Result<(), String> {
        validate_new_master(new)?;
        let key = self.key_for_master(current)?;
        let old = self.file.master.clone();
        self.file.master = Some(make_wrap(new.as_bytes(), self.kdf, &key, AAD_MASTER)?);
        if let Err(e) = self.save() {
            self.file.master = old;
            return Err(e);
        }
        self.key = Some(key);
        self.touch();
        Ok(())
    }

    /// "Forgot master password": the recovery key sets a new master password without losing
    /// anything. A new recovery key replaces the used one and is returned.
    pub fn recover(&mut self, recovery_key: &str, new_master: &str) -> Result<Zeroizing<String>, String> {
        if let Some(d) = &self.damaged {
            return Err(d.clone());
        }
        validate_new_master(new_master)?;
        let wrap = self.file.recovery.clone().ok_or("This vault has no recovery key.")?;
        self.check_retry()?;
        let normalized = crypto::normalize_recovery_key(recovery_key);
        let Some(key) = unwrap_key(normalized.as_bytes(), &wrap, AAD_RECOVERY)? else {
            return Err(self.failed("That recovery key isn't right."));
        };
        self.failures = 0;
        let fresh = crypto::new_recovery_key();
        let (old_master, old_recovery) = (self.file.master.clone(), self.file.recovery.clone());
        self.file.master = Some(make_wrap(new_master.as_bytes(), self.kdf, &key, AAD_MASTER)?);
        self.file.recovery = Some(make_wrap(crypto::normalize_recovery_key(&fresh).as_bytes(), self.kdf, &key, AAD_RECOVERY)?);
        if let Err(e) = self.save() {
            self.file.master = old_master;
            self.file.recovery = old_recovery;
            return Err(e);
        }
        self.key = Some(key);
        self.touch();
        Ok(fresh)
    }

    /// Replaces the recovery key (the old one stops working).
    pub fn new_recovery_key(&mut self, master: &str) -> Result<Zeroizing<String>, String> {
        let key = self.key_for_master(master)?;
        let fresh = crypto::new_recovery_key();
        let old = self.file.recovery.clone();
        self.file.recovery = Some(make_wrap(crypto::normalize_recovery_key(&fresh).as_bytes(), self.kdf, &key, AAD_RECOVERY)?);
        if let Err(e) = self.save() {
            self.file.recovery = old;
            return Err(e);
        }
        self.key = Some(key);
        self.touch();
        Ok(fresh)
    }

    /// Deletes every saved password and the master password (only on the user's explicit,
    /// confirmed request). A damaged file is renamed instead, in case it can be recovered.
    pub fn reset(&mut self) -> Result<(), String> {
        if self.path.exists() {
            if self.damaged.is_some() {
                let aside = self.path.with_file_name(format!("vault-damaged-{}.json", now_ms()));
                std::fs::rename(&self.path, aside).map_err(|e| e.to_string())?;
            } else {
                std::fs::remove_file(&self.path).map_err(|e| e.to_string())?;
            }
        }
        *self = Vault::open(self.path.clone(), self.kdf);
        Ok(())
    }

    // -- entries ------------------------------------------------------------------------------

    fn info(m: &Meta) -> EntryInfo {
        EntryInfo {
            id: m.id.clone(),
            origin: m.origin.clone(),
            site: display_site(&m.origin),
            username: m.username.clone(),
            label: m.label.clone(),
            created: m.created,
            updated: m.updated,
            last_used: m.last_used,
            times_used: m.times_used,
            has_notes: m.has_notes,
        }
    }

    pub fn list(&mut self) -> Result<Vec<EntryInfo>, String> {
        self.key()?;
        self.touch();
        let mut out: Vec<EntryInfo> = self.index.entries.iter().map(Self::info).collect();
        out.sort_by(|a, b| {
            (a.site.to_lowercase(), a.username.to_lowercase()).cmp(&(b.site.to_lowercase(), b.username.to_lowercase()))
        });
        Ok(out)
    }

    fn secret(&self, m: &Meta) -> Result<Secret, String> {
        let key = self.key()?;
        let sealed = self.file.secrets.get(&m.id).ok_or("This password is missing from the vault.")?;
        let plain = crypto::open(key, sealed, &entry_aad(&m.id, &m.origin, &m.username))
            .ok_or("This password could not be decrypted: the vault may be damaged.")?;
        serde_json::from_slice::<Secret>(&plain).map_err(|e| e.to_string())
    }

    fn meta(&self, id: &str) -> Result<&Meta, String> {
        self.index.entries.iter().find(|m| m.id == id).ok_or_else(|| "That password no longer exists.".to_string())
    }

    pub fn reveal(&mut self, id: &str) -> Result<Revealed, String> {
        let m = self.meta(id)?.clone();
        let s = self.secret(&m)?;
        self.touch();
        Ok(Revealed { password: s.password.clone(), notes: s.notes.clone() })
    }

    /// Adds an entry without saving the file.
    fn insert(&mut self, origin: &str, username: &str, password: &str, label: &str, notes: &str) -> Result<String, String> {
        let key = self.key()?.clone();
        check_len(username, MAX_USERNAME, "username")?;
        check_len(password, MAX_PASSWORD, "password")?;
        check_len(label, MAX_LABEL, "name")?;
        check_len(notes, MAX_NOTES, "note")?;
        let id = new_id();
        let secret = Secret { password: password.to_string(), notes: notes.to_string() };
        let plain = Zeroizing::new(serde_json::to_vec(&secret).map_err(|e| e.to_string())?);
        self.file.secrets.insert(id.clone(), crypto::seal(&key, &plain, &entry_aad(&id, origin, username)));
        let now = now_ms();
        self.index.entries.push(Meta {
            id: id.clone(),
            origin: origin.to_string(),
            username: username.to_string(),
            label: label.trim().to_string(),
            created: now,
            updated: now,
            last_used: 0,
            times_used: 0,
            check: crypto::fingerprint(&self.index.check_key, password),
            has_notes: !notes.trim().is_empty(),
        });
        Ok(id)
    }

    pub fn add(&mut self, input: &EntryInput) -> Result<String, String> {
        self.key()?;
        let origin = normalize_origin(&input.url).ok_or("Enter a website address, like example.com.")?;
        let password = input.password.as_deref().unwrap_or("");
        if password.is_empty() {
            return Err("Enter a password.".into());
        }
        let id = self.insert(&origin, input.username.trim(), password, &input.label, input.notes.as_deref().unwrap_or(""))?;
        self.save()?;
        self.touch();
        Ok(id)
    }

    /// Rewrites one entry (re-sealed, since its site and username are part of the context).
    fn rewrite(&mut self, id: &str, origin: &str, username: &str, password: &str, label: &str, notes: &str) -> Result<(), String> {
        let key = self.key()?.clone();
        check_len(username, MAX_USERNAME, "username")?;
        check_len(password, MAX_PASSWORD, "password")?;
        check_len(label, MAX_LABEL, "name")?;
        check_len(notes, MAX_NOTES, "note")?;
        let check = crypto::fingerprint(&self.index.check_key, password);
        let secret = Secret { password: password.to_string(), notes: notes.to_string() };
        let plain = Zeroizing::new(serde_json::to_vec(&secret).map_err(|e| e.to_string())?);
        let sealed = crypto::seal(&key, &plain, &entry_aad(id, origin, username));
        let m = self.index.entries.iter_mut().find(|m| m.id == id).ok_or("That password no longer exists.")?;
        m.origin = origin.to_string();
        m.username = username.to_string();
        m.label = label.trim().to_string();
        m.updated = now_ms();
        m.check = check;
        m.has_notes = !notes.trim().is_empty();
        self.file.secrets.insert(id.to_string(), sealed);
        Ok(())
    }

    pub fn update(&mut self, id: &str, input: &EntryInput) -> Result<(), String> {
        let m = self.meta(id)?.clone();
        let old = self.secret(&m)?;
        let origin = normalize_origin(&input.url).ok_or("Enter a website address, like example.com.")?;
        let password = input.password.as_deref().unwrap_or(&old.password);
        if password.is_empty() {
            return Err("Enter a password.".into());
        }
        let notes = input.notes.as_deref().unwrap_or(&old.notes);
        self.rewrite(id, &origin, input.username.trim(), password, &input.label, notes)?;
        self.save()?;
        self.touch();
        Ok(())
    }

    pub fn delete(&mut self, ids: &[String]) -> Result<usize, String> {
        self.key()?;
        let before = self.index.entries.len();
        self.index.entries.retain(|m| !ids.contains(&m.id));
        for id in ids {
            self.file.secrets.remove(id);
        }
        self.save()?;
        self.touch();
        Ok(before - self.index.entries.len())
    }

    // -- autofill -----------------------------------------------------------------------------

    /// Saved accounts usable on a page (works while locked: metadata only).
    pub fn accounts_for(&self, page_origin: &str, same_site: bool) -> Vec<Account> {
        if !self.exists() || self.damaged.is_some() {
            return Vec::new();
        }
        let mut found: Vec<(&Meta, bool)> = self
            .index
            .entries
            .iter()
            .filter_map(|m| origin_match(&m.origin, page_origin, same_site).map(|exact| (m, exact)))
            .collect();
        found.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.last_used.cmp(&a.0.last_used)).then(a.0.username.cmp(&b.0.username)));
        found
            .into_iter()
            .map(|(m, exact)| Account {
                id: m.id.clone(),
                username: m.username.clone(),
                label: m.label.clone(),
                site: display_site(&m.origin),
                exact,
            })
            .collect()
    }

    /// The username and password to fill into a page, after checking the page may get them.
    pub fn credentials_for_fill(&mut self, id: &str, page_origin: &str, same_site: bool) -> Result<(String, Zeroizing<String>), String> {
        let m = self.meta(id)?.clone();
        if origin_match(&m.origin, page_origin, same_site).is_none() {
            return Err(format!("This password was saved for {}, not for this site.", display_site(&m.origin)));
        }
        let secret = self.secret(&m)?;
        let password = Zeroizing::new(secret.password.clone());
        if let Some(e) = self.index.entries.iter_mut().find(|e| e.id == id) {
            e.last_used = now_ms();
            e.times_used = e.times_used.saturating_add(1);
        }
        let _ = self.save();
        self.touch();
        Ok((m.username, password))
    }

    /// Compares a login the user just submitted with the vault (works while locked).
    pub fn classify(&self, origin: &str, username: &str, password: &str) -> Capture {
        let fp = crypto::fingerprint(&self.index.check_key, password);
        let same_user = |m: &&Meta| m.username.eq_ignore_ascii_case(username);
        let exact = self.index.entries.iter().filter(|m| m.origin == origin).find(same_user);
        let near = || {
            self.index
                .entries
                .iter()
                .filter(|m| origin_match(&m.origin, origin, true).is_some())
                .find(same_user)
        };
        match exact.or_else(near) {
            Some(m) if m.check == fp => Capture::Saved,
            Some(m) => Capture::Update(m.id.clone()),
            None => Capture::New,
        }
    }

    /// Saves a login offered on a page: a new entry, or a new password for `update`.
    pub fn save_login(&mut self, origin: &str, username: &str, password: &str, update: Option<&str>) -> Result<String, String> {
        self.key()?;
        let id = match update {
            Some(id) => {
                let m = self.meta(id)?.clone();
                let old = self.secret(&m)?;
                let notes = old.notes.clone();
                self.rewrite(id, &m.origin, &m.username, password, &m.label, &notes)?;
                id.to_string()
            }
            None => self.insert(origin, username, password, "", "")?,
        };
        self.save()?;
        self.touch();
        Ok(id)
    }

    pub fn is_never(&self, origin: &str) -> bool {
        let key = never_key(origin);
        self.index.never.iter().any(|s| *s == key)
    }

    pub fn never_list(&self) -> Vec<String> {
        let mut v = self.index.never.clone();
        v.sort();
        v
    }

    pub fn set_never(&mut self, site: &str, never: bool) -> Result<(), String> {
        let site = site.trim().to_ascii_lowercase();
        if site.is_empty() {
            return Ok(());
        }
        self.index.never.retain(|s| *s != site);
        if never {
            self.index.never.push(site);
        }
        self.save()
    }

    // -- import and export --------------------------------------------------------------------

    fn all_plain(&self) -> Result<Vec<(Meta, Secret)>, String> {
        self.key()?;
        let mut out = Vec::new();
        for m in &self.index.entries {
            out.push((m.clone(), self.secret(m)?));
        }
        out.sort_by(|a, b| (display_site(&a.0.origin), a.0.username.clone()).cmp(&(display_site(&b.0.origin), b.0.username.clone())));
        Ok(out)
    }

    /// Every credential in plain text, in Chrome's CSV format.
    pub fn export_csv(&mut self) -> Result<Zeroizing<String>, String> {
        let rows: Vec<_> = self
            .all_plain()?
            .into_iter()
            .map(|(m, s)| {
                let name = if m.label.is_empty() { display_site(&m.origin) } else { m.label.clone() };
                (name, m.origin.clone(), m.username.clone(), Zeroizing::new(s.password.clone()), Zeroizing::new(s.notes.clone()))
            })
            .collect();
        self.touch();
        Ok(csv::write_export(&rows))
    }

    /// An encrypted backup file, protected by `password` (the master password).
    pub fn backup(&mut self, password: &str) -> Result<Vec<u8>, String> {
        let entries: Vec<BackupEntry> = self
            .all_plain()?
            .into_iter()
            .map(|(m, s)| BackupEntry {
                url: m.origin.clone(),
                username: m.username.clone(),
                password: s.password.clone(),
                label: m.label.clone(),
                notes: s.notes.clone(),
            })
            .collect();
        let plain = Zeroizing::new(serde_json::to_vec(&entries).map_err(|e| e.to_string())?);
        let salt = crypto::random::<16>().to_vec();
        let kek = crypto::derive(password.as_bytes(), &salt, self.kdf)?;
        let file = BackupFile {
            format: BACKUP_FORMAT.into(),
            version: 1,
            created: now_ms(),
            count: entries.len(),
            kdf: self.kdf,
            salt,
            data: crypto::seal(&kek, &plain, AAD_BACKUP),
        };
        self.touch();
        serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())
    }

    fn row_status(&self, row: &ImportRow, origin: &Option<String>) -> Result<(&'static str, Option<String>, Option<String>), String> {
        let Some(origin) = origin else {
            let reason = if row.url.trim().is_empty() { "No website address" } else { "Not a website address" };
            return Ok(("invalid", Some(reason.into()), None));
        };
        if row.password.is_empty() {
            return Ok(("invalid", Some("No password".into()), None));
        }
        if row.username.chars().count() > MAX_USERNAME || row.password.chars().count() > MAX_PASSWORD {
            return Ok(("invalid", Some("Too long".into()), None));
        }
        let existing = self.index.entries.iter().find(|m| m.origin == *origin && m.username == row.username);
        Ok(match existing {
            Some(m) => {
                let same = self.secret(m)?.password == *row.password;
                if same {
                    ("duplicate", Some("Already saved".into()), Some(m.id.clone()))
                } else {
                    ("conflict", Some("Saved with a different password".into()), Some(m.id.clone()))
                }
            }
            None => ("new", None, None),
        })
    }

    /// How each imported row compares with the vault, before anything is saved.
    pub fn preview(&mut self, rows: &[ImportRow]) -> Result<Vec<PreviewRow>, String> {
        self.key()?;
        let mut out = Vec::with_capacity(rows.len());
        let mut seen: HashMap<(String, String), String> = HashMap::new();
        for (index, row) in rows.iter().enumerate() {
            let origin = normalize_origin(&row.url);
            let (mut status, mut reason, existing_id) = self.row_status(row, &origin)?;
            if let (Some(o), "new") = (&origin, status) {
                let k = (o.clone(), row.username.clone());
                match seen.get(&k) {
                    Some(p) if *p == *row.password => {
                        status = "duplicate";
                        reason = Some("Repeated in this file".into());
                    }
                    _ => {
                        seen.insert(k, row.password.to_string());
                    }
                }
            }
            out.push(PreviewRow {
                index,
                site: origin.as_deref().map(display_site).unwrap_or_else(|| row.url.clone()),
                origin: origin.clone().unwrap_or_default(),
                username: row.username.clone(),
                label: row.label.clone(),
                status,
                reason,
                existing_id,
            });
        }
        for v in seen.values_mut() {
            v.zeroize();
        }
        self.touch();
        Ok(out)
    }

    /// Imports rows. `choices` by row index: new rows are added unless "skip"; rows saved with a
    /// different password are kept ("skip", the default), "replace"d, or added as well ("both").
    pub fn import(&mut self, rows: &[ImportRow], choices: &HashMap<usize, String>) -> Result<ImportSummary, String> {
        let preview = self.preview(rows)?;
        let mut sum = ImportSummary::default();
        for p in &preview {
            let row = &rows[p.index];
            let choice = choices.get(&p.index).map(String::as_str);
            match (p.status, choice) {
                ("new", Some("skip")) | ("invalid", _) | ("duplicate", _) | ("conflict", None | Some("skip")) => sum.skipped += 1,
                ("new", _) | ("conflict", Some("both")) => {
                    self.insert(&p.origin, &row.username, &row.password, &row.label, &row.notes)?;
                    sum.added += 1;
                }
                ("conflict", Some("replace")) => {
                    let id = p.existing_id.clone().unwrap_or_default();
                    let m = self.meta(&id)?.clone();
                    let old = self.secret(&m)?;
                    let notes = if row.notes.trim().is_empty() { old.notes.clone() } else { row.notes.to_string() };
                    let label = if row.label.trim().is_empty() { m.label.clone() } else { row.label.clone() };
                    self.rewrite(&id, &m.origin, &m.username, &row.password, &label, &notes)?;
                    sum.replaced += 1;
                }
                _ => sum.skipped += 1,
            }
        }
        if sum.added + sum.replaced > 0 {
            self.save()?;
        }
        Ok(sum)
    }
}

/// Reads a Zenith backup file with the password it was made with.
pub fn read_backup(bytes: &[u8], password: &str) -> Result<Vec<ImportRow>, String> {
    let file: BackupFile = serde_json::from_slice(bytes).map_err(|_| "This isn't a Zenith password backup.".to_string())?;
    if file.format != BACKUP_FORMAT {
        return Err("This isn't a Zenith password backup.".into());
    }
    if file.version > 1 {
        return Err("This backup was made by a newer version of Zenith.".into());
    }
    let kek = crypto::derive(password.as_bytes(), &file.salt, file.kdf)?;
    let plain = crypto::open(&kek, &file.data, AAD_BACKUP)
        .ok_or("That password doesn't open this backup. Use the master password you had when you made it.")?;
    let entries: Vec<BackupEntry> = serde_json::from_slice(&plain).map_err(|e| e.to_string())?;
    Ok(entries
        .iter()
        .map(|e| ImportRow {
            url: e.url.clone(),
            username: e.username.clone(),
            password: Zeroizing::new(e.password.clone()),
            label: e.label.clone(),
            notes: Zeroizing::new(e.notes.clone()),
        })
        .collect())
}

/// Whether bytes look like a Zenith backup (rather than CSV).
pub fn is_backup(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|v| v.get("format").and_then(|f| f.as_str()).map(|f| f == BACKUP_FORMAT))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------------------------
// The process-wide vault

static VAULT: LazyLock<Mutex<Option<Vault>>> = LazyLock::new(|| Mutex::new(None));

fn vault_path() -> PathBuf {
    crate::host::persistent_paths().roaming.join("vault.json")
}

/// Runs `f` with the vault, waiting for the lock. Master-password checks hold it for about half a
/// second, so call this off the UI thread.
pub fn with<R>(f: impl FnOnce(&mut Vault) -> R) -> R {
    let mut guard = VAULT.lock().unwrap_or_else(|e| e.into_inner());
    let v = guard.get_or_insert_with(|| Vault::open(vault_path(), KdfParams::DEFAULT));
    f(v)
}

/// Like `with`, without waiting: `None` while another thread is using the vault. For the UI
/// thread, which must never block on a key derivation.
pub fn try_with<R>(f: impl FnOnce(&mut Vault) -> R) -> Option<R> {
    let mut guard = match VAULT.try_lock() {
        Ok(g) => g,
        Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return None,
    };
    let v = guard.get_or_insert_with(|| Vault::open(vault_path(), KdfParams::DEFAULT));
    Some(f(v))
}

pub fn status() -> Status {
    with(|v| v.status())
}

/// Tells every window the vault's state (after unlocking, locking, setup or reset).
pub fn broadcast() {
    crate::host::emit("vault-status", status());
}

/// Locks and forgets anything decrypted that is waiting in memory.
pub fn lock() {
    with(|v| v.lock());
    clear_imports();
}

// -- save offers ------------------------------------------------------------------------------

/// A login captured on a page, waiting for the user to save it.
pub struct Offer {
    pub tab: String,
    pub origin: String,
    pub username: String,
    pub password: Zeroizing<String>,
    pub update: Option<String>,
    at: Instant,
}

static OFFERS: LazyLock<Mutex<HashMap<u64, Offer>>> = LazyLock::new(Default::default);
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
const OFFER_TTL: Duration = Duration::from_secs(15 * 60);

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

// -- auto-lock ----------------------------------------------------------------------------------

/// Locks the vault after the configured inactivity and drops stale offers, every 10 seconds.
pub fn start_timer() {
    std::thread::Builder::new()
        .name("zenith-vault".into())
        .spawn(|| loop {
            std::thread::sleep(Duration::from_secs(10));
            if let Ok(mut o) = OFFERS.lock() {
                o.retain(|_, x| x.at.elapsed() < OFFER_TTL);
            }
            cards::prune_card_offers();
            let minutes = crate::host::config().vault_lock_minutes;
            if try_with(|v| v.lock_if_idle(minutes)).unwrap_or(false) {
                clear_imports();
                broadcast();
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault(name: &str) -> Vault {
        let dir = std::env::temp_dir().join(format!("zenith-vault-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Vault::open(dir.join("vault.json"), KdfParams::FAST)
    }

    fn input(url: &str, user: &str, pw: &str) -> EntryInput {
        EntryInput { url: url.into(), username: user.into(), password: Some(pw.into()), label: String::new(), notes: None }
    }

    #[test]
    fn origins_normalize_and_match() {
        assert_eq!(normalize_origin("Example.com/login").as_deref(), Some("https://example.com"));
        assert_eq!(normalize_origin("http://a.b.org:8080/x").as_deref(), Some("http://a.b.org:8080"));
        assert_eq!(normalize_origin("https://x.org:443/").as_deref(), Some("https://x.org"));
        assert_eq!(normalize_origin("javascript:alert(1)"), None);
        assert_eq!(normalize_origin("ftp://x.org"), None);
        assert_eq!(normalize_origin(""), None);

        let g = "https://accounts.google.com";
        assert_eq!(origin_match(g, g, false), Some(true));
        assert_eq!(origin_match(g, "https://mail.google.com", false), None);
        assert_eq!(origin_match(g, "https://mail.google.com", true), Some(false));
        assert_eq!(origin_match(g, "http://accounts.google.com", true), None, "no HTTPS to HTTP");
        assert_eq!(origin_match("http://a.example.com", "https://b.example.com", true), Some(false));
        assert_eq!(origin_match(g, "https://google.com.evil.net", true), None);
        assert_eq!(origin_match("https://alice.github.io", "https://mallory.github.io", true), None);
        assert_eq!(origin_match("http://192.168.1.1", "http://192.168.1.2", true), None);
    }

    #[test]
    fn setup_lock_unlock_and_wrong_password() {
        let mut v = temp_vault("unlock");
        assert!(!v.exists());
        assert!(v.setup("short").is_err());
        let rk = v.setup("correct horse").unwrap();
        assert!(!rk.is_empty());
        assert!(v.unlocked());
        let id = v.add(&input("example.com", "ann", "pw-1")).unwrap();
        v.lock();
        assert!(v.reveal(&id).is_err(), "locked vaults reveal nothing");
        assert!(v.list().is_err());
        assert!(v.unlock("wrong password").is_err());
        v.unlock("correct horse").unwrap();
        assert_eq!(v.reveal(&id).unwrap().password, "pw-1");

        // A fresh load from disk sees the same data and starts locked.
        let mut again = Vault::open(v.path.clone(), KdfParams::FAST);
        assert!(again.exists() && !again.unlocked());
        assert_eq!(again.accounts_for("https://example.com", false).len(), 1, "usernames work while locked");
        again.unlock("correct horse").unwrap();
        assert_eq!(again.reveal(&id).unwrap().password, "pw-1");
    }

    #[test]
    fn file_never_contains_secrets_in_plain_text() {
        let mut v = temp_vault("plain");
        v.setup("master password 1").unwrap();
        let mut i = input("bank.example", "someone@example.com", "SuperSecret!42");
        i.notes = Some("pin 1234".into());
        v.add(&i).unwrap();
        let raw = String::from_utf8_lossy(&std::fs::read(&v.path).unwrap()).to_string();
        for needle in ["SuperSecret!42", "master password 1", "someone@example.com", "bank.example", "pin 1234"] {
            assert!(!raw.contains(needle), "{needle} leaked into the file");
        }
    }

    #[test]
    fn backoff_after_repeated_failures() {
        let mut v = temp_vault("backoff");
        v.setup("the master pw").unwrap();
        v.lock();
        for _ in 0..FREE_ATTEMPTS {
            assert!(v.unlock("nope nope").unwrap_err().contains("isn't right"));
        }
        let err = v.unlock("the master pw").unwrap_err();
        assert!(err.contains("Try again"), "{err}");
        assert!(v.status().retry_after > 0);
    }

    #[test]
    fn recovery_key_resets_master_without_losing_data() {
        let mut v = temp_vault("recover");
        let rk = v.setup("first master").unwrap();
        let id = v.add(&input("example.com", "ann", "keep-me")).unwrap();
        v.lock();
        assert!(v.recover("0000-0000", "second master").is_err());
        let fresh = v.recover(&rk.to_lowercase(), "second master").unwrap();
        assert_ne!(*fresh, *rk);
        assert_eq!(v.reveal(&id).unwrap().password, "keep-me");
        v.lock();
        assert!(v.unlock("first master").is_err());
        v.unlock("second master").unwrap();
        assert!(v.recover(&rk, "third master").is_err(), "a used recovery key stops working");
        v.recover(&fresh, "third master").unwrap();
    }

    #[test]
    fn change_master_and_reset() {
        let mut v = temp_vault("change");
        v.setup("old master pw").unwrap();
        v.add(&input("example.com", "ann", "x")).unwrap();
        assert!(v.change_master("bad", "new master pw").is_err());
        v.change_master("old master pw", "new master pw").unwrap();
        v.lock();
        v.unlock("new master pw").unwrap();
        v.reset().unwrap();
        assert!(!v.exists() && !v.unlocked());
        assert!(!v.path.exists());
        v.setup("another master").unwrap();
        assert_eq!(v.list().unwrap().len(), 0);
    }

    #[test]
    fn edit_reseals_and_keeps_password() {
        let mut v = temp_vault("edit");
        v.setup("master master").unwrap();
        let id = v.add(&input("example.com", "ann", "pw")).unwrap();
        let edit = EntryInput { url: "https://login.example.com".into(), username: "ann2".into(), password: None, label: "Work".into(), notes: Some("n".into()) };
        v.update(&id, &edit).unwrap();
        let r = v.reveal(&id).unwrap();
        assert_eq!(r.password, "pw");
        assert_eq!(r.notes, "n");
        let list = v.list().unwrap();
        assert_eq!(list[0].origin, "https://login.example.com");
        assert_eq!(list[0].label, "Work");
        assert!(list[0].has_notes);
        assert_eq!(v.delete(&[id.clone()]).unwrap(), 1);
        assert!(v.reveal(&id).is_err());
    }

    #[test]
    fn capture_classification_and_fill_rules() {
        let mut v = temp_vault("capture");
        v.setup("master master").unwrap();
        let id = v.add(&input("https://www.facebook.com", "ann@example.com", "pw1")).unwrap();
        v.add(&input("https://www.facebook.com", "bob", "pw2")).unwrap();
        v.lock();
        // Works while locked.
        assert_eq!(v.classify("https://www.facebook.com", "ANN@example.com", "pw1"), Capture::Saved);
        assert_eq!(v.classify("https://www.facebook.com", "ann@example.com", "pw9"), Capture::Update(id.clone()));
        assert_eq!(v.classify("https://m.facebook.com", "ann@example.com", "pw9"), Capture::Update(id.clone()));
        assert_eq!(v.classify("https://www.facebook.com", "carol", "pw1"), Capture::New);
        assert_eq!(v.accounts_for("https://www.facebook.com", false).len(), 2);
        assert_eq!(v.accounts_for("https://m.facebook.com", false).len(), 0);
        assert_eq!(v.accounts_for("https://m.facebook.com", true).len(), 2);
        assert_eq!(v.accounts_for("https://facebook.com.evil.io", true).len(), 0);

        assert_eq!(v.credentials_for_fill(&id, "https://www.facebook.com", false).unwrap_err(), LOCKED);
        v.unlock("master master").unwrap();
        assert!(v.credentials_for_fill(&id, "https://evil.example", true).is_err(), "never fills on another site");
        let (user, pw) = v.credentials_for_fill(&id, "https://www.facebook.com", false).unwrap();
        assert_eq!((user.as_str(), pw.as_str()), ("ann@example.com", "pw1"));
        assert_eq!(v.accounts_for("https://www.facebook.com", false)[0].id, id, "most recently used first");

        let new_id = v.save_login("https://www.facebook.com", "ann@example.com", "pw9", Some(&id)).unwrap();
        assert_eq!(new_id, id);
        assert_eq!(v.classify("https://www.facebook.com", "ann@example.com", "pw9"), Capture::Saved);

        v.set_never("example.org", true).unwrap();
        assert!(v.is_never("https://login.example.org"));
        v.set_never("example.org", false).unwrap();
        assert!(!v.is_never("https://login.example.org"));
    }

    #[test]
    fn import_detects_duplicates_and_conflicts() {
        let mut v = temp_vault("import");
        v.setup("master master").unwrap();
        let existing = v.add(&input("https://a.com", "ann", "same")).unwrap();
        v.add(&input("https://b.com", "bob", "old")).unwrap();
        let rows = csv::read_export(
            "name,url,username,password,note\n\
             A,https://a.com/login,ann,same,\n\
             B,https://b.com,bob,new,\n\
             C,https://c.com,cy,pw,\n\
             C2,https://c.com/x,cy,pw,\n\
             D,,dan,pw,\n\
             E,https://e.com,eve,,\n",
        )
        .unwrap();
        let p = v.preview(&rows).unwrap();
        let st: Vec<&str> = p.iter().map(|r| r.status).collect();
        assert_eq!(st, ["duplicate", "conflict", "new", "duplicate", "invalid", "invalid"]);
        assert_eq!(p[0].existing_id.as_deref(), Some(existing.as_str()));

        let mut choices = HashMap::new();
        choices.insert(1usize, "replace".to_string());
        let sum = v.import(&rows, &choices).unwrap();
        assert_eq!(sum, ImportSummary { added: 1, replaced: 1, skipped: 4 });
        let b = v.list().unwrap().into_iter().find(|e| e.site == "b.com").unwrap();
        assert_eq!(v.reveal(&b.id).unwrap().password, "new");
        assert_eq!(v.list().unwrap().len(), 3);
    }

    #[test]
    fn backup_and_csv_round_trip() {
        let mut v = temp_vault("backup");
        v.setup("master master").unwrap();
        let mut i = input("https://a.com", "ann", "p\"w,1");
        i.notes = Some("note".into());
        i.label = "Mine".into();
        v.add(&i).unwrap();
        let bytes = v.backup("master master").unwrap();
        assert!(is_backup(&bytes));
        assert!(!String::from_utf8_lossy(&bytes).contains("p\"w,1"));
        assert!(read_backup(&bytes, "wrong").is_err());
        let rows = read_backup(&bytes, "master master").unwrap();
        assert_eq!(*rows[0].password, "p\"w,1");
        assert_eq!(rows[0].label, "Mine");

        let text = v.export_csv().unwrap();
        let back = csv::read_export(&text).unwrap();
        assert_eq!(*back[0].password, "p\"w,1");
        assert_eq!(*back[0].notes, "note");
        assert!(!is_backup(text.as_bytes()));

        // Restoring into a new vault adds everything once.
        let mut other = temp_vault("restore");
        other.setup("other master").unwrap();
        assert_eq!(other.import(&rows, &HashMap::new()).unwrap().added, 1);
        assert_eq!(other.import(&rows, &HashMap::new()).unwrap().added, 0);
    }

    #[test]
    fn damaged_files_are_never_overwritten() {
        let v = temp_vault("damaged");
        std::fs::write(&v.path, b"{not json").unwrap();
        let mut v = Vault::open(v.path.clone(), KdfParams::FAST);
        assert!(v.status().damaged.is_some());
        assert!(v.setup("master master").is_err());
        assert!(v.set_never("x.com", true).is_err());
        assert_eq!(std::fs::read(&v.path).unwrap(), b"{not json");
        v.reset().unwrap();
        assert!(v.status().damaged.is_none());
        let kept = std::fs::read_dir(v.path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with("vault-damaged-"));
        assert!(kept, "the damaged file is set aside, not deleted");
    }
}
