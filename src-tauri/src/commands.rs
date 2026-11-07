//! Tauri commands for the chrome UI. Only the windows' local chrome webviews (`main`,
//! `incognito-N`) can call these; page webviews are raw WebView2 views without the Tauri bridge.
//! Commands that act on one window take the calling `WebviewWindow`.
//!
//! WebView2 objects are single-threaded, so webview work hops to the UI thread with
//! `run_on_main_thread`; disk and network work runs on blocking worker threads.

use std::path::PathBuf;
use std::sync::Mutex;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::host::downloads::{self, DownloadEvent};
use crate::host::crx;
use crate::host::extensions::{self, ExtensionInfo};
use crate::host::tab::{self, CreateOptions};
use crate::host::{self, bookmarks_db, db, paths, LayoutSpec, RuntimeConfig, BLOCKER};
use crate::keys::BindingSpec;
use crate::store::{self, Bookmark, DownloadRecord, HistoryEntry, Note};
use crate::{blocker, importer, net, urls, win};

type CmdResult<T> = Result<T, String>;

async fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|_| "the UI thread did not respond".to_string())
}

/// Like `on_main`, for WebView2 calls that complete through a callback.
async fn on_main_cb<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(Box<dyn FnOnce(T)>) + Send + 'static,
) -> CmdResult<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        f(Box::new(move |v| {
            let _ = tx.send(v);
        }))
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|_| "the operation was dropped".to_string())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------
// Startup and window

/// Addresses each window opens once its UI is ready, by window label.
static STARTUP_URLS: Mutex<Option<HashMap<String, Vec<String>>>> = Mutex::new(None);

pub fn set_startup_urls(label: &str, urls: Vec<String>) {
    if let Ok(mut s) = STARTUP_URLS.lock() {
        s.get_or_insert_with(HashMap::new).insert(label.to_string(), urls);
    }
}

fn take_startup_urls(label: &str) -> Vec<String> {
    STARTUP_URLS
        .lock()
        .ok()
        .and_then(|mut s| s.as_mut().and_then(|m| m.remove(label)))
        .unwrap_or_default()
}

