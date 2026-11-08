//! Tauri commands for browser profiles (see `profiles`). Each profile runs in its own Zenith
//! process; these list and edit the shared profile list and start or focus other profiles.

use base64::Engine;
use serde::Serialize;
use tauri::image::Image;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::app::command::{blocking, CmdResult};
use crate::host;
use crate::profiles::{self, Look, Profile};

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
    /// `image`, `emoji:<name>`, or absent for the initial on the colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// The chosen picture as a `data:image/png` URL (only for the `image` avatar).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

impl From<&Profile> for ProfileInfo {
    fn from(p: &Profile) -> Self {
        let image = (p.avatar.as_deref() == Some("image"))
            .then(|| profiles::icon_png(&profiles::active().base, p))
            .flatten()
            .map(|png| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)));
        // An image whose file went missing shows the initial rather than nothing.
        let avatar = p.avatar.clone().filter(|a| a != "image" || image.is_some());
        Self { id: p.id.clone(), name: p.name.clone(), color: p.color.clone(), is_default: p.id == profiles::DEFAULT, avatar, image }
    }
}

pub fn current_info() -> ProfileInfo {
    ProfileInfo::from(&profiles::active().profile())
}

/// The window icon for this process's profile: its picture, or `None` for the app icon.
pub fn window_icon() -> Option<Image<'static>> {
    let active = profiles::active();
    let png = profiles::icon_png(&active.base, &active.profile())?;
    Image::from_bytes(&png).ok().map(|i| i.to_owned())
}

/// Puts the profile's picture (or the app icon) on this process's normal windows. Incognito
/// windows keep the app icon.
pub fn apply_window_icon(app: &AppHandle) {
    let icon = window_icon().or_else(|| app.default_window_icon().cloned().map(|i| i.to_owned()));
    let Some(icon) = icon else { return };
    for (label, w) in app.webview_windows() {
        if !host::is_private_window(&label) {
            let _ = w.set_icon(icon.clone());
        }
    }
}

/// The rendered picture the UI sends: base64 PNG, optionally as a `data:` URL.
fn decode_icon(icon: Option<String>) -> CmdResult<Option<Vec<u8>>> {
    let Some(icon) = icon else { return Ok(None) };
    let b64 = icon.split_once(";base64,").map_or(icon.as_str(), |(_, b)| b);
    base64::engine::general_purpose::STANDARD.decode(b64.trim()).map(Some).map_err(|_| "That picture can't be used.".into())
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
        let _ = w.set_title(&crate::app::window::window_title(host::is_private_window(&label)));
    }
}

#[tauri::command]
pub async fn profiles_list() -> CmdResult<ProfilesView> {
    blocking(view).await
}

/// Adds a profile; with `shortcut`, also puts a desktop shortcut for it on the desktop.
/// `avatar` and `icon` are the picture (see `profiles::Look`).
#[tauri::command]
pub async fn profile_create(
    window: WebviewWindow,
    name: String,
    color: String,
    avatar: Option<String>,
    icon: Option<String>,
    shortcut: bool,
) -> CmdResult<ProfileInfo> {
    refuse_in_incognito(&window)?;
    blocking(move || {
        let icon = decode_icon(icon)?;
        let look = Look { color: &color, avatar: avatar.as_deref(), icon: icon.as_deref() };
        let p = profiles::create(&profiles::active().base, &name, look)?;
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
pub async fn profile_update(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    name: String,
    color: String,
    avatar: Option<String>,
    icon: Option<String>,
) -> CmdResult<ProfileInfo> {
    refuse_in_incognito(&window)?;
    let (p, info) = blocking(move || {
        let icon = decode_icon(icon)?;
        let look = Look { color: &color, avatar: avatar.as_deref(), icon: icon.as_deref() };
        let p = profiles::update(&profiles::active().base, &id, &name, look)?;
        // The desktop shortcut follows the picture; the profile is saved either way.
        if let Err(e) = profiles::refresh_shortcut(&p.id) {
            eprintln!("[zenith] {e}");
        }
        let info = ProfileInfo::from(&p);
        Ok::<_, String>((p, info))
    })
    .await??;
    if p.id == profiles::active().id {
        retitle(&app);
        apply_window_icon(&app);
    }
    Ok(info)
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
