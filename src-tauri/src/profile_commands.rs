//! Tauri commands for browser profiles (see profiles.rs). Each profile runs in its own Zenith
//! process; these list and edit the shared profile list and start or focus other profiles.

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::host;
use crate::profiles::{self, Profile};

type CmdResult<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

/// Profile management changes data outside the window; incognito windows stay out of it.
fn refuse_in_incognito(window: &WebviewWindow) -> CmdResult<()> {
    if host::is_private_window(window.label()) {
        return Err("Profiles can't be managed from an incognito window.".into());
    }
    Ok(())
}

/// The running profile, as the UI shows it.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub color: String,
    pub is_default: bool,
}

impl From<&Profile> for ProfileInfo {
    fn from(p: &Profile) -> Self {
        Self { id: p.id.clone(), name: p.name.clone(), color: p.color.clone(), is_default: p.id == profiles::DEFAULT }
    }
}

pub fn current_info() -> ProfileInfo {
    ProfileInfo::from(&profiles::active().profile())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileEntry {
    #[serde(flatten)]
    info: ProfileInfo,
    /// Some Zenith process has the profile open (this one included).
    running: bool,
    /// A desktop shortcut Zenith created for it still exists.
    shortcut: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilesView {
    /// The profile of the calling window's process.
    active: String,
    profiles: Vec<ProfileEntry>,
    colors: Vec<&'static str>,
}

fn view() -> ProfilesView {
    let active = profiles::active();
    let list = profiles::load(&active.base);
    ProfilesView {
        active: active.id.clone(),
        profiles: list
            .profiles
            .iter()
            .map(|p| ProfileEntry {
                info: ProfileInfo::from(p),
                running: p.id == active.id || profiles::is_running(&active.base, &p.id),
                shortcut: p.shortcut.as_deref().is_some_and(|s| s.exists()),
            })
            .collect(),
        colors: profiles::COLORS.to_vec(),
    }
}

/// Retitles this process's windows after its profile was renamed.
fn retitle(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        let _ = w.set_title(&crate::window_title(host::is_private_window(&label)));
    }
}

#[tauri::command]
pub async fn profiles_list() -> CmdResult<ProfilesView> {
    blocking(view).await
}

/// Adds a profile; with `shortcut`, also puts a desktop shortcut for it on the desktop.
#[tauri::command]
pub async fn profile_create(window: WebviewWindow, name: String, color: String, shortcut: bool) -> CmdResult<ProfileInfo> {
    refuse_in_incognito(&window)?;
    blocking(move || {
        let p = profiles::create(&profiles::active().base, &name, &color)?;
        if shortcut {
            // The profile exists either way; a failed shortcut is reported but not fatal.
            if let Err(e) = profiles::create_shortcut(&p.id) {
                eprintln!("[zenith] {e}");
            }
        }
        Ok(ProfileInfo::from(&p))
    })
    .await?
}

#[tauri::command]
pub async fn profile_update(app: AppHandle, window: WebviewWindow, id: String, name: String, color: String) -> CmdResult<ProfileInfo> {
    refuse_in_incognito(&window)?;
    let p = blocking(move || profiles::update(&profiles::active().base, &id, &name, &color)).await??;
    if p.id == profiles::active().id {
        retitle(&app);
    }
    Ok(ProfileInfo::from(&p))
}

/// Opens a profile: focuses this process's window for its own profile, otherwise starts (or
/// focuses, through its single-instance redirect) the profile's process.
#[tauri::command]
pub async fn profile_open(app: AppHandle, window: WebviewWindow, id: String) -> CmdResult<()> {
    if id == profiles::active().id {
        let label = host::primary_window();
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        } else if !host::is_private_window(window.label()) {
            let _ = window.set_focus();
        }
        return Ok(());
    }
    blocking(move || profiles::launch(&id)).await?
}

/// Deletes a profile that is not open, with all of its data.
#[tauri::command]
pub async fn profile_delete(window: WebviewWindow, id: String) -> CmdResult<()> {
    refuse_in_incognito(&window)?;
    blocking(move || {
        let active = profiles::active();
        profiles::delete(&active.base, &id, &active.id)
    })
    .await?
}

/// Creates (or replaces) the profile's desktop shortcut; returns its path.
#[tauri::command]
pub async fn profile_shortcut(window: WebviewWindow, id: String) -> CmdResult<String> {
    refuse_in_incognito(&window)?;
    blocking(move || profiles::create_shortcut(&id).map(|p| p.display().to_string())).await?
}
