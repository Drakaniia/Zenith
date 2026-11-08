//! Tauri commands for the chrome UI. Only the windows' local chrome webviews (`main`,
//! `incognito-N`) can call these; page webviews are raw WebView2 views without the Tauri bridge.
//! Commands that act on one window take the calling `WebviewWindow`.
//!
//! WebView2 objects are single-threaded, so webview work hops to the UI thread with
//! `run_on_main_thread`; disk and network work runs on blocking worker threads.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::command::{blocking, on_main, on_main_cb, private, refuse_in_incognito, CmdResult};
use crate::host::crx;
use crate::host::extensions::{self, ExtensionInfo};
use crate::host::page_tools;
use crate::host::tab::{self, CreateOptions};
use crate::host::{self, bookmarks_db, db, paths, LayoutSpec, BLOCKER};
use crate::platform::{net, urls};
use crate::storage;
use crate::{blocker, bookmarks, importer};

// ---------------------------------------------------------------------------------------------
// Tabs

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabCreateArgs {
    id: String,
    url: Option<String>,
    #[serde(default)]
    glance: bool,
    #[serde(default)]
    muted: bool,
    zoom: Option<f64>,
    scroll_x: Option<f64>,
    scroll_y: Option<f64>,
    opener_url: Option<String>,
}

#[tauri::command]
pub async fn tab_create(app: AppHandle, window: WebviewWindow, args: TabCreateArgs) -> CmdResult<()> {
    let label = window.label().to_string();
    let url = match &args.url {
        Some(u) => Some(urls::sanitize_navigation(u).ok_or("this address cannot be opened")?),
        None => None,
    };
    let scroll = args.scroll_x.zip(args.scroll_y).filter(|(x, y)| *x > 0.0 || *y > 0.0);
    on_main(&app, move || {
        tab::create(CreateOptions {
            id: args.id,
            window: label,
            url,
            glance: args.glance,
            muted: args.muted,
            zoom: args.zoom,
            scroll,
            opener_url: args.opener_url,
        })
        .map(|_| ())
    })
    .await?
}

#[tauri::command]
pub async fn tab_close(app: AppHandle, id: String) -> CmdResult<()> {
    on_main(&app, move || tab::close(&id)).await
}

#[tauri::command]
pub async fn tab_navigate(app: AppHandle, id: String, url: String) -> CmdResult<()> {
    let url = urls::sanitize_navigation(&url).ok_or("this address cannot be opened")?;
    on_main(&app, move || tab::navigate(&id, &url)).await?
}

#[tauri::command]
pub async fn tab_command(
    app: AppHandle,
    id: String,
    cmd: String,
    arg: Option<serde_json::Value>,
) -> CmdResult<serde_json::Value> {
    if page_tools::handles(&cmd) {
        return on_main_cb(&app, move |done| page_tools::run(&id, &cmd, arg, done)).await?;
    }
    on_main(&app, move || tab::command(&id, &cmd, arg)).await?
}

#[derive(Serialize)]
pub struct Scroll {
    x: f64,
    y: f64,
}

/// Frees a tab's webview and returns its scroll position for restoring later.
#[tauri::command]
pub async fn tab_sleep(app: AppHandle, id: String) -> CmdResult<Option<Scroll>> {
    let scroll = on_main_cb(&app, move |done| tab::sleep(&id, move |s| done(s))).await?;
    Ok(scroll.map(|(x, y)| Scroll { x, y }))
}

/// The natural size of a page's content (sizes extension popups to fit, like Chrome).
#[tauri::command]
pub async fn tab_content_size(app: AppHandle, id: String) -> CmdResult<Option<Scroll>> {
    let size = on_main_cb(&app, move |done| tab::content_size(&id, move |s| done(s))).await?;
    Ok(size.map(|(x, y)| Scroll { x, y }))
}

#[tauri::command]
pub async fn tab_set_glance(app: AppHandle, id: String, glance: bool) -> CmdResult<()> {
    on_main(&app, move || tab::set_glance(&id, glance)).await
}

