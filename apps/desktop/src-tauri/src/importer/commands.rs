//! Commands for importing bookmarks, history, passwords and payment cards from other browsers.

use serde::Serialize;
use crate::app::runtime::WindowHandle;

use crate::app::command::{blocking, private, CmdResult};
use crate::bookmarks::commands::bookmarks_changed;
use crate::host::{bookmarks_db, db};
use crate::vault::commands::{vault_call, ImportOpened};
use crate::{bookmarks, importer, vault};

/// Browser profiles on this PC with bookmarks or history to import.
pub async fn import_sources() -> CmdResult<Vec<importer::ImportSource>> {
    blocking(importer::detect).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    /// New bookmarks (ones already present are skipped).
    bookmarks: usize,
    /// History entries merged.
    history: usize,
    errors: Vec<String>,
}

/// Adds the addresses saved in a Chromium-family profile to the password manager's vault (which
/// needs a master password set up). Returns how many were new.
pub async fn import_addresses(window: WindowHandle, source: String) -> CmdResult<usize> {
    if private(&window) {
        return Err("Incognito windows can't import data.".into());
    }
    let rows = blocking(move || importer::addresses::read(&source)).await??;
    let added = blocking(move || crate::vault::with(|v| v.addresses_import(&rows))).await??;
    crate::vault::broadcast();
    Ok(added)
}

/// Imports bookmarks and/or history from a profile found by `import_sources`. History older
/// than `history_days` (0: any age) is left out, since retention would drop it anyway.
pub async fn import_run(
    window: WindowHandle,
    source: String,
    bookmarks: bool,
    history: bool,
    history_days: i64,
) -> CmdResult<ImportResult> {
    if private(&window) {
        return Err("Incognito windows can't import data.".into());
    }
    let data = blocking(move || importer::read(&source, bookmarks, history, history_days)).await??;
    let marks = data.bookmarks;
    let added = if marks.is_empty() { 0 } else { bookmarks_db().call(move |c| bookmarks::import_paths(c, &marks)).await? };
    if added > 0 {
        bookmarks_changed(&window);
    }
    let visits: Vec<_> = data.history.into_iter().map(|v| (v.url, v.title, v.visit_count, v.last_visit)).collect();
    let merged = if visits.is_empty() { 0 } else { db().call(move |c| crate::history::history_import(c, &visits)).await? };
    Ok(ImportResult { bookmarks: added, history: merged, errors: data.errors })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretsImport {
    /// The passwords' import preview, committed with `passwords_import_commit` like a CSV import.
    passwords: Option<ImportOpened>,
    /// New cards added to the wallet.
    cards: usize,
    errors: Vec<String>,
}

/// Reads the saved passwords and/or payment cards of a Chromium profile found by `import_sources`.
/// Cards are added right away (ones already saved are kept); passwords are previewed first.
/// Needs the vault unlocked, like a CSV import.
pub async fn import_secrets(window: WindowHandle, source: String, passwords: bool, cards: bool) -> CmdResult<SecretsImport> {
    if private(&window) {
        return Err("Incognito windows can't import data.".into());
    }
    vault_call(|v| v.list().map(|_| ())).await?;
    let data = blocking(move || importer::secrets::read(&source, passwords, cards)).await??;
    let found = data.cards;
    let added = if found.is_empty() { 0 } else { vault_call(move |v| importer::secrets::save_cards(v, &found)).await? };
    if added > 0 {
        vault::broadcast();
    }
    let logins = data.logins;
    let passwords = if logins.is_empty() {
        None
    } else {
        let (rows, preview) = blocking(move || vault::with(|v| v.preview(&logins).map(|p| (logins, p)))).await??;
        let token = vault::stash_import(vault::PendingImport::Rows(rows));
        Some(ImportOpened { token, file_name: data.name, needs_password: false, rows: preview })
    };
    Ok(SecretsImport { passwords, cards: added, errors: data.errors })
}

/// Start-page tiles of a profile found by `import_sources`, for the UI to add to the Tableau
/// (the tiles live in settings.json, which the UI owns).
pub async fn import_tiles(window: WindowHandle, source: String) -> CmdResult<Vec<importer::tiles::Tile>> {
    if private(&window) {
        return Err("Incognito windows can't import data.".into());
    }
    blocking(move || importer::tiles::read(&source)).await?
}

/// The last session's open tabs (Chromium browsers) of a profile found by `import_sources`,
/// for the UI to open as tabs.
pub async fn import_tabs(window: WindowHandle, source: String) -> CmdResult<Vec<importer::tabs::ImportedTab>> {
    if private(&window) {
        return Err("Incognito windows can't import data.".into());
    }
    blocking(move || importer::tabs::read(&source)).await?
}
