//! Saved passwords and payment cards from Chromium browsers (Chrome, Edge, Brave, Vivaldi,
//! Chromium, Opera, Opera GX).
//!
//! Chromium encrypts them with an AES-256-GCM key stored in the browser's `Local State`, itself
//! protected with Windows DPAPI for this user. Values written with that key start with "v10" or
//! "v11". Values starting with "v20" use Chrome's app-bound encryption (Chrome 127+), which only
//! the browser itself can open: they are skipped and the user is told to export a CSV instead.
//! Yandex is not in the browser list: its password store adds its own encryption layer.
//!
//! Plain text secrets only ever live in `Zeroizing` buffers and go straight to the vault.

use std::fs;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;
use rusqlite::Connection;
use zeroize::Zeroizing;

use super::{scan, with_copy, Engine};
use crate::vault::cards::{digits_only, parse_expiry, valid_number, CardCapture, CardOffer};
use crate::vault::csv::ImportRow;
use crate::vault::{crypto, Vault};

/// A payment card read from another browser (the number is checked).
pub struct Card {
    pub number: Zeroizing<String>,
    pub holder: String,
    pub exp_month: u8,
    pub exp_year: u16,
}

/// What was read from a profile, before it is checked against the vault.
#[derive(Default)]
pub struct Secrets {
    /// The browser's name, for the import preview.
    pub name: String,
    pub logins: Vec<ImportRow>,
    pub cards: Vec<Card>,
    /// Parts that could not be read, as sentences for the UI.
    pub errors: Vec<String>,
}

#[derive(Debug, PartialEq)]
enum Fail {
    /// "v20": Chrome's app-bound encryption.
    AppBound,
    Broken,
}

/// Tallies of values that could not be decrypted.
#[derive(Default)]
struct Skipped {
    app_bound: usize,
    broken: usize,
}

impl Skipped {
    fn note(&mut self, f: Fail) {
        match f {
            Fail::AppBound => self.app_bound += 1,
            Fail::Broken => self.broken += 1,
        }
    }
}

/// The AES key from a Chromium `Local State` file.
fn master_key(local_state: &str) -> Option<crypto::Key> {
    let value: serde_json::Value = serde_json::from_str(local_state).ok()?;
    let raw = base64::engine::general_purpose::STANDARD.decode(value["os_crypt"]["encrypted_key"].as_str()?).ok()?;
    let plain = crypto::unprotect_foreign(raw.strip_prefix(b"DPAPI")?).ok()?;
    let key: [u8; 32] = plain.as_slice().try_into().ok()?;
    Some(Zeroizing::new(key))
}

/// One encrypted value of a Chromium profile.
fn decrypt(key: &[u8; 32], blob: &[u8]) -> Result<Zeroizing<String>, Fail> {
    if blob.is_empty() {
        return Ok(Zeroizing::default());
    }
    let plain = match &blob[..blob.len().min(3)] {
        b"v10" | b"v11" => {
            let rest = &blob[3..];
            if rest.len() < 12 + 16 {
                return Err(Fail::Broken);
            }
            let (nonce, data) = rest.split_at(12);
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| Fail::Broken)?;
            Zeroizing::new(cipher.decrypt(Nonce::from_slice(nonce), data).map_err(|_| Fail::Broken)?)
        }
        b"v20" => return Err(Fail::AppBound),
        // Before Chrome 80 each value was its own DPAPI blob.
        _ => crypto::unprotect_foreign(blob).map_err(|_| Fail::Broken)?,
    };
    std::str::from_utf8(&plain).map(|s| Zeroizing::new(s.to_string())).map_err(|_| Fail::Broken)
}

fn login_row(key: &[u8; 32], url: String, username: String, blob: &[u8]) -> Result<ImportRow, Fail> {
    Ok(ImportRow {
        url,
        username: username.trim().to_string(),
        password: decrypt(key, blob)?,
        label: String::new(),
        notes: Zeroizing::default(),
    })
}

/// `None` for a card whose number or expiry isn't valid.
fn card_row(key: &[u8; 32], holder: String, month: i64, year: i64, blob: &[u8]) -> Result<Option<Card>, Fail> {
    let plain = decrypt(key, blob)?;
    let Some(number) = digits_only(&plain).filter(|n| valid_number(n)) else { return Ok(None) };
    let Some((exp_month, exp_year)) = parse_expiry(&month.to_string(), &year.to_string()) else { return Ok(None) };
    Ok(Some(Card { number, holder: holder.trim().to_string(), exp_month, exp_year }))
}

