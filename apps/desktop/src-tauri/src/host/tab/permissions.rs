//! Site permission prompts (camera, microphone, location, notifications, clipboard, MIDI, etc.)
//! held via CEF `PermissionHandler` callbacks until the user answers in Zenith's chrome UI.
//! Dropping an unanswered `PendingPermission` denies it safely.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use cef::*;
use serde::Serialize;

use super::TabShared;
use crate::host::cef_app::opt_cef_str;
use crate::host::{emit_tab, with_host};

static NEXT_PERMISSION: AtomicU32 = AtomicU32::new(1);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PermissionEvent {
    request_id: u32,
    tab_id: String,
    origin: String,
    kind: String,
    user_initiated: bool,
}

enum PermCallback {
    Prompt(PermissionPromptCallback),
    Media(MediaAccessCallback, u32),
}

pub struct PendingPermission {
    callback: Option<PermCallback>,
    pub tab: String,
    /// Where a remembered decision is stored: the tab's own context, so incognito stays in memory.
    ctx: Option<RequestContext>,
    origin: String,
    settings: Vec<ContentSettingTypes>,
}

impl PendingPermission {
    pub fn answer(mut self, allow: bool, remember: bool) {
        if remember {
            if let Some(ctx) = &self.ctx {
                let origin = CefString::from(self.origin.as_str());
                let value = if allow { ContentSettingValues::ALLOW } else { ContentSettingValues::BLOCK };
                for t in &self.settings {
                    ctx.set_content_setting(Some(&origin), Some(&origin), t.clone(), value.clone());
                }
            }
        }
        if let Some(cb) = self.callback.take() {
            match cb {
                PermCallback::Prompt(p) => {
                    p.cont(if allow {
                        PermissionRequestResult::ACCEPT
                    } else {
                        PermissionRequestResult::DENY
                    });
                }
                PermCallback::Media(m, requested) => {
                    if allow {
                        m.cont(requested);
                    } else {
                        m.cancel();
                    }
                }
            }
        }
    }
}

impl Drop for PendingPermission {
    fn drop(&mut self) {
        if let Some(cb) = self.callback.take() {
            match cb {
                PermCallback::Prompt(p) => p.cont(PermissionRequestResult::DENY),
                PermCallback::Media(m, _) => m.cancel(),
            }
        }
    }
}

fn prompt_permission_name(mask: u32) -> Option<&'static str> {
    let has = |flag: PermissionRequestTypes| (mask & (flag.get_raw() as u32)) != 0;
    if has(PermissionRequestTypes::MIC_STREAM) {
        Some("microphone")
    } else if has(PermissionRequestTypes::CAMERA_STREAM) {
        Some("camera")
    } else if has(PermissionRequestTypes::GEOLOCATION) {
        Some("location")
    } else if has(PermissionRequestTypes::NOTIFICATIONS) {
        Some("notifications")
    } else if has(PermissionRequestTypes::CLIPBOARD) {
        Some("clipboard")
    } else if has(PermissionRequestTypes::MIDI_SYSEX) {
        Some("midi")
    } else if has(PermissionRequestTypes::WINDOW_MANAGEMENT) {
        Some("windowManagement")
    } else if has(PermissionRequestTypes::LOCAL_FONTS) {
        Some("localFonts")
    } else if has(PermissionRequestTypes::FILE_SYSTEM_ACCESS) {
        Some("files")
    } else if has(PermissionRequestTypes::MULTIPLE_DOWNLOADS) {
        Some("downloads")
    } else if has(PermissionRequestTypes::SENSORS) {
        Some("sensors")
    } else {
        None
    }
}

/// Content settings behind a prompt mask; empty for kinds Chromium never persists (file access).
fn prompt_settings(mask: u32) -> Vec<ContentSettingTypes> {
    let pairs = [
        (PermissionRequestTypes::MIC_STREAM, ContentSettingTypes::MEDIASTREAM_MIC),
        (PermissionRequestTypes::CAMERA_STREAM, ContentSettingTypes::MEDIASTREAM_CAMERA),
        (PermissionRequestTypes::GEOLOCATION, ContentSettingTypes::GEOLOCATION),
        (PermissionRequestTypes::NOTIFICATIONS, ContentSettingTypes::NOTIFICATIONS),
        (PermissionRequestTypes::CLIPBOARD, ContentSettingTypes::CLIPBOARD_READ_WRITE),
        (PermissionRequestTypes::MIDI_SYSEX, ContentSettingTypes::MIDI_SYSEX),
        (PermissionRequestTypes::WINDOW_MANAGEMENT, ContentSettingTypes::WINDOW_MANAGEMENT),
        (PermissionRequestTypes::LOCAL_FONTS, ContentSettingTypes::LOCAL_FONTS),
        (PermissionRequestTypes::MULTIPLE_DOWNLOADS, ContentSettingTypes::AUTOMATIC_DOWNLOADS),
        (PermissionRequestTypes::SENSORS, ContentSettingTypes::SENSORS),
    ];
    pairs
        .into_iter()
        .filter(|(flag, _)| mask & (flag.get_raw() as u32) != 0)
        .map(|(_, setting)| setting)
        .collect()
}

