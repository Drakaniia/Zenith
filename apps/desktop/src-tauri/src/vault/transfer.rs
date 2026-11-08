//! Import and export: the plain CSV export, encrypted backups (making and reading them), and
//! previewing and importing rows from another password manager or a backup.

use std::collections::HashMap;

use zeroize::Zeroizing;

use super::*;

impl Vault {

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
