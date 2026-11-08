//! The vault itself: opening and saving the file, the master password and recovery key (setup,
//! unlock, verify, change, recover, reset), the retry backoff and the lock state.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use super::*;

pub(super) fn entry_aad(id: &str, origin: &str, username: &str) -> Vec<u8> {
    format!("zenith-vault:entry:v1\n{id}\n{origin}\n{username}").into_bytes()
}

pub(super) fn new_id() -> String {
    crypto::random::<8>().iter().map(|b| format!("{b:02x}")).collect()
}

pub(super) fn now_ms() -> i64 {
    crate::storage::now_ms()
}

pub(super) fn check_len(value: &str, max: usize, what: &str) -> Result<(), String> {
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
    pub(super) path: PathBuf,
    pub(super) file: VaultFile,
    pub(super) index: Index,
    key: Option<Key>,
    last_used: Instant,
    failures: u32,
    last_failure: Option<Instant>,
    pub(super) damaged: Option<String>,
    pub(super) kdf: KdfParams,
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

    pub(super) fn save(&mut self) -> Result<(), String> {
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

    pub(super) fn key(&self) -> Result<&Key, String> {
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
}
