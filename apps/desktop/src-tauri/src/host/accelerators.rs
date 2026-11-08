//! Keyboard shortcuts: the active bindings, and routing browser shortcuts from every webview to
//! its window's chrome UI.

use std::cell::RefCell;

use serde::Serialize;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RMENU, VK_SHIFT};

use super::emit_to_window;
use super::tab::TabShared;
use crate::shortcuts::keys::{BindingSpec, Bindings, Chord};

// ---------------------------------------------------------------------------------------------
// Keyboard shortcuts

thread_local! {
    static BINDINGS: RefCell<Bindings> = RefCell::new(Bindings::default());
}

/// Applies the shortcut bindings, including the system-wide ones. Returns the system-wide
/// shortcuts that could not be registered.
pub fn set_bindings(specs: Vec<BindingSpec>) -> Vec<String> {
    let bindings = Bindings::from_specs(&specs);
    BINDINGS.with(|b| *b.borrow_mut() = bindings);
    crate::shortcuts::hotkey::set(&specs)
}

/// Runs a browser action in window `label`'s chrome UI, as if its shortcut was pressed there.
pub fn emit_shortcut(label: &str, action: &str) {
    emit_to_window(label, "shortcut", ShortcutEvent { action: action.to_string(), repeat: false, source: None });
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct ShortcutEvent {
    pub(super) action: String,
    pub(super) repeat: bool,
    pub(super) source: Option<String>,
}

pub(super) fn key_down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    unsafe { GetKeyState(vk.0 as i32) < 0 }
}

/// The browser action bound to a chord, if any.
pub(super) fn bound_action(ctrl: bool, alt: bool, shift: bool, vk: u32) -> Option<String> {
    // AltGr is reported as Ctrl+Alt: leave those chords to text input.
    if ctrl && alt && key_down(VK_RMENU) {
        return None;
    }
    let chord = Chord { ctrl, alt, shift, vk };
    BINDINGS.with(|b| b.borrow().lookup(&chord).map(str::to_string))
}

/// Routes browser shortcuts from a webview (the chrome when `tab` is `None`) to the chrome UI of
/// window `label`. Matching keys are marked handled so the page never sees them; everything else
/// passes through untouched.
pub fn attach_accelerators(controller: &ICoreWebView2Controller, tab: Option<std::rc::Rc<TabShared>>, label: &str) {
    let label = label.to_string();
    let mut token = 0i64;
    unsafe {
        let _ = controller.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                    && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                {
                    return Ok(());
                }
                let mut vk = 0u32;
                args.VirtualKey(&mut vk)?;
                let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
                args.PhysicalKeyStatus(&mut status)?;
                let ctrl = key_down(VK_CONTROL);
                let shift = key_down(VK_SHIFT);
                let alt = status.IsMenuKeyDown.as_bool() || key_down(VK_MENU);
                let repeat = status.WasKeyDown.as_bool();
                let source = tab.as_ref().map(|t| t.id.clone());

                if let Some(t) = &tab {
                    if vk == VK_ESCAPE.0 as u32 && !ctrl && !alt && !shift {
                        if t.glance.get() {
                            args.SetHandled(true)?;
                            emit_to_window(&label, "shortcut", ShortcutEvent { action: "closeGlance".into(), repeat, source });
                            return Ok(());
                        }
                        // Esc stops a page that is still loading (the UI knows whether it is);
                        // the page gets the key as well.
                        if !repeat {
                            emit_to_window(&label, "shortcut", ShortcutEvent { action: "escape".into(), repeat, source });
                        }
                        return Ok(());
                    }
                }
                if let Some(action) = bound_action(ctrl, alt, shift, vk) {
                    args.SetHandled(true)?;
                    emit_to_window(&label, "shortcut", ShortcutEvent { action, repeat, source });
                }
                Ok(())
            })),
            &mut token,
        );
    }
}
