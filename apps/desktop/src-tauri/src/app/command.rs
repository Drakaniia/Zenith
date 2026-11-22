//! Shared command helpers: result type, CEF UI-thread dispatch, blocking worker dispatch, and
//! incognito window guards.

use super::runtime::{spawn_blocking, AppHandle, WindowHandle};
use crate::host;

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) async fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })?;
    rx.await.map_err(|_| "the UI thread did not respond".to_string())
}

/// Like `on_main`, for CEF calls that complete through a callback.
pub(crate) async fn on_main_cb<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(Box<dyn FnOnce(T)>) + Send + 'static,
) -> CmdResult<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        f(Box::new(move |v| {
            let _ = tx.send(v);
        }))
    })?;
    rx.await.map_err(|_| "the operation was dropped".to_string())
}

pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    spawn_blocking(f).await.map_err(|e| e.to_string())
}

pub(crate) fn private(window: &WindowHandle) -> bool {
    host::is_private_window(window.label())
}

pub(crate) fn refuse_in_incognito(window: &WindowHandle, what: &str) -> CmdResult<()> {
    if private(window) {
        return Err(format!("{what} is not available in an incognito window"));
    }
    Ok(())
}