#[tauri::command]
pub async fn set_layout(app: AppHandle, window: WebviewWindow, spec: LayoutSpec) -> CmdResult<()> {
    let label = window.label().to_string();
    on_main(&app, move || host::set_layout(&label, spec)).await
}

#[tauri::command]
pub async fn focus_view(app: AppHandle, window: WebviewWindow, target: Option<String>) -> CmdResult<()> {
    let label = window.label().to_string();
    on_main(&app, move || host::focus_view(&label, target)).await
}

#[tauri::command]
pub async fn find_in_page(
    app: AppHandle,
    id: String,
    op: String,
    text: Option<String>,
    case_sensitive: Option<bool>,
    whole_word: Option<bool>,
) -> CmdResult<()> {
    on_main(&app, move || {
        tab::find(
            &id,
            &op,
            text.as_deref().unwrap_or(""),
            case_sensitive.unwrap_or(false),
            whole_word.unwrap_or(false),
        )
    })
    .await?
}

#[tauri::command]
pub async fn permission_respond(app: AppHandle, request_id: u32, allow: bool, remember: bool) -> CmdResult<()> {
    on_main(&app, move || tab::respond_permission(request_id, allow, remember)).await
}

#[tauri::command]
pub async fn clear_browsing_data(
    app: AppHandle,
    window: WebviewWindow,
    kinds: Vec<String>,
    since_ms: Option<i64>,
    history: bool,
) -> CmdResult<()> {
    let private = private(&window);
    if history && !private {
        db().call(move |c| crate::history::history_clear(c, since_ms)).await?;
    }
    if kinds.is_empty() {
        return Ok(());
    }
    on_main_cb(&app, move |done| host::clear_browsing_data(private, &kinds, since_ms, move |r| done(r))).await?
}

// ---------------------------------------------------------------------------------------------
// Bookmarks. The tree itself is in bookmarks.rs. Every window can change it, incognito ones
// included, as in other browsers; the other windows hear about a change through
// `bookmarks-changed` and reload.

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BookmarksChanged {
    /// Window that made the change; it already has the new state.
    origin: String,
}

fn bookmarks_changed(window: &WebviewWindow) {
    host::emit("bookmarks-changed", BookmarksChanged { origin: window.label().to_string() });
}

#[tauri::command]
pub async fn bookmarks_list() -> CmdResult<Vec<bookmarks::Node>> {
    bookmarks_db().call(|c| bookmarks::list(c)).await
}

#[tauri::command]
pub async fn bookmarks_add(window: WebviewWindow, parent: i64, index: Option<i64>, nodes: Vec<bookmarks::NewNode>) -> CmdResult<Vec<i64>> {
    let ids = bookmarks_db().call(move |c| bookmarks::add(c, parent, index, &nodes)).await?;
    bookmarks_changed(&window);
    Ok(ids)
}

#[tauri::command]
pub async fn bookmark_update(window: WebviewWindow, id: i64, patch: bookmarks::NodePatch) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::update(c, id, &patch)).await?;
    bookmarks_changed(&window);
    Ok(())
}

#[tauri::command]
pub async fn bookmark_move(window: WebviewWindow, id: i64, parent: i64, index: i64) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::move_node(c, id, parent, index)).await?;
    bookmarks_changed(&window);
    Ok(())
}

#[tauri::command]
pub async fn bookmarks_reorder(window: WebviewWindow, parent: i64, order: Vec<i64>) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::reorder(c, parent, &order)).await?;
    bookmarks_changed(&window);
    Ok(())
}

#[tauri::command]
pub async fn bookmarks_remove(window: WebviewWindow, ids: Vec<i64>) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::remove(c, &ids)).await?;
    bookmarks_changed(&window);
    Ok(())
}

#[tauri::command]
pub async fn bookmarks_remove_url(window: WebviewWindow, url: String) -> CmdResult<()> {
    bookmarks_db().call(move |c| bookmarks::remove_url(c, &url)).await?;
    bookmarks_changed(&window);
    Ok(())
}

