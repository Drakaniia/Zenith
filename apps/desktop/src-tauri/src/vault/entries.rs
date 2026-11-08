//! Saved logins: listing, revealing, adding, editing and deleting them, the accounts offered on a
//! login page and filling them, classifying and saving captured logins, and the "never save" list.

use zeroize::Zeroizing;

use super::*;

impl Vault {

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

    pub(super) fn secret(&self, m: &Meta) -> Result<Secret, String> {
        let key = self.key()?;
        let sealed = self.file.secrets.get(&m.id).ok_or("This password is missing from the vault.")?;
        let plain = crypto::open(key, sealed, &entry_aad(&m.id, &m.origin, &m.username))
            .ok_or("This password could not be decrypted: the vault may be damaged.")?;
        serde_json::from_slice::<Secret>(&plain).map_err(|e| e.to_string())
    }

    pub(super) fn meta(&self, id: &str) -> Result<&Meta, String> {
        self.index.entries.iter().find(|m| m.id == id).ok_or_else(|| "That password no longer exists.".to_string())
    }

    pub fn reveal(&mut self, id: &str) -> Result<Revealed, String> {
        let m = self.meta(id)?.clone();
        let s = self.secret(&m)?;
        self.touch();
        Ok(Revealed { password: s.password.clone(), notes: s.notes.clone() })
    }

    /// Adds an entry without saving the file.
    pub(super) fn insert(&mut self, origin: &str, username: &str, password: &str, label: &str, notes: &str) -> Result<String, String> {
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
    pub(super) fn rewrite(&mut self, id: &str, origin: &str, username: &str, password: &str, label: &str, notes: &str) -> Result<(), String> {
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
}