fn logins(conn: &Connection, key: &[u8; 32], skipped: &mut Skipped) -> rusqlite::Result<Vec<ImportRow>> {
    let mut stmt = conn.prepare("SELECT origin_url, username_value, password_value FROM logins WHERE blacklisted_by_user = 0")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, Zeroizing::new(r.get::<_, Vec<u8>>(2)?))))?;
    let mut out = Vec::new();
    for (url, user, blob) in rows.filter_map(Result::ok) {
        match login_row(key, url, user, &blob) {
            Ok(row) => out.push(row),
            Err(f) => skipped.note(f),
        }
    }
    Ok(out)
}

fn cards(conn: &Connection, key: &[u8; 32], skipped: &mut Skipped) -> rusqlite::Result<Vec<Card>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(name_on_card, ''), expiration_month, expiration_year, card_number_encrypted FROM credit_cards",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, Zeroizing::new(r.get::<_, Vec<u8>>(3)?)))
    })?;
    let mut out = Vec::new();
    for (holder, month, year, blob) in rows.filter_map(Result::ok) {
        match card_row(key, holder, month, year, &blob) {
            Ok(Some(card)) => out.push(card),
            Ok(None) => {}
            Err(f) => skipped.note(f),
        }
    }
    Ok(out)
}

/// Reads the saved passwords and/or cards of a Chromium profile found by `detect`.
pub fn read(id: &str, passwords: bool, payment_cards: bool) -> Result<Secrets, String> {
    let found = scan()
        .into_iter()
        .find(|f| f.source.id == id)
        .ok_or_else(|| "That browser profile is no longer available.".to_string())?;
    let name = found.source.name.clone();
    if found.engine != Engine::Chromium || found.source.browser == "yandex" {
        return Err(format!("Zenith can't read {name}'s passwords directly. Export them from {name} as a CSV file and import that."));
    }
    // Opera keeps `Local State` in its profile folder; the others one level up.
    let local_state = [Some(found.dir.as_path()), found.dir.parent()]
        .into_iter()
        .flatten()
        .map(|d| d.join("Local State"))
        .find(|p| p.is_file())
        .and_then(|p| fs::read_to_string(p).ok())
        .ok_or_else(|| format!("Couldn't find {name}'s settings."))?;
    let key = master_key(&local_state).ok_or_else(|| format!("Couldn't unlock {name}'s saved data for this Windows account."))?;

    let mut out = Secrets { name: name.clone(), ..Default::default() };
    let mut skipped = Skipped::default();
    if passwords && found.dir.join("Login Data").is_file() {
        match with_copy(&found.dir.join("Login Data"), |c| logins(c, &key, &mut skipped)) {
            Ok(rows) => out.logins = rows,
            Err(_) => out.errors.push(format!("Couldn't read {name}'s passwords. Close {name} and try again.")),
        }
    }
    if payment_cards && found.dir.join("Web Data").is_file() {
        match with_copy(&found.dir.join("Web Data"), |c| cards(c, &key, &mut skipped)) {
            Ok(list) => out.cards = list,
            Err(_) => out.errors.push(format!("Couldn't read {name}'s payment cards. Close {name} and try again.")),
        }
    }
    if skipped.app_bound > 0 {
        out.errors.push(format!(
            "{} saved {} in {name} use app-bound encryption, which only {name} can read. Export your passwords from {name} as a CSV file (Settings › Passwords › Export passwords) and import that file instead.",
            skipped.app_bound,
            if skipped.app_bound == 1 { "item" } else { "items" },
        ));
    }
    if skipped.broken > 0 {
        out.errors.push(format!("{} saved {} in {name} couldn't be decrypted.", skipped.broken, if skipped.broken == 1 { "item" } else { "items" }));
    }
    Ok(out)
}