/// Largest bookmarks file read (an export or a backup is far smaller).
const MAX_BOOKMARK_FILE: u64 = 64 * 1024 * 1024;

fn read_bookmark_file(path: PathBuf) -> Result<String, String> {
    let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if len > MAX_BOOKMARK_FILE {
        return Err("That file is too large to be a bookmarks file.".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    // Browsers write UTF-8; anything else is read as well as it can be.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Where automatic backups and the copies made before a restore are kept.
fn bookmark_backups_dir() -> PathBuf {
    paths().roaming.join("bookmarkbackups")
}

/// Saves the bookmarks as a Netscape HTML file that Firefox, Chrome and Edge can import.
#[tauri::command]
pub async fn bookmarks_export(app: AppHandle) -> CmdResult<Option<String>> {
    let html = bookmarks_db().call(|c| bookmarks::export_html(c)).await?;
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Export bookmarks")
            .set_file_name("zenith-bookmarks.html")
            .add_filter("Bookmarks (HTML)", &["html", "htm"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        storage::write_atomic(&path, html.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await?
}

/// Adds the bookmarks of a Netscape HTML file; returns how many are new.
#[tauri::command]
pub async fn bookmarks_import(window: WebviewWindow, app: AppHandle) -> CmdResult<usize> {
    let html = blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Import bookmarks")
            .add_filter("Bookmarks (HTML)", &["html", "htm"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        read_bookmark_file(path.into_path().map_err(|e| e.to_string())?).map(Some)
    })
    .await??;
    let Some(html) = html else { return Ok(0) };
    let added = bookmarks_db().call(move |c| bookmarks::import_html(c, &html)).await?;
    if added > 0 {
        bookmarks_changed(&window);
    }
    Ok(added)
}

/// Saves every bookmark, folder, tag and keyword as a JSON backup file.
#[tauri::command]
pub async fn bookmarks_backup(app: AppHandle) -> CmdResult<Option<String>> {
    let backup = bookmarks_db().call(|c| bookmarks::backup_json(c)).await?;
    let name = format!("zenith-bookmarks-{}.json", bookmarks::date_string(storage::now_ms()));
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Back up bookmarks")
            .set_file_name(name)
            .add_filter("Bookmarks backup (JSON)", &["json"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        storage::write_atomic(&path, backup.json.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await?
}

/// Replaces every bookmark with the ones in a backup file; None when the dialog is cancelled.
/// A copy of the current bookmarks is kept first.
#[tauri::command]
pub async fn bookmarks_restore(window: WebviewWindow, app: AppHandle) -> CmdResult<Option<usize>> {
    if private(&window) {
        return Err("Incognito windows can't restore bookmarks.".into());
    }
    let json = blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Restore bookmarks")
            .add_filter("Bookmarks backup (JSON)", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        read_bookmark_file(path.into_path().map_err(|e| e.to_string())?).map(Some)
    })
    .await??;
    let Some(json) = json else { return Ok(None) };
    let dir = bookmark_backups_dir();
    let restored = bookmarks_db().call(move |c| bookmarks::restore(c, Some(&dir), &json, storage::now_ms())).await?;
    bookmarks_changed(&window);
    Ok(Some(restored))
}

/// The automatic backups and pre-restore copies, newest first.
#[tauri::command]
pub async fn bookmark_backups() -> CmdResult<Vec<bookmarks::BackupInfo>> {
    let dir = bookmark_backups_dir();
    blocking(move || bookmarks::list_backups(&dir)).await
}

/// Restores one of those backups by name (never by path).
#[tauri::command]
pub async fn bookmarks_restore_backup(window: WebviewWindow, name: String) -> CmdResult<usize> {
    if private(&window) {
        return Err("Incognito windows can't restore bookmarks.".into());
    }
    let dir = bookmark_backups_dir();
    let json = {
        let dir = dir.clone();
        blocking(move || bookmarks::read_backup(&dir, &name)).await??
    };
    let restored = bookmarks_db().call(move |c| bookmarks::restore(c, Some(&dir), &json, storage::now_ms())).await?;
    bookmarks_changed(&window);
    Ok(restored)
}

/// The icon of a bookmarked site, as a data URL: its `/favicon.ico`, fetched once for bookmarks that
/// came from an import (or were made before the site was visited). Private network addresses
/// are never asked. None when the site has none.
#[tauri::command]
pub async fn favicon_fetch(url: String) -> CmdResult<Option<String>> {
    let Some(icon) = bookmarks::favicon_url(&url) else { return Ok(None) };
    // A redirect could lead to a private address that the check above never saw: none are followed.
    blocking(move || {
        net::get_no_redirects(&icon, bookmarks::FAVICON_MAX_BYTES, 5_000).ok().and_then(|b| bookmarks::favicon_data_url(&b))
    })
    .await
}

/// Writes today's automatic backup unless there is one already (the main window calls this once
/// at startup).
#[tauri::command]
pub async fn bookmarks_auto_backup(window: WebviewWindow) -> CmdResult<()> {
    if private(&window) {
        return Ok(());
    }
    let dir = bookmark_backups_dir();
    bookmarks_db().call(move |c| bookmarks::auto_backup(c, &dir, storage::now_ms())).await
}

/// Browser profiles on this PC with bookmarks or history to import.
#[tauri::command]
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

/// Imports bookmarks and/or history from a profile found by `import_sources`. History older
/// than `history_days` (0: any age) is left out, since retention would drop it anyway.
#[tauri::command]
pub async fn import_run(
    window: WebviewWindow,
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

// ---------------------------------------------------------------------------------------------
// Media downloader

/// Videos and audio found in a tab. With `scan` (default), the page is searched first.
#[tauri::command]
pub async fn media_list(app: AppHandle, tab_id: String, scan: Option<bool>) -> CmdResult<crate::media::Report> {
    on_main_cb(&app, move |done| crate::media::list(&tab_id, scan.unwrap_or(true), move |r| done(r))).await?
}

/// Starts downloading a listed item; returns the download id.
#[tauri::command]
pub async fn media_download(
    app: AppHandle,
    tab_id: String,
    item_id: u32,
    variant: Option<String>,
    audio: Option<String>,
) -> CmdResult<i64> {
    on_main(&app, move || crate::media::download(&tab_id, item_id, variant, audio)).await?
}

#[tauri::command]
pub async fn media_tools() -> CmdResult<crate::media::Tools> {
    blocking(crate::media::tools).await
}

/// Lets the user point Zenith at ffmpeg.exe.
#[tauri::command]
pub async fn media_pick_ffmpeg(app: AppHandle) -> CmdResult<Option<String>> {
    blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose ffmpeg.exe")
            .add_filter("FFmpeg", &["exe"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
}

// ---------------------------------------------------------------------------------------------
// Ad and tracker blocking

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FilterList {
    id: String,
    url: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ListStatus {
    id: String,
    rules: usize,
    updated: Option<i64>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockerStats {
    rules: usize,
    total_blocked: u64,
    lists: Vec<ListStatus>,
}

fn list_file(id: &str) -> PathBuf {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    // Filter lists are cached in the user's profile, incognito windows included.
    host::persistent_paths().filters().join(format!("{safe}.txt"))
}

/// Rebuilds the engine from the built-in list plus the given subscriptions. With `update`, lists
/// are downloaded again; otherwise cached copies are used (downloaded when missing).
#[tauri::command]
pub async fn blocker_configure(lists: Vec<FilterList>, update: bool, disabled_sites: Vec<String>) -> CmdResult<BlockerStats> {
    blocking(move || {
        let mut engine = blocker::Engine::with_builtin();
        let mut statuses = Vec::new();
        for list in &lists {
            let file = list_file(&list.id);
            let mut error = None;
            if update || !file.exists() {
                match net::get(&list.url, 16 * 1024 * 1024, 20_000) {
                    Ok(body) => {
                        let _ = std::fs::create_dir_all(host::persistent_paths().filters());
                        if let Err(e) = storage::write_atomic(&file, &body) {
                            error = Some(e.to_string());
                        }
                    }
                    Err(e) => error = Some(e),
                }
            }
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let rules = if text.is_empty() { 0 } else { engine.add_list(&text) };
            let updated = std::fs::metadata(&file)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64);
            statuses.push(ListStatus { id: list.id.clone(), rules, updated, error });
        }
        let rules = engine.rule_count();
        if let Ok(mut state) = BLOCKER.write() {
            state.engine = engine;
            state.disabled_sites = disabled_sites.into_iter().map(|s| s.to_ascii_lowercase()).collect();
        }
        BlockerStats {
            rules,
            total_blocked: tab::TOTAL_BLOCKED.load(std::sync::atomic::Ordering::Relaxed),
            lists: statuses,
        }
    })
    .await
}

#[tauri::command]
pub async fn blocker_set_disabled_sites(sites: Vec<String>) -> CmdResult<()> {
    if let Ok(mut state) = BLOCKER.write() {
        state.disabled_sites = sites.into_iter().map(|s| s.to_ascii_lowercase()).collect();
    }
    Ok(())
}

#[tauri::command]
pub async fn blocker_stats() -> CmdResult<BlockerStats> {
    let rules = BLOCKER.read().map(|b| b.engine.rule_count()).unwrap_or(0);
    Ok(BlockerStats {
        rules,
        total_blocked: tab::TOTAL_BLOCKED.load(std::sync::atomic::Ordering::Relaxed),
        lists: Vec::new(),
    })
}

// ---------------------------------------------------------------------------------------------
// Extensions

#[tauri::command]
pub async fn ext_list(app: AppHandle) -> CmdResult<Vec<ExtensionInfo>> {
    on_main_cb(&app, |done| extensions::list(move |r| done(r))).await?
}

async fn install_staged(app: &AppHandle, dir: PathBuf, store_id: Option<String>) -> CmdResult<ExtensionInfo> {
    on_main_cb(app, move |done| extensions::install_folder(dir, store_id, move |r| done(r))).await?
}

#[tauri::command]
pub async fn ext_install_folder(app: AppHandle, window: WebviewWindow, path: Option<String>) -> CmdResult<Option<ExtensionInfo>> {
    refuse_in_incognito(&window, "Installing extensions")?;
    let picked = match path {
        Some(p) => Some(PathBuf::from(p)),
        None => {
            let app2 = app.clone();
            blocking(move || {
                app2.dialog()
                    .file()
                    .set_title("Choose an unpacked extension folder (with manifest.json)")
                    .blocking_pick_folder()
                    .and_then(|p| p.into_path().ok())
            })
            .await?
        }
    };
    let Some(source) = picked else { return Ok(None) };
    let staged = blocking(move || extensions::stage_folder(&source)).await??;
    install_staged(&app, staged, None).await.map(Some)
}

#[tauri::command]
pub async fn ext_install_crx(app: AppHandle, window: WebviewWindow) -> CmdResult<Option<ExtensionInfo>> {
    refuse_in_incognito(&window, "Installing extensions")?;
    let app2 = app.clone();
    let picked = blocking(move || {
        app2.dialog()
            .file()
            .set_title("Choose a Chrome extension package")
            .add_filter("Chrome extension", &["crx"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
    })
    .await?;
    let Some(file) = picked else { return Ok(None) };
    // Store packages saved by other browsers or mirror sites are named after their id.
    let hint = file.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let staged = blocking(move || {
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        extensions::stage_crx(&bytes)
    })
    .await??;
    let store_id = staged.crx_id.filter(|id| crx::store_id(&hint).as_ref() == Some(id));
    install_staged(&app, staged.dir, store_id).await.map(Some)
}

/// Installs from a Chrome Web Store link or extension id.
#[tauri::command]
pub async fn ext_install_store(app: AppHandle, window: WebviewWindow, input: String) -> CmdResult<ExtensionInfo> {
    refuse_in_incognito(&window, "Installing extensions")?;
    let id = crx::store_id(&input).ok_or("paste a Chrome Web Store link or a 32-letter extension id")?;
    install_from_store(&app, id).await
}

async fn install_from_store(app: &AppHandle, id: String) -> CmdResult<ExtensionInfo> {
    let store_id = id.clone();
    let staged = blocking(move || extensions::stage_crx(&extensions::download_from_store(&id)?)).await??;
    install_staged(app, staged.dir, Some(store_id)).await
}

/// Installs an extension the user accepted in the install prompt.
#[tauri::command]
pub async fn ext_offer_accept(app: AppHandle, window: WebviewWindow, token: u64) -> CmdResult<ExtensionInfo> {
    if private(&window) {
        extensions::discard(token);
        return Err("Installing extensions is not available in an incognito window".into());
    }
    on_main_cb(&app, move |done| extensions::accept(token, move |r| done(r))).await?
}

#[tauri::command]
pub async fn ext_offer_discard(token: u64) -> CmdResult<()> {
    blocking(move || extensions::discard(token)).await
}

/// Newer versions of the extensions installed from the Chrome Web Store.
#[tauri::command]
pub async fn ext_check_updates() -> CmdResult<Vec<extensions::ExtensionUpdate>> {
    blocking(extensions::check_updates).await?
}

/// Reinstalls a store extension from the Web Store's current version.
#[tauri::command]
pub async fn ext_update(app: AppHandle, window: WebviewWindow, id: String) -> CmdResult<ExtensionInfo> {
    refuse_in_incognito(&window, "Updating extensions")?;
    let store_id = extensions::store_source(&id).ok_or("this extension was not installed from the Chrome Web Store")?;
    install_from_store(&app, store_id).await
}

#[tauri::command]
pub async fn ext_remove(app: AppHandle, id: String) -> CmdResult<()> {
    on_main_cb(&app, move |done| extensions::remove(id, move |r| done(r))).await?
}

#[tauri::command]
pub async fn ext_set_enabled(app: AppHandle, id: String, enabled: bool) -> CmdResult<()> {
    on_main_cb(&app, move |done| extensions::set_enabled(id, enabled, move |r| done(r))).await?
}

// ---------------------------------------------------------------------------------------------
// Platform

#[tauri::command]
pub async fn debug_cdp(app: AppHandle, method: String, params: Option<String>) -> CmdResult<String> {
    let params = params.unwrap_or_else(|| "{}".into());
    on_main_cb(&app, move |done| host::chrome_cdp(&method, &params, move |r| done(r))).await?
}

#[tauri::command]
pub async fn debug_layout(app: AppHandle, window: WebviewWindow) -> CmdResult<Option<host::DebugLayout>> {
    let label = window.label().to_string();
    on_main(&app, move || host::debug_layout(&label)).await
}

// ---------------------------------------------------------------------------------------------
// Updates

#[tauri::command]
pub async fn update_status() -> CmdResult<crate::updates::Status> {
    Ok(crate::updates::status())
}

/// Checks now (and downloads a newer version in the background).
#[tauri::command]
pub async fn update_check(app: AppHandle, window: WebviewWindow) -> CmdResult<crate::updates::Status> {
    // Updating restarts Zenith, which would end the incognito session.
    refuse_in_incognito(&window, "Updating")?;
    Ok(crate::updates::check(app).await)
}

/// Quits into the downloaded update; the installer reopens Zenith. Call after the session is saved.
#[tauri::command]
pub async fn update_restart(app: AppHandle, window: WebviewWindow) -> CmdResult<()> {
    refuse_in_incognito(&window, "Updating")?;
    if host::window_labels().iter().any(|l| host::is_private_window(l)) {
        return Err("close the incognito windows first: restarting would end their session".into());
    }
    if !crate::updates::ready() {
        return Err("no update has been downloaded yet".into());
    }
    // The installer closes every Zenith process; other profiles would lose their unsaved state.
    if crate::profiles::others_running() {
        return Err("close your other Zenith profiles first: the update restarts all of them".into());
    }
    crate::updates::restart_after_quit();
    crate::app::request_quit(&app);
    Ok(())
}
