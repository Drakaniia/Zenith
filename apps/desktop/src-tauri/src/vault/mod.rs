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

pub mod card_commands;
pub mod cards;
pub mod commands;
pub mod crypto;
pub mod csv;
mod entries;
mod offers;
mod origin;
mod transfer;
mod types;
mod vault;

pub use offers::*;
pub use origin::*;
pub use self::vault::*;
pub use transfer::*;
pub use types::*;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use zeroize::Zeroize;

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
    use std::collections::HashMap;

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
