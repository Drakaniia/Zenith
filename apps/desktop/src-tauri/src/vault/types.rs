//! The vault's data types: the on-disk format (wrapped keys, the file, its DPAPI-protected index
//! and the sealed secrets, the backup file) and the values handed to the UI.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use super::*;

// ---------------------------------------------------------------------------------------------
// On-disk format

/// The vault key wrapped by a key derived from a secret (master password or recovery key).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(super) struct Wrap {
    pub(super) kdf: KdfParams,
    #[serde(with = "crypto::b64")]
    pub(super) salt: Vec<u8>,
    pub(super) key: Sealed,
}

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct VaultFile {
    pub(super) version: u32,
    #[serde(default)]
    pub(super) master: Option<Wrap>,
    #[serde(default)]
    pub(super) recovery: Option<Wrap>,
    /// DPAPI-protected JSON of `Index`.
    #[serde(with = "crypto::b64", default)]
    pub(super) index: Vec<u8>,
    /// Sealed `Secret`s by entry id.
    #[serde(default)]
    pub(super) secrets: BTreeMap<String, Sealed>,
    /// Sealed card numbers (and security codes) by card id.
    #[serde(default)]
    pub(super) card_secrets: BTreeMap<String, Sealed>,
}

impl VaultFile {
    pub(super) fn empty() -> Self {
        Self { version: 1, master: None, recovery: None, index: Vec::new(), secrets: BTreeMap::new(), card_secrets: BTreeMap::new() }
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct Index {
    #[serde(default)]
    pub(super) entries: Vec<Meta>,
    /// Saved payment cards (see `cards`).
    #[serde(default)]
    pub(super) cards: Vec<cards::CardMeta>,
    /// Sites the user chose "Never save" for.
    #[serde(default)]
    pub(super) never: Vec<String>,
    /// Key for password fingerprints (see `crypto::fingerprint`).
    #[serde(with = "crypto::b64", default)]
    pub(super) check_key: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct Meta {
    pub(super) id: String,
    pub(super) origin: String,
    pub(super) username: String,
    #[serde(default)]
    pub(super) label: String,
    pub(super) created: i64,
    pub(super) updated: i64,
    #[serde(default)]
    pub(super) last_used: i64,
    #[serde(default)]
    pub(super) times_used: u32,
    #[serde(default)]
    pub(super) check: u32,
    #[serde(default)]
    pub(super) has_notes: bool,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Secret {
    pub(super) password: String,
    #[serde(default)]
    pub(super) notes: String,
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.password.zeroize();
        self.notes.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct BackupFile {
    pub(super) format: String,
    pub(super) version: u32,
    pub(super) created: i64,
    pub(super) count: usize,
    pub(super) kdf: KdfParams,
    #[serde(with = "crypto::b64")]
    pub(super) salt: Vec<u8>,
    pub(super) data: Sealed,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BackupEntry {
    pub(super) url: String,
    pub(super) username: String,
    pub(super) password: String,
    #[serde(default)]
    pub(super) label: String,
    #[serde(default)]
    pub(super) notes: String,
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
