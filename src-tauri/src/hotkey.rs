//! System-wide shortcuts: bindings marked `global` (the notes editor's Ctrl+Alt+N by default)
//! also work while another app has the focus, through `RegisterHotKey`.
//!
//! The hotkeys belong to one background thread with its own message loop, which receives
//! `WM_HOTKEY` and hands the action to the UI thread: the Zenith window the user was last in
//! (or the normal window) comes to the front and runs the action like any other shortcut.
//! While Zenith itself has the focus the hotkey wins over the in-window bindings, so the action
//! runs once either way.

use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tauri::Manager;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, SendInput, UnregisterHotKey, HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD,
    KEYBDINPUT, KEYBD_EVENT_FLAGS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, VIRTUAL_KEY, VK_RMENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY, WM_USER,
};

use crate::keys::{is_reserved, parse_chord, BindingSpec, Chord};

/// Posted to the hotkey thread when `PENDING` holds a new set of hotkeys.
const WM_APPLY: u32 = WM_APP + 1;

#[derive(Clone, Debug, PartialEq)]
struct Wanted {
    chord: Chord,
    /// As the user wrote it ("Ctrl+Alt+N"), for error reports.
    text: String,
    action: String,
}

struct Request {
    wanted: Vec<Wanted>,
    reply: mpsc::Sender<Vec<String>>,
}

static THREAD_ID: OnceLock<u32> = OnceLock::new();
static PENDING: Mutex<Option<Request>> = Mutex::new(None);

/// The system-wide hotkeys `specs` ask for. Chords a page must keep receiving, and chords
/// without Ctrl or Alt (a bare F-key would be taken from every app), are left out and reported.
fn wanted(specs: &[BindingSpec]) -> (Vec<Wanted>, Vec<String>) {
    let mut out: Vec<Wanted> = Vec::new();
    let mut refused = Vec::new();
    for spec in specs.iter().filter(|s| s.global) {
        for text in &spec.keys {
            match parse_chord(text) {
                Some(chord) if !is_reserved(&chord) && (chord.ctrl || chord.alt) => {
                    if !out.iter().any(|w| w.chord == chord) {
                        out.push(Wanted { chord, text: text.clone(), action: spec.action.clone() });
                    }
                }
                _ => refused.push(text.clone()),
            }
        }
    }
    (out, refused)
}

fn modifiers(chord: &Chord) -> HOT_KEY_MODIFIERS {
    let mut m = MOD_NOREPEAT;
    if chord.ctrl {
        m |= MOD_CONTROL;
    }
    if chord.alt {
        m |= MOD_ALT;
    }
    if chord.shift {
        m |= MOD_SHIFT;
    }
    m
}

/// Replaces the registered system-wide hotkeys with the `global` bindings in `specs`. Returns
/// the shortcuts that could not be registered, usually because another app already owns them.
pub fn set(specs: &[BindingSpec]) -> Vec<String> {
    let (wanted, mut failed) = wanted(specs);
    // No thread until a hotkey is first wanted; with none wanted there is nothing to undo.
    if wanted.is_empty() && THREAD_ID.get().is_none() {
        return failed;
    }
    let tid = *THREAD_ID.get_or_init(start);
    if tid == 0 {
        failed.extend(wanted.into_iter().map(|w| w.text));
        return failed;
    }
    let (tx, rx) = mpsc::channel();
    let texts: Vec<String> = wanted.iter().map(|w| w.text.clone()).collect();
    if let Ok(mut p) = PENDING.lock() {
        *p = Some(Request { wanted, reply: tx });
    }
    let posted = unsafe { PostThreadMessageW(tid, WM_APPLY, Default::default(), Default::default()) };
    if posted.is_err() {
        failed.extend(texts);
        return failed;
    }
    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(more) => failed.extend(more),
        Err(_) => failed.extend(texts),
    }
    failed
}

/// Starts the hotkey thread and returns its id (0 if it could not start).
fn start() -> u32 {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("zenith-hotkeys".into()).spawn(move || run(tx));
    if spawned.is_err() {
        return 0;
    }
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or(0)
}

struct Registered {
    id: i32,
    chord: Chord,
    action: String,
}

