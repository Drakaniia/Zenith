//! Commands for app updates: the current state, checking now, and restarting into a downloaded
//! update.

use crate::app::runtime::{AppHandle, WindowHandle};

use crate::app::command::{refuse_in_incognito, CmdResult};
use crate::host;

pub async fn update_status() -> CmdResult<super::Status> {
    Ok(super::status())
}

/// Checks now (and downloads a newer version in the background).
pub async fn update_check(app: AppHandle, window: WindowHandle) -> CmdResult<super::Status> {
    // Updating restarts Zenith, which would end the incognito session.
    refuse_in_incognito(&window, "Updating")?;
    Ok(super::check(app).await)
}

/// Quits into the downloaded update; the installer reopens Zenith. Call after the session is saved.
pub async fn update_restart(app: AppHandle, window: WindowHandle) -> CmdResult<()> {
    refuse_in_incognito(&window, "Updating")?;
    if host::window_labels().iter().any(|l| host::is_private_window(l)) {
        return Err("close the incognito windows first: restarting would end their session".into());
    }
    if !super::ready() {
        return Err("no update has been downloaded yet".into());
    }
    // The installer closes every Zenith process; other profiles would lose their unsaved state.
    if crate::profiles::others_running() {
        return Err("close your other Zenith profiles first: the update restarts all of them".into());
    }
    super::restart_after_quit();
    crate::app::request_quit(&app);
    Ok(())
}
