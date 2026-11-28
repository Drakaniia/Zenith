//! Saved addresses for filling forms (name, street, city, phone, email…).
//!
//! Addresses live in the vault's DPAPI-protected index, like the sites and usernames of passwords
//! and the card metadata: encrypted at rest for this Windows account, and readable while the vault
//! is locked, so a form can be filled without typing the master password (as other browsers do).
//! Managing them in zenith://passwords needs the vault unlocked, like everything else there.

use serde::{Deserialize, Serialize};

use super::*;

const MAX_FIELD: usize = 300;
const MAX_STREET: usize = 1000;
const NO_ADDRESS: &str = "That address no longer exists.";

/// One saved address. The same shape is stored, sent to the UI and received from it (the id and
/// times of an incoming one are ignored).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Address {
    pub id: String,
    pub name: String,
    pub organization: String,
    /// One line per street line.
    pub street: String,
    pub city: String,
    /// State, province or region.
    pub region: String,
    pub postal: String,
    pub country: String,
    pub phone: String,
    pub email: String,
    pub created: i64,
    pub updated: i64,
}

impl Address {
    fn fields(&self) -> [&str; 9] {
        [&self.name, &self.organization, &self.street, &self.city, &self.region, &self.postal, &self.country, &self.phone, &self.email]
    }

    /// The same address, trimmed and checked.
    fn cleaned(&self) -> Result<Address, String> {
        let t = |s: &str| s.trim().to_string();
        let street = self.street.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
        let a = Address {
            name: t(&self.name),
            organization: t(&self.organization),
            street,
            city: t(&self.city),
            region: t(&self.region),
            postal: t(&self.postal),
            country: t(&self.country),
            phone: t(&self.phone),
            email: t(&self.email),
            ..Address::default()
        };
        check_len(&a.street, MAX_STREET, "street address")?;
        for f in a.fields() {
            check_len(f, MAX_FIELD, "address field")?;
        }
        if a.fields().iter().all(|f| f.is_empty()) {
            return Err("Fill in at least one field.".into());
        }
        Ok(a)
    }

    /// Same contents, ignoring case (for skipping duplicates on import).
    fn same_as(&self, other: &Address) -> bool {
        self.fields().iter().zip(other.fields()).all(|(a, b)| a.to_lowercase() == b.to_lowercase())
    }
}

impl Vault {
    /// Addresses for the manager. Needs the vault unlocked.
    pub fn addresses_list(&mut self) -> Result<Vec<Address>, String> {
        self.key()?;
        self.touch();
        Ok(self.index.addresses.clone())
    }

    /// Addresses that can be offered in a page's form. Works while locked.
    pub fn addresses_for_page(&self) -> Vec<Address> {
        if !self.exists() || self.damaged.is_some() {
            return Vec::new();
        }
        self.index.addresses.clone()
    }

    pub fn address(&self, id: &str) -> Option<Address> {
        self.addresses_for_page().into_iter().find(|a| a.id == id)
    }

    pub fn address_add(&mut self, input: &Address) -> Result<String, String> {
        self.key()?;
        let mut a = input.cleaned()?;
        a.id = new_id();
        a.created = now_ms();
        a.updated = a.created;
        let id = a.id.clone();
        self.index.addresses.push(a);
        self.save()?;
        self.touch();
        Ok(id)
    }

    pub fn address_update(&mut self, id: &str, input: &Address) -> Result<(), String> {
        self.key()?;
        let clean = input.cleaned()?;
        let a = self.index.addresses.iter_mut().find(|a| a.id == id).ok_or(NO_ADDRESS)?;
        *a = Address { id: a.id.clone(), created: a.created, updated: now_ms(), ..clean };
        self.save()?;
        self.touch();
        Ok(())
    }

    pub fn addresses_delete(&mut self, ids: &[String]) -> Result<usize, String> {
        self.key()?;
        let before = self.index.addresses.len();
        self.index.addresses.retain(|a| !ids.contains(&a.id));
        self.save()?;
        self.touch();
        Ok(before - self.index.addresses.len())
    }

    /// Adds addresses read from another browser, skipping empty ones and ones already saved.
    /// Needs a master password set up (the vault file), not an unlocked vault.
    pub fn addresses_import(&mut self, rows: &[Address]) -> Result<usize, String> {
        if let Some(d) = &self.damaged {
            return Err(d.clone());
        }
        if !self.exists() {
            return Err("Set up a master password in Passwords first.".into());
        }
        let mut added = 0;
        for row in rows {
            let Ok(mut a) = row.cleaned() else { continue };
            if self.index.addresses.iter().any(|x| x.same_as(&a)) {
                continue;
            }
            a.id = new_id();
            a.created = if row.created > 0 { row.created } else { now_ms() };
            a.updated = now_ms();
            self.index.addresses.push(a);
            added += 1;
        }
        if added > 0 {
            self.save()?;
        }
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault(name: &str) -> Vault {
        let dir = std::env::temp_dir().join(format!("zenith-addresses-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Vault::open(dir.join("vault.json"), KdfParams::FAST)
    }

    fn home() -> Address {
        Address {
            name: " Ann Lee ".into(),
            street: "1 Main St\n\n  Apt 4 ".into(),
            city: "Springfield".into(),
            postal: "12345".into(),
            country: "US".into(),
            email: "ann@example.com".into(),
            ..Address::default()
        }
    }

    #[test]
    fn add_edit_delete_and_file_stays_encrypted() {
        let mut v = temp_vault("crud");
        v.setup("master master").unwrap();
        let id = v.address_add(&home()).unwrap();
        let a = &v.addresses_list().unwrap()[0];
        assert_eq!((a.name.as_str(), a.street.as_str()), ("Ann Lee", "1 Main St\nApt 4"));

        let raw = String::from_utf8_lossy(&std::fs::read(&v.path).unwrap()).to_string();
        for needle in ["Ann Lee", "Main St", "Springfield", "ann@example.com"] {
            assert!(!raw.contains(needle), "{needle} leaked into the file");
        }
        assert!(raw.contains("\"version\": 3"), "older versions must not open (and drop) addresses");

        v.lock();
        assert!(v.addresses_list().is_err(), "the manager needs the vault unlocked");
        let again = Vault::open(v.path.clone(), KdfParams::FAST);
        assert_eq!(again.addresses_for_page().len(), 1, "forms can be filled while locked");
        assert_eq!(again.address(&id).unwrap().city, "Springfield");

        v.unlock("master master").unwrap();
        let mut e = home();
        e.city = "Shelbyville".into();
        v.address_update(&id, &e).unwrap();
        assert_eq!(v.addresses_list().unwrap()[0].city, "Shelbyville");
        assert_eq!(v.addresses_list().unwrap()[0].id, id);
        assert!(v.address_add(&Address::default()).is_err(), "an empty address is refused");
        assert_eq!(v.addresses_delete(&[id]).unwrap(), 1);
        assert!(v.addresses_list().unwrap().is_empty());
        let raw = std::fs::read_to_string(&v.path).unwrap();
        assert!(raw.contains("\"version\": 1"), "back to the old format once none are left");
    }

    #[test]
    fn import_skips_duplicates_and_needs_a_vault() {
        let mut v = temp_vault("import");
        assert!(v.addresses_import(&[home()]).is_err());
        v.setup("master master").unwrap();
        v.lock();
        let mut upper = home();
        upper.name = "ANN LEE".into();
        assert_eq!(v.addresses_import(&[home(), upper, Address::default()]).unwrap(), 1);
        assert_eq!(v.addresses_import(&[home()]).unwrap(), 0);
        assert_eq!(v.addresses_for_page().len(), 1);
    }
}