fn run(ready: mpsc::Sender<u32>) {
    let mut registered: Vec<Registered> = Vec::new();
    unsafe {
        let mut msg = MSG::default();
        // Creates this thread's message queue before its id is handed out.
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        let _ = ready.send(GetCurrentThreadId());
        // GetMessageW: 0 on WM_QUIT, -1 on error.
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            match msg.message {
                WM_HOTKEY => {
                    let id = msg.wParam.0 as i32;
                    let Some(r) = registered.iter().find(|r| r.id == id) else { continue };
                    // AltGr is reported as Ctrl+Alt: on layouts where AltGr plus the key types a
                    // character (AltGr+N is "ń" in Polish), give the key back to the app.
                    if r.chord.ctrl && r.chord.alt && GetAsyncKeyState(VK_RMENU.0 as i32) < 0 {
                        pass_through(r);
                        continue;
                    }
                    fire(r.action.clone());
                }
                WM_APPLY => {
                    let Some(req) = PENDING.lock().ok().and_then(|mut p| p.take()) else { continue };
                    for r in registered.drain(..) {
                        let _ = UnregisterHotKey(None, r.id);
                    }
                    let mut failed = Vec::new();
                    for (i, w) in req.wanted.into_iter().enumerate() {
                        let id = i as i32 + 1;
                        if RegisterHotKey(None, id, modifiers(&w.chord), w.chord.vk).is_ok() {
                            registered.push(Registered { id, chord: w.chord, action: w.action });
                        } else {
                            failed.push(w.text);
                        }
                    }
                    let _ = req.reply.send(failed);
                }
                _ => {}
            }
        }
    }
}

/// Re-sends a hotkey's key press to the focused app with the hotkey briefly unregistered.
unsafe fn pass_through(r: &Registered) {
    let _ = UnregisterHotKey(None, r.id);
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(r.chord.vk as u16),
                wScan: 0,
                dwFlags: KEYBD_EVENT_FLAGS(0),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    // The injected key is matched against hotkeys asynchronously; let it through first.
    std::thread::sleep(Duration::from_millis(80));
    let _ = RegisterHotKey(None, r.id, modifiers(&r.chord), r.chord.vk);
}

/// Brings a Zenith window to the front and runs `action` there: the focused Zenith window, else
/// the normal window, else any (incognito) window.
fn fire(action: String) {
    let app = crate::host::app().clone();
    let handle = app.clone();
    let _ = handle.run_on_main_thread(move || {
        let windows: Vec<_> = app
            .webview_windows()
            .into_values()
            .filter(|w| w.label() == crate::host::MAIN || crate::host::is_private_window(w.label()))
            .collect();
        let target = windows
            .iter()
            .find(|w| w.is_focused().unwrap_or(false))
            .or_else(|| windows.iter().find(|w| w.label() == crate::host::MAIN))
            .or_else(|| windows.first());
        let Some(w) = target else { return };
        if w.is_minimized().unwrap_or(false) {
            let _ = w.unminimize();
        }
        let _ = w.show();
        let _ = w.set_focus();
        crate::host::emit_shortcut(w.label(), &action);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(action: &str, keys: &[&str], global: bool) -> BindingSpec {
        BindingSpec { action: action.into(), keys: keys.iter().map(|k| k.to_string()).collect(), global }
    }

    #[test]
    fn only_global_bindings_become_hotkeys() {
        let (w, refused) = wanted(&[
            spec("newTab", &["Ctrl+T"], false),
            spec("openNotes", &["Ctrl+Alt+N", "Ctrl+Alt+N"], true),
        ]);
        assert_eq!(w.len(), 1, "duplicates collapse");
        assert_eq!(w[0].action, "openNotes");
        assert_eq!(w[0].chord, parse_chord("Ctrl+Alt+N").unwrap());
        assert!(refused.is_empty());
    }

    #[test]
    fn unsafe_global_chords_are_refused() {
        let (w, refused) = wanted(&[spec("openNotes", &["F7", "Ctrl+C", "Ctrl+Nope", "Alt+N"], true)]);
        assert_eq!(w.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["Alt+N"]);
        assert_eq!(refused, ["F7", "Ctrl+C", "Ctrl+Nope"]);
    }

    #[test]
    fn modifier_flags() {
        let m = modifiers(&parse_chord("Ctrl+Alt+N").unwrap());
        assert_eq!(m, MOD_NOREPEAT | MOD_CONTROL | MOD_ALT);
        let m = modifiers(&parse_chord("Ctrl+Shift+F9").unwrap());
        assert_eq!(m, MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT);
    }

    #[test]
    fn nothing_wanted_starts_no_thread() {
        assert!(set(&[spec("newTab", &["Ctrl+T"], false)]).is_empty());
    }
}
