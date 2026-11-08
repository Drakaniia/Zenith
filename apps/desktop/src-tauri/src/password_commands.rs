//! Tauri commands for the password manager and autofill (see `vault/` and `host::autofill`).
//!
//! Every vault call runs on a worker thread: checking a master password takes about half a
//! second. Passwords reach the chrome UI only when the user reveals one in the manager; copying
//! and filling happen here, in Rust.

use std::collections::HashMap;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

use crate::host::{self, autofill};
use crate::{store, vault, win};

pub(crate) type CmdResult<T> = Result<T, String>;

/// How long a copied password stays on the clipboard.
pub(crate) const CLIPBOARD_CLEAR: std::time::Duration = std::time::Duration::from_secs(30);

pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

pub(crate) async fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|_| "the UI thread did not respond".to_string())
}

pub(crate) async fn on_main_cb<T: Send + 'static>(app: &AppHandle, f: impl FnOnce(Box<dyn FnOnce(T)>) + Send + 'static) -> CmdResult<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        f(Box::new(move |v| {
            let _ = tx.send(v);
        }))
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|_| "the operation was dropped".to_string())
}

pub(crate) async fn vault_call<T: Send + 'static>(f: impl FnOnce(&mut vault::Vault) -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    blocking(move || vault::with(f)).await?
}

/// A fresh master password when one is given (or the settings ask for it every time);
/// otherwise an unlocked vault is enough.
fn authorize(v: &mut vault::Vault, master: Option<&Zeroizing<String>>) -> CmdResult<()> {
    match master {
        Some(m) => v.verify(m),
        None if !v.unlocked() || host::config().vault_ask_every_time => Err(vault::NEED_MASTER.into()),
        None => Ok(()),
    }
}

pub(crate) fn secret(s: Option<String>) -> Option<Zeroizing<String>> {
    s.map(Zeroizing::new)
}

