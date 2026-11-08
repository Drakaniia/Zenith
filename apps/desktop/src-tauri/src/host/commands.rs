//! Commands for the tab host: tabs, the layout of the page views and focus, find in page,
//! permission prompts, clearing browsing data, and debugging aids.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, WebviewWindow};

use crate::app::command::{on_main, on_main_cb, private, CmdResult};
use crate::host::page_tools;
use crate::host::tab::{self, CreateOptions};
use crate::host::{self, db, LayoutSpec};
use crate::platform::urls;

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
// Debugging

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