fn private(window: &WebviewWindow) -> bool {
    host::is_private_window(window.label())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupInfo {
    /// Running on a separate profile (ZENITH_PROFILE_DIR), e.g. an automated test instance.
    test_profile: bool,
    /// An incognito window (InPrivate pages, nothing saved).
    incognito: bool,
    startup_urls: Vec<String>,
    windows11: bool,
    webview_version: String,
    version: String,
    data_dir: String,
    default_download_dir: String,
}

#[tauri::command]
pub async fn ui_ready(app: AppHandle, window: WebviewWindow) -> CmdResult<StartupInfo> {
    let incognito = private(&window);
    let label = window.label().to_string();
    let test_profile = host::profile_override().is_some() && !incognito;
    let shown = window.clone();
    on_main(&app, move || {
        // Shown outside the host borrow: showing the window sends WM_SIZE, which relayouts tabs.
        let _ = shown.show();
        if shown.label() == host::MAIN && crate::RESTORE_MAXIMIZED.swap(false, std::sync::atomic::Ordering::SeqCst) {
            let _ = shown.maximize();
        }
        // A separate-profile (test) instance must not steal focus from the user's window.
        if !test_profile {
            let _ = shown.set_focus();
        }
    })
    .await?;
    // Time from process start until the chrome UI is shown (read by scripts/measure.ps1).
    if std::env::var_os("ZENITH_PERF").is_some() {
        if let Some(started) = host::STARTED.get() {
            let json = format!("{{\"startupMs\":{}}}", started.elapsed().as_millis());
            let _ = store::write_atomic(&paths().local.join("perf.json"), json.as_bytes());
        }
    }
    let startup_urls = take_startup_urls(&label);
    Ok(StartupInfo {
        test_profile,
        incognito,
        startup_urls,
        windows11: win::is_windows_11(),
        webview_version: wry::webview_version().unwrap_or_default(),
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: paths().roaming.display().to_string(),
        default_download_dir: app
            .path()
            .download_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn app_quit(app: AppHandle) -> CmdResult<()> {
    crate::request_quit(&app);
    Ok(())
}

/// Closes the calling window (after its UI saved what it keeps). Closing the last window quits.
#[tauri::command]
pub async fn window_close(app: AppHandle, window: WebviewWindow) -> CmdResult<()> {
    crate::close_window(&app, window.label());
    Ok(())
}

/// Opens a new incognito window in this process, optionally with web addresses to load.
#[tauri::command]
pub async fn incognito_open(urls: Option<Vec<String>>) -> CmdResult<()> {
    let urls: Vec<String> = urls
        .unwrap_or_default()
        .iter()
        .filter_map(|u| urls::sanitize_navigation(u))
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        .collect();
    crate::incognito::open_window(urls);
    Ok(())
}

fn refuse_in_incognito(window: &WebviewWindow, what: &str) -> CmdResult<()> {
    if private(window) {
        return Err(format!("{what} is not available in an incognito window"));
    }
    Ok(())
}

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

// ---------------------------------------------------------------------------------------------
// Settings pushed to the backend

/// Returns the system-wide shortcuts that could not be registered (another app owns them).
#[tauri::command]
pub async fn set_shortcuts(app: AppHandle, bindings: Vec<BindingSpec>) -> CmdResult<Vec<String>> {
    on_main(&app, move || host::set_bindings(bindings)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    /// "system" | "light" | "dark"
    theme: String,
    /// "mica" | "mica-alt" | "acrylic" | "solid"
    material: String,
    page_background: [u8; 3],
    /// prefers-color-scheme for web content: "system" | "light" | "dark"
    content_scheme: String,
}

#[derive(Serialize)]
pub struct AppliedSettings {
    /// The material actually in use (Windows 10 always gets "solid").
    material: String,
}

#[tauri::command]
pub async fn apply_settings(
    app: AppHandle,
    window: WebviewWindow,
    config: RuntimeConfig,
    appearance: Appearance,
) -> CmdResult<AppliedSettings> {
    // App-wide settings come from the normal window; an incognito window only styles itself
    // (its settings changes last only as long as it is open).
    if !private(&window) {
        host::apply_config(config);
    }
    let material = if win::is_windows_11() { appearance.material.clone() } else { "solid".to_string() };
    let theme = match appearance.theme.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    };
    let _ = window.set_theme(theme);
    let effect = match material.as_str() {
        "mica" => Some(tauri::window::Effect::Mica),
        "mica-alt" => Some(tauri::window::Effect::Tabbed),
        "acrylic" => Some(tauri::window::Effect::Acrylic),
        _ => None,
    };
    let effects = effect.map(|e| tauri::utils::config::WindowEffectsConfig {
        effects: vec![e],
        ..Default::default()
    });
    let _ = window.set_effects(effects);
    let bg = appearance.page_background;
    let scheme = appearance.content_scheme;
    on_main(&app, move || {
        host::set_page_background((bg[0], bg[1], bg[2]));
        host::set_content_color_scheme(&scheme);
    })
    .await?;
    Ok(AppliedSettings { material })
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
        db().call(move |c| store::history_clear(c, since_ms)).await?;
    }
    if kinds.is_empty() {
        return Ok(());
    }
    on_main_cb(&app, move |done| host::clear_browsing_data(private, &kinds, since_ms, move |r| done(r))).await?
}

// ---------------------------------------------------------------------------------------------
// Persistence

static SESSION_LOCK: Mutex<()> = Mutex::new(());

/// Incognito windows always start empty and never write a session.
#[tauri::command]
pub async fn session_load(window: WebviewWindow) -> CmdResult<Option<String>> {
    if private(&window) {
        return Ok(None);
    }
    blocking(|| store::read_text(&paths().session())).await
}

#[tauri::command]
pub async fn session_save(window: WebviewWindow, data: String) -> CmdResult<()> {
    if private(&window) {
        return Ok(());
    }
    blocking(move || {
        let _guard = SESSION_LOCK.lock();
        store::write_atomic(&paths().session(), data.as_bytes()).map_err(|e| e.to_string())
    })
    .await?
}

#[tauri::command]
pub async fn settings_load() -> CmdResult<Option<String>> {
    blocking(|| store::read_text(&paths().settings())).await
}

/// Incognito windows start from the user's settings; their own changes are not saved.
#[tauri::command]
pub async fn settings_save(window: WebviewWindow, data: String) -> CmdResult<()> {
    if private(&window) {
        return Ok(());
    }
    blocking(move || store::write_atomic(&paths().settings(), data.as_bytes()).map_err(|e| e.to_string())).await?
}

#[tauri::command]
pub async fn history_search(text: String, limit: Option<i64>, offset: Option<i64>) -> CmdResult<Vec<HistoryEntry>> {
    let (limit, offset) = (limit.unwrap_or(50).clamp(1, 500), offset.unwrap_or(0).max(0));
    db().call(move |c| store::history_search(c, &text, limit, offset)).await
}

#[tauri::command]
pub async fn history_top(limit: Option<i64>) -> CmdResult<Vec<HistoryEntry>> {
    let limit = limit.unwrap_or(40).clamp(1, 200);
    db().call(move |c| store::history_top(c, limit)).await
}

#[tauri::command]
pub async fn history_delete(ids: Vec<i64>) -> CmdResult<usize> {
    db().call(move |c| store::history_delete(c, &ids)).await
}

#[tauri::command]
pub async fn history_clear(since_ms: Option<i64>) -> CmdResult<usize> {
    db().call(move |c| store::history_clear(c, since_ms)).await
}

#[tauri::command]
pub async fn history_prune(days: i64) -> CmdResult<()> {
    db().call(move |c| store::history_prune(c, days, 100_000)).await
}

#[tauri::command]
pub async fn bookmarks_list() -> CmdResult<Vec<Bookmark>> {
    bookmarks_db().call(store::bookmarks_list).await
}

#[tauri::command]
pub async fn bookmark_add(bookmark: Bookmark) -> CmdResult<i64> {
    bookmarks_db().call(move |c| store::bookmark_add(c, &bookmark)).await
}

#[tauri::command]
pub async fn bookmark_update(bookmark: Bookmark) -> CmdResult<()> {
    bookmarks_db().call(move |c| store::bookmark_update(c, &bookmark)).await
}

#[tauri::command]
pub async fn bookmark_remove(id: Option<i64>, url: Option<String>) -> CmdResult<()> {
    bookmarks_db().call(move |c| match (id, url) {
        (Some(id), _) => store::bookmark_remove(c, id),
        (None, Some(url)) => store::bookmark_remove_url(c, &url),
        _ => Ok(()),
    })
    .await
}

// ---------------------------------------------------------------------------------------------
// Notes. Shared by every window, incognito ones included (notes are the user's own documents,
// not browsing data). Other windows hear about changes through `notes-changed`.

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NotesChanged {
    /// Window that made the change; it already has the new state.
    origin: String,
    id: String,
    deleted: bool,
}

#[tauri::command]
pub async fn notes_list() -> CmdResult<Vec<Note>> {
    bookmarks_db().call(store::notes_list).await
}

#[tauri::command]
pub async fn note_save(window: WebviewWindow, note: Note) -> CmdResult<bool> {
    if note.id.is_empty() || note.id.len() > 64 {
        return Err("invalid note id".into());
    }
    let id = note.id.clone();
    let written = bookmarks_db().call(move |c| store::note_save(c, &note)).await?;
    if written {
        host::emit("notes-changed", NotesChanged { origin: window.label().to_string(), id, deleted: false });
    }
    Ok(written)
}

#[tauri::command]
pub async fn note_delete(window: WebviewWindow, id: String) -> CmdResult<()> {
    let key = id.clone();
    bookmarks_db().call(move |c| store::note_delete(c, &key)).await?;
    host::emit("notes-changed", NotesChanged { origin: window.label().to_string(), id, deleted: true });
    Ok(())
}

#[tauri::command]
pub async fn bookmarks_export(app: AppHandle) -> CmdResult<Option<String>> {
    let items = bookmarks_db().call(store::bookmarks_list).await?;
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
        std::fs::write(&path, store::bookmarks_to_html(&items)).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await?
}

#[tauri::command]
pub async fn bookmarks_import(app: AppHandle) -> CmdResult<usize> {
    let parsed = blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Import bookmarks")
            .add_filter("Bookmarks (HTML)", &["html", "htm"])
            .blocking_pick_file()
        else {
            return Ok(Vec::new());
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        let html = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Ok::<_, String>(store::bookmarks_from_html(&html))
    })
    .await??;
    bookmarks_db().call(move |c| {
        let mut n = 0;
        for b in &parsed {
            store::bookmark_add(c, b)?;
            n += 1;
        }
        Ok(n)
    })
    .await
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
    let added = if marks.is_empty() { 0 } else { bookmarks_db().call(move |c| store::bookmarks_add_many(c, &marks)).await? };
    let visits: Vec<_> = data.history.into_iter().map(|v| (v.url, v.title, v.visit_count, v.last_visit)).collect();
    let merged = if visits.is_empty() { 0 } else { db().call(move |c| store::history_import(c, &visits)).await? };
    Ok(ImportResult { bookmarks: added, history: merged, errors: data.errors })
}

// ---------------------------------------------------------------------------------------------
// Downloads

#[tauri::command]
pub async fn downloads_active(app: AppHandle) -> CmdResult<Vec<DownloadEvent>> {
    on_main(&app, downloads::active).await
}

#[tauri::command]
pub async fn downloads_history(limit: Option<i64>) -> CmdResult<Vec<DownloadRecord>> {
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    db().call(move |c| store::downloads_list(c, limit)).await
}

#[tauri::command]
pub async fn download_action(app: AppHandle, id: i64, action: String) -> CmdResult<()> {
    on_main(&app, move || downloads::action(id, &action)).await?
}

#[tauri::command]
pub async fn downloads_remove(id: Option<i64>) -> CmdResult<()> {
    crate::media::job::forget(id);
    db().call(move |c| store::downloads_remove(c, id)).await
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

#[tauri::command]
pub async fn open_path(path: String) -> CmdResult<()> {
    if !std::path::Path::new(&path).exists() {
        return Err("the file no longer exists".into());
    }
    blocking(move || win::shell_open(&path)).await?
}

#[tauri::command]
pub async fn show_in_folder(path: String) -> CmdResult<()> {
    blocking(move || win::show_in_folder(&path)).await?
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle, title: Option<String>) -> CmdResult<Option<String>> {
    blocking(move || {
        let mut dialog = app.dialog().file();
        if let Some(t) = title {
            dialog = dialog.set_title(t);
        }
        dialog
            .blocking_pick_folder()
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
                        if let Err(e) = store::write_atomic(&file, &body) {
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
pub async fn default_browser_status() -> CmdResult<win::DefaultBrowserStatus> {
    blocking(win::default_browser_status).await
}

/// Registers Zenith with Windows and (by default) opens the Default apps page so the user can
/// pick it; Windows does not let apps make themselves the default silently.
#[tauri::command]
pub async fn default_browser_register(open_settings: Option<bool>) -> CmdResult<()> {
    blocking(move || {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        win::register_browser(&exe)?;
        if open_settings.unwrap_or(true) {
            win::open_default_apps_settings()?;
        }
        Ok(())
    })
    .await?
}

#[tauri::command]
pub async fn default_browser_unregister() -> CmdResult<()> {
    blocking(win::unregister_browser).await
}

#[tauri::command]
pub async fn search_suggest(engine: String, query: String) -> CmdResult<Vec<String>> {
    let query = query.trim().to_string();
    if query.is_empty() || query.len() > 200 {
        return Ok(Vec::new());
    }
    blocking(move || net::suggestions(&engine, &query)).await?
}

#[tauri::command]
pub async fn memory_stats() -> CmdResult<win::MemoryStats> {
    blocking(win::memory_stats).await
}

#[tauri::command]
pub async fn system_memory_load() -> CmdResult<u32> {
    Ok(win::system_memory_load())
}

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
    crate::updates::restart_after_quit();
    crate::request_quit(&app);
    Ok(())
}

#[tauri::command]
pub async fn clipboard_write(text: String) -> CmdResult<()> {
    blocking(move || win::set_clipboard_text(&text)).await?
}