/// Today's date as YYYY-MM-DD (UTC), for backup file names.
fn today() -> String {
    let days = store::now_ms() / 1000 / 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

// ---------------------------------------------------------------------------------------------
// Master password

#[tauri::command]
pub async fn vault_status() -> CmdResult<vault::Status> {
    blocking(vault::status).await
}

/// Sets up the master password; returns the recovery key (shown once, never stored).
#[tauri::command]
pub async fn vault_setup(master: String) -> CmdResult<String> {
    let master = Zeroizing::new(master);
    let key = vault_call(move |v| v.setup(&master)).await?;
    vault::broadcast();
    Ok(key.to_string())
}

#[tauri::command]
pub async fn vault_unlock(master: String) -> CmdResult<vault::Status> {
    let master = Zeroizing::new(master);
    let result = vault_call(move |v| v.unlock(&master)).await;
    vault::broadcast();
    result?;
    blocking(vault::status).await
}

#[tauri::command]
pub async fn vault_lock() -> CmdResult<()> {
    blocking(vault::lock).await?;
    vault::broadcast();
    Ok(())
}

#[tauri::command]
pub async fn vault_change_master(current: String, new: String) -> CmdResult<()> {
    let (current, new) = (Zeroizing::new(current), Zeroizing::new(new));
    vault_call(move |v| v.change_master(&current, &new)).await?;
    vault::broadcast();
    Ok(())
}

/// Forgot master password: the recovery key sets a new one. Returns the new recovery key.
#[tauri::command]
pub async fn vault_recover(recovery_key: String, new_master: String) -> CmdResult<String> {
    let (recovery_key, new_master) = (Zeroizing::new(recovery_key), Zeroizing::new(new_master));
    let result = vault_call(move |v| v.recover(&recovery_key, &new_master)).await;
    vault::broadcast();
    Ok(result?.to_string())
}

#[tauri::command]
pub async fn vault_new_recovery_key(master: String) -> CmdResult<String> {
    let master = Zeroizing::new(master);
    Ok(vault_call(move |v| v.new_recovery_key(&master)).await?.to_string())
}

/// Deletes every saved password and the master password. The UI makes the user type DELETE.
#[tauri::command]
pub async fn vault_reset(confirm: String) -> CmdResult<()> {
    if confirm != "DELETE" {
        return Err("Type DELETE to confirm.".into());
    }
    vault_call(|v| v.reset()).await?;
    vault::clear_imports();
    vault::broadcast();
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Saved passwords

#[tauri::command]
pub async fn passwords_list() -> CmdResult<Vec<vault::EntryInfo>> {
    vault_call(|v| v.list()).await
}

#[tauri::command]
pub async fn password_reveal(id: String, master: Option<String>) -> CmdResult<vault::Revealed> {
    let master = secret(master);
    let r = vault_call(move |v| {
        authorize(v, master.as_ref())?;
        v.reveal(&id)
    })
    .await;
    vault::broadcast();
    r
}

/// Copies a username, or a password (kept out of clipboard history, cleared after 30 s).
#[tauri::command]
pub async fn password_copy(id: String, field: String, master: Option<String>) -> CmdResult<()> {
    let master = secret(master);
    let r = vault_call(move |v| {
        if field == "username" {
            let user = v.list()?.into_iter().find(|e| e.id == id).map(|e| e.username).ok_or("That password no longer exists.")?;
            return win::set_clipboard_text(&user);
        }
        authorize(v, master.as_ref())?;
        let revealed = v.reveal(&id)?;
        win::copy_secret(&revealed.password, CLIPBOARD_CLEAR)
    })
    .await;
    vault::broadcast();
    r
}

#[tauri::command]
pub async fn password_add(entry: vault::EntryInput) -> CmdResult<String> {
    vault_call(move |v| v.add(&entry)).await
}

#[tauri::command]
pub async fn password_update(id: String, entry: vault::EntryInput) -> CmdResult<()> {
    vault_call(move |v| v.update(&id, &entry)).await
}

#[tauri::command]
pub async fn passwords_delete(ids: Vec<String>) -> CmdResult<usize> {
    vault_call(move |v| v.delete(&ids)).await
}

#[tauri::command]
pub async fn password_generate(length: Option<usize>) -> CmdResult<String> {
    Ok(vault::crypto::generate_password(length.unwrap_or(20)).to_string())
}

#[tauri::command]
pub async fn passwords_never_list() -> CmdResult<Vec<String>> {
    vault_call(|v| Ok(v.never_list())).await
}

#[tauri::command]
pub async fn passwords_never_remove(site: String) -> CmdResult<()> {
    vault_call(move |v| v.set_never(&site, false)).await
}

// ---------------------------------------------------------------------------------------------
// Export, backup and import

/// Writes every password, unencrypted, to a CSV file the user picks. Always asks for the master
/// password.
#[tauri::command]
pub async fn passwords_export_csv(app: AppHandle, master: String) -> CmdResult<Option<String>> {
    let master = Zeroizing::new(master);
    let text = vault_call(move |v| {
        v.verify(&master)?;
        v.export_csv()
    })
    .await;
    vault::broadcast();
    let text = text?;
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Export passwords (not encrypted)")
            .set_file_name("Zenith Passwords.csv")
            .add_filter("CSV", &["csv"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        store::write_atomic(&path, text.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await?
}

/// Saves an encrypted backup that opens with the current master password.
#[tauri::command]
pub async fn passwords_backup(app: AppHandle, master: String) -> CmdResult<Option<String>> {
    let master = Zeroizing::new(master);
    let bytes = vault_call(move |v| {
        v.verify(&master)?;
        v.backup(&master)
    })
    .await;
    vault::broadcast();
    let bytes = bytes?;
    let name = format!("Zenith Passwords {}.zenithpasswords", today());
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Back up passwords")
            .set_file_name(name)
            .add_filter("Zenith password backup", &["zenithpasswords"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        store::write_atomic(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOpened {
    token: u64,
    file_name: String,
    /// An encrypted backup: call `passwords_import_unlock` with its password.
    needs_password: bool,
    rows: Vec<vault::PreviewRow>,
}

/// Picks a CSV export or a Zenith backup and previews it. Nothing is saved yet.
#[tauri::command]
pub async fn passwords_import_open(app: AppHandle) -> CmdResult<Option<ImportOpened>> {
    vault_call(|v| v.list().map(|_| ())).await?;
    let picked = blocking(move || {
        app.dialog()
            .file()
            .set_title("Import passwords")
            .add_filter("Password export (CSV) or Zenith backup", &["csv", "zenithpasswords"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
    })
    .await?;
    let Some(path) = picked else { return Ok(None) };
    let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let bytes = blocking(move || {
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        if meta.len() > 32 * 1024 * 1024 {
            return Err("That file is too large to be a password export.".to_string());
        }
        std::fs::read(&path).map(Zeroizing::new).map_err(|e| e.to_string())
    })
    .await??;
    if vault::is_backup(&bytes) {
        let token = vault::stash_import(vault::PendingImport::Backup(bytes.to_vec()));
        return Ok(Some(ImportOpened { token, file_name, needs_password: true, rows: Vec::new() }));
    }
    let text = Zeroizing::new(String::from_utf8_lossy(&bytes).to_string());
    let rows = vault::csv::read_export(&text)?;
    let (rows, preview) = blocking(move || vault::with(|v| v.preview(&rows).map(|p| (rows, p)))).await??;
    let token = vault::stash_import(vault::PendingImport::Rows(rows));
    Ok(Some(ImportOpened { token, file_name, needs_password: false, rows: preview }))
}

/// Opens an encrypted backup picked with `passwords_import_open`. On a wrong password the
/// file stays available under the same token.
#[tauri::command]
pub async fn passwords_import_unlock(token: u64, password: String) -> CmdResult<ImportOpened> {
    let password = Zeroizing::new(password);
    let Some(vault::PendingImport::Backup(bytes)) = vault::take_import(token) else {
        return Err("Choose the backup file again.".into());
    };
    let (rows, bytes) = blocking(move || (vault::read_backup(&bytes, &password), bytes)).await?;
    let rows = match rows {
        Ok(rows) => rows,
        Err(e) => {
            vault::restore_import(token, vault::PendingImport::Backup(bytes));
            return Err(e);
        }
    };
    drop(Zeroizing::new(bytes));
    let (rows, preview) = blocking(move || vault::with(|v| v.preview(&rows).map(|p| (rows, p)))).await??;
    let token = vault::stash_import(vault::PendingImport::Rows(rows));
    Ok(ImportOpened { token, file_name: String::new(), needs_password: false, rows: preview })
}

/// Saves a previewed import. `choices` by row index: "skip", "replace" or "both".
#[tauri::command]
pub async fn passwords_import_commit(token: u64, choices: HashMap<usize, String>) -> CmdResult<vault::ImportSummary> {
    let Some(vault::PendingImport::Rows(rows)) = vault::take_import(token) else {
        return Err("This import expired. Choose the file again.".into());
    };
    vault_call(move |v| v.import(&rows, &choices)).await
}

#[tauri::command]
pub async fn passwords_import_cancel(token: u64) -> CmdResult<()> {
    vault::take_import(token);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Autofill

/// Fills a saved account into the tab's login form, after checking the page's site.
#[tauri::command]
pub async fn autofill_fill(app: AppHandle, tab_id: String, entry_id: String, master: Option<String>) -> CmdResult<()> {
    let cfg = host::config();
    if !cfg.passwords_enabled {
        return Err("The password manager is turned off.".into());
    }
    let master = secret(master);
    let tab = tab_id.clone();
    let origin = on_main(&app, move || autofill::tab_origin(&tab)).await?.ok_or("This page can't be filled.")?;
    let page = origin.clone();
    let same_site = cfg.autofill_same_site;
    let creds = vault_call(move |v| {
        authorize(v, master.as_ref())?;
        v.credentials_for_fill(&entry_id, &page, same_site)
    })
    .await;
    vault::broadcast();
    let (user, pass) = creds?;
    let user = cfg.autofill_usernames.then_some(user);
    let pass = cfg.autofill_passwords.then_some(pass);
    on_main_cb(&app, move |done| {
        autofill::close_suggestions(&tab_id);
        autofill::fill(&tab_id, &origin, user.as_deref(), pass.as_deref().map(|p| p.as_str()), "login", move |r| done(r));
    })
    .await?
}

/// Fills the strong password suggested in a sign-up form.
#[tauri::command]
pub async fn autofill_fill_generated(app: AppHandle, tab_id: String) -> CmdResult<()> {
    on_main_cb(&app, move |done| autofill::fill_generated(&tab_id, move |r| done(r))).await?
}

#[tauri::command]
pub async fn autofill_dismiss(app: AppHandle, tab_id: String) -> CmdResult<()> {
    // The user closed the list: typing in the field leaves it closed.
    on_main(&app, move || {
        autofill::mute_suggestions(&tab_id);
        autofill::close_suggestions(&tab_id)
    })
    .await
}

/// Answers a "Save password?" prompt: "save", "never" (for this site) or "dismiss". Saving
/// may need the master password; on any error the offer stays so the user can try again.
#[tauri::command]
pub async fn password_offer_respond(offer_id: u64, action: String, master: Option<String>) -> CmdResult<Option<String>> {
    let master = secret(master);
    match action.as_str() {
        "dismiss" => {
            vault::take_offer(offer_id);
            Ok(None)
        }
        "never" => {
            let Some(offer) = vault::take_offer(offer_id) else { return Ok(None) };
            vault_call(move |v| v.set_never(&vault::never_key(&offer.origin), true)).await?;
            Ok(None)
        }
        "save" => {
            let offer = vault::take_offer(offer_id).ok_or("This login is no longer waiting to be saved.")?;
            let result = blocking(move || {
                let r = vault::with(|v| {
                    if !v.exists() {
                        return Err("Set up a master password first.".to_string());
                    }
                    if let Some(m) = &master {
                        v.unlock(m)?;
                    }
                    if !v.unlocked() {
                        return Err(vault::NEED_MASTER.to_string());
                    }
                    v.save_login(&offer.origin, &offer.username, &offer.password, offer.update.as_deref())
                });
                if r.is_err() {
                    vault::restore_offer(offer_id, offer);
                }
                r
            })
            .await?;
            vault::broadcast();
            result.map(Some)
        }
        _ => Err("unknown action".into()),
    }
}
