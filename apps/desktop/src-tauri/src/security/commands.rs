//! Commands for security prompts (certificate errors, external protocols, dangerous downloads)
//! and local Safe Browsing status/updates.

use crate::app::command::{blocking, on_main, CmdResult};
use crate::app::runtime::AppHandle;
use crate::host::with_host;
use crate::security::{SafeBrowsingStatus, SAFE_BROWSING, THREAT_FEEDS};

pub async fn security_respond_cert(app: AppHandle, request_id: u32, proceed: bool) -> CmdResult<()> {
    on_main(&app, move || {
        let pending = with_host(|h| h.cert_errors.remove(&request_id)).flatten();
        if let Some(p) = pending {
            p.respond(proceed);
        }
    })
    .await
}

pub async fn security_respond_protocol(app: AppHandle, request_id: u32, allow: bool) -> CmdResult<()> {
    let url = on_main(&app, move || {
        with_host(|h| h.protocol_prompts.remove(&request_id)).flatten()
    })
    .await?
    .map(|p| p.url);
    if allow {
        if let Some(u) = url {
            blocking(move || crate::platform::win::shell_open(&u)).await??;
        }
    }
    Ok(())
}

pub async fn security_respond_download(app: AppHandle, request_id: u32, allow: bool) -> CmdResult<()> {
    on_main(&app, move || {
        let pending = with_host(|h| h.pending_downloads.remove(&request_id)).flatten();
        if let Some(p) = pending {
            p.respond(allow);
        }
    })
    .await
}

pub async fn security_safebrowsing_status() -> CmdResult<SafeBrowsingStatus> {
    let sb = SAFE_BROWSING.read().map_err(|_| "safe browsing lock poisoned".to_string())?;
    Ok(SafeBrowsingStatus {
        enabled: sb.enabled,
        entries: sb.len(),
        updated: sb.updated,
        feeds: THREAT_FEEDS.iter().map(|(n, _)| (*n).to_string()).collect(),
    })
}

pub async fn security_safebrowsing_update() -> CmdResult<SafeBrowsingStatus> {
    blocking(crate::security::update_feeds).await?
}

pub async fn security_safebrowsing_bypass(app: AppHandle, tab_id: String, url: String) -> CmdResult<()> {
    if let Ok(mut sb) = SAFE_BROWSING.write() {
        sb.allow_bypass(&tab_id, &url);
    }
    on_main(&app, move || {
        let _ = crate::host::tab::navigate(&tab_id, &url);
    })
    .await
}