/// Adds the cards the wallet doesn't have yet. A card already saved (same brand and last four
/// digits) is kept as it is, like a password conflict's default. Returns how many were added.
pub fn save_cards(v: &mut Vault, list: &[Card]) -> Result<usize, String> {
    let mut added = 0;
    for c in list {
        if v.classify_card(&c.number, c.exp_month, c.exp_year) != CardCapture::New {
            continue;
        }
        let offer = CardOffer::new("", "", c.number.clone(), Zeroizing::default(), &c.holder, c.exp_month, c.exp_year, None);
        v.save_card(&offer, false)?;
        added += 1;
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];

    fn v10(plain: &str) -> Vec<u8> {
        let nonce = [9u8; 12];
        let mut out = b"v10".to_vec();
        out.extend_from_slice(&nonce);
        out.extend(Aes256Gcm::new_from_slice(&KEY).unwrap().encrypt(Nonce::from_slice(&nonce), plain.as_bytes()).unwrap());
        out
    }

    #[test]
    fn decrypts_v10_and_rejects_v20() {
        assert_eq!(*decrypt(&KEY, &v10("hunter2")).unwrap(), "hunter2");
        let mut v11 = v10("x");
        v11[2] = b'1';
        assert_eq!(*decrypt(&KEY, &v11).unwrap(), "x");
        assert_eq!(*decrypt(&KEY, b"").unwrap(), "");
        let mut v20 = v10("secret");
        v20[1] = b'2';
        v20[2] = b'0';
        assert_eq!(decrypt(&KEY, &v20).unwrap_err(), Fail::AppBound);
        assert_eq!(decrypt(&[1; 32], &v10("hunter2")).unwrap_err(), Fail::Broken, "wrong key");
        assert_eq!(decrypt(&KEY, b"v10short").unwrap_err(), Fail::Broken);
    }

    #[test]
    fn maps_logins_and_cards() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE logins (origin_url TEXT, username_value TEXT, password_value BLOB, blacklisted_by_user INTEGER);
             CREATE TABLE credit_cards (name_on_card TEXT, expiration_month INTEGER, expiration_year INTEGER, card_number_encrypted BLOB);",
        )
        .unwrap();
        let mut v20 = v10("pw");
        v20[1] = b'2';
        v20[2] = b'0';
        let add_login = |url: &str, user: &str, blob: &[u8], never: i64| {
            c.execute("INSERT INTO logins VALUES (?1, ?2, ?3, ?4)", rusqlite::params![url, user, blob, never]).unwrap();
        };
        add_login("https://a.example/login", " ann ", &v10("pw1"), 0);
        add_login("https://b.example/", "bob", &v20, 0);
        add_login("https://never.example/", "", b"", 1);
        let add_card = |holder: Option<&str>, m: i64, y: i64, blob: &[u8]| {
            c.execute("INSERT INTO credit_cards VALUES (?1, ?2, ?3, ?4)", rusqlite::params![holder, m, y, blob]).unwrap();
        };
        add_card(Some("Ann Lee"), 8, 2031, &v10("4111 1111 1111 1111"));
        add_card(None, 13, 2031, &v10("4111111111111111"));
        add_card(Some("Bad"), 1, 2030, &v10("1234"));
        add_card(Some("Locked"), 1, 2030, &v20);

        let mut skipped = Skipped::default();
        let rows = logins(&c, &KEY, &mut skipped).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url, "https://a.example/login");
        assert_eq!(rows[0].username, "ann");
        assert_eq!(*rows[0].password, "pw1");
        let list = cards(&c, &KEY, &mut skipped).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(*list[0].number, "4111111111111111");
        assert_eq!((list[0].holder.as_str(), list[0].exp_month, list[0].exp_year), ("Ann Lee", 8, 2031));
        assert_eq!((skipped.app_bound, skipped.broken), (2, 0));
    }

    #[test]
    fn reads_the_key_from_local_state() {
        assert!(master_key("not json").is_none());
        assert!(master_key(r#"{"os_crypt":{"encrypted_key":"bm9wZQ=="}}"#).is_none(), "no DPAPI prefix");
        #[cfg(windows)]
        {
            use base64::engine::general_purpose::STANDARD;
            let protected = windows_protect(&KEY);
            let json = format!(r#"{{"os_crypt":{{"encrypted_key":"{}"}}}}"#, STANDARD.encode([b"DPAPI".as_slice(), &protected].concat()));
            assert_eq!(*master_key(&json).unwrap(), KEY);
        }
    }

    /// DPAPI without entropy, the way Chromium protects its key.
    #[cfg(windows)]
    fn windows_protect(data: &[u8]) -> Vec<u8> {
        use windows::Win32::Foundation::{LocalFree, HLOCAL};
        use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
        unsafe {
            let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
            let mut out = CRYPT_INTEGER_BLOB::default();
            CryptProtectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).unwrap();
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            let _ = LocalFree(Some(HLOCAL(out.pbData as *mut core::ffi::c_void)));
            bytes
        }
    }
}
