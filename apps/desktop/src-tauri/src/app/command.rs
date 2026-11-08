//! What every command shares: the result type the UI receives (errors as plain strings),
//! helpers that run work on the UI thread or on a blocking worker thread, and the checks for
//! commands an incognito window may not use.

use tauri::{AppHandle, WebviewWindow};

use crate::host;

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) async fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|_| "the UI thread did not respond".to_string())
}

/// Like `on_main`, for WebView2 calls that complete through a callback.
pub(crate) async fn on_main_cb<T: Send + 'static>(
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

pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

pub(crate) fn private(window: &WebviewWindow) -> bool {
    host::is_private_window(window.label())
}

pub(crate) fn refuse_in_incognito(window: &WebviewWindow, what: &str) -> CmdResult<()> {
    if private(window) {
        return Err(format!("{what} is not available in an incognito window"));
    }
    Ok(())
}
