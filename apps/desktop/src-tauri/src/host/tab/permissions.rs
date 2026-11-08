//! Site permission requests (camera, microphone, location, ...): held with a deferral until
//! the user answers in the chrome UI.

use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, PWSTR};

use super::{take, TabShared};
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

pub struct PendingPermission {
    pub args: ICoreWebView2PermissionRequestedEventArgs,
    pub deferral: ICoreWebView2Deferral,
    pub tab: String,
    pub answered: bool,
}

impl PendingPermission {
    pub fn answer(mut self, allow: bool, remember: bool) {
        unsafe {
            let _ = self.args.SetState(if allow {
                COREWEBVIEW2_PERMISSION_STATE_ALLOW
            } else {
                COREWEBVIEW2_PERMISSION_STATE_DENY
            });
            if let Ok(a3) = self.args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
                let _ = a3.SetSavesInProfile(remember);
            }
            let _ = self.deferral.Complete();
        }
        self.answered = true;
    }
}

impl Drop for PendingPermission {
    fn drop(&mut self) {
        if !self.answered {
            unsafe {
                let _ = self.args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY);
                let _ = self.deferral.Complete();
            }
        }
    }
}

fn permission_name(kind: COREWEBVIEW2_PERMISSION_KIND) -> Option<&'static str> {
    Some(match kind {
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => "microphone",
        COREWEBVIEW2_PERMISSION_KIND_CAMERA => "camera",
        COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION => "location",
        COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS => "notifications",
        COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ => "clipboard",
        COREWEBVIEW2_PERMISSION_KIND_MIDI_SYSTEM_EXCLUSIVE_MESSAGES => "midi",
        COREWEBVIEW2_PERMISSION_KIND_WINDOW_MANAGEMENT => "windowManagement",
        COREWEBVIEW2_PERMISSION_KIND_LOCAL_FONTS => "localFonts",
        COREWEBVIEW2_PERMISSION_KIND_FILE_READ_WRITE => "files",
        COREWEBVIEW2_PERMISSION_KIND_MULTIPLE_AUTOMATIC_DOWNLOADS => "downloads",
        COREWEBVIEW2_PERMISSION_KIND_OTHER_SENSORS => "sensors",
        _ => return None,
    })
}

fn origin_of(uri: &str) -> String {
    match tauri::Url::parse(uri) {
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

/// Site permission requests, held until the user answers.
pub(super) unsafe fn wire_permissions(core: &ICoreWebView2, s: &Rc<TabShared>) {
    let mut token = 0i64;
    unsafe {
        let sh = s.clone();
        let _ = core.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut kind)?;
                let Some(name) = permission_name(kind) else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = take(uri);
                let mut user = BOOL::default();
                args.IsUserInitiated(&mut user)?;
                let deferral = args.GetDeferral()?;
                let request_id = NEXT_PERMISSION.fetch_add(1, Ordering::Relaxed);
                let tab = sh.id.clone();
                let pending = PendingPermission { args, deferral, tab: tab.clone(), answered: false };
                let stored = with_host(move |h| {
                    h.permissions.insert(request_id, pending);
                });
                if stored.is_some() {
                    emit_tab(
                        &sh.id,
                        "permission-request",
                        PermissionEvent {
                            request_id,
                            tab_id: tab,
                            origin: origin_of(&uri),
                            kind: name.into(),
                            user_initiated: user.as_bool(),
                        },
                    );
                }
                Ok(())
            })),
            &mut token,
        );
    }
}