/// Content settings behind a media mask; screen capture is never persisted, as in Chrome.
fn media_settings(mask: u32) -> Vec<ContentSettingTypes> {
    let has = |flag: MediaAccessPermissionTypes| (mask & (flag.get_raw() as u32)) != 0;
    let mut out = Vec::new();
    if has(MediaAccessPermissionTypes::DEVICE_AUDIO_CAPTURE) {
        out.push(ContentSettingTypes::MEDIASTREAM_MIC);
    }
    if has(MediaAccessPermissionTypes::DEVICE_VIDEO_CAPTURE) {
        out.push(ContentSettingTypes::MEDIASTREAM_CAMERA);
    }
    if has(MediaAccessPermissionTypes::DESKTOP_AUDIO_CAPTURE) || has(MediaAccessPermissionTypes::DESKTOP_VIDEO_CAPTURE) {
        out.clear();
    }
    out
}

/// A remembered decision covering every requested setting, if there is one.
fn remembered(ctx: Option<&RequestContext>, origin: &str, settings: &[ContentSettingTypes]) -> Option<bool> {
    let ctx = ctx?;
    if settings.is_empty() {
        return None;
    }
    let origin = CefString::from(origin);
    let values: Vec<_> = settings
        .iter()
        .map(|t| ctx.content_setting(Some(&origin), Some(&origin), t.clone()))
        .collect();
    if values.iter().any(|v| *v == ContentSettingValues::BLOCK) {
        Some(false)
    } else if values.iter().all(|v| *v == ContentSettingValues::ALLOW) {
        Some(true)
    } else {
        None
    }
}

fn context_of(browser: Option<&mut Browser>) -> Option<RequestContext> {
    browser?.host()?.request_context()
}

fn media_permission_name(mask: u32) -> Option<&'static str> {
    let has = |flag: MediaAccessPermissionTypes| (mask & (flag.get_raw() as u32)) != 0;
    if has(MediaAccessPermissionTypes::DEVICE_AUDIO_CAPTURE) {
        Some("microphone")
    } else if has(MediaAccessPermissionTypes::DEVICE_VIDEO_CAPTURE)
        || has(MediaAccessPermissionTypes::DESKTOP_AUDIO_CAPTURE)
        || has(MediaAccessPermissionTypes::DESKTOP_VIDEO_CAPTURE)
    {
        Some("camera")
    } else {
        None
    }
}

fn origin_of(uri: &str) -> String {
    match url::Url::parse(uri) {
        Ok(u) => u.origin().ascii_serialization(),
        Err(_) => uri.to_string(),
    }
}

pub fn respond_permission(request_id: u32, allow: bool, remember: bool) {
    let pending = with_host(|h| h.permissions.remove(&request_id)).flatten();
    if let Some(p) = pending {
        p.answer(allow, remember);
    }
}

wrap_permission_handler! {
    pub(super) struct TabPermissionHandler {
        shared: Arc<TabShared>,
    }

    impl PermissionHandler {
        fn on_show_permission_prompt(
            &self,
            browser: Option<&mut Browser>,
            _prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> ::std::os::raw::c_int {
            let (Some(name), Some(cb)) = (prompt_permission_name(requested_permissions), callback) else {
                return 0;
            };
            let origin = origin_of(&opt_cef_str(requesting_origin));
            let ctx = context_of(browser);
            let settings = prompt_settings(requested_permissions);
            if let Some(allow) = remembered(ctx.as_ref(), &origin, &settings) {
                cb.cont(if allow { PermissionRequestResult::ACCEPT } else { PermissionRequestResult::DENY });
                return 1;
            }
            let request_id = NEXT_PERMISSION.fetch_add(1, Ordering::Relaxed);
            let tab = self.shared.id.clone();
            let pending = PendingPermission {
                callback: Some(PermCallback::Prompt(cb.clone())),
                tab: tab.clone(),
                ctx,
                origin: origin.clone(),
                settings,
            };
            let stored = with_host(move |h| {
                h.permissions.insert(request_id, pending);
            });
            if stored.is_some() {
                emit_tab(
                    &self.shared.id,
                    "permission-request",
                    PermissionEvent {
                        request_id,
                        tab_id: tab,
                        origin: origin.clone(),
                        kind: name.into(),
                        user_initiated: true,
                    },
                );
                return 1;
            }
            0
        }

        fn on_request_media_access_permission(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> ::std::os::raw::c_int {
            let (Some(name), Some(cb)) = (media_permission_name(requested_permissions), callback) else {
                return 0;
            };
            let origin = origin_of(&opt_cef_str(requesting_origin));
            let ctx = context_of(browser);
            let settings = media_settings(requested_permissions);
            if let Some(allow) = remembered(ctx.as_ref(), &origin, &settings) {
                if allow {
                    cb.cont(requested_permissions);
                } else {
                    cb.cancel();
                }
                return 1;
            }
            let request_id = NEXT_PERMISSION.fetch_add(1, Ordering::Relaxed);
            let tab = self.shared.id.clone();
            let pending = PendingPermission {
                callback: Some(PermCallback::Media(cb.clone(), requested_permissions)),
                tab: tab.clone(),
                ctx,
                origin: origin.clone(),
                settings,
            };
            let stored = with_host(move |h| {
                h.permissions.insert(request_id, pending);
            });
            if stored.is_some() {
                emit_tab(
                    &self.shared.id,
                    "permission-request",
                    PermissionEvent {
                        request_id,
                        tab_id: tab,
                        origin: origin.clone(),
                        kind: name.into(),
                        user_initiated: true,
                    },
                );
                return 1;
            }
            0
        }
    }
}
