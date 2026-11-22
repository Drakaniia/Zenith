//! Keyboard shortcuts: active bindings and pre-page CEF `KeyboardHandler` interception.

use std::cell::RefCell;
use std::sync::Arc;

use cef::*;
use serde::Serialize;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RMENU, VK_SHIFT, VIRTUAL_KEY,
};

use super::emit_to_window;
use super::tab::TabShared;
use crate::shortcuts::keys::{BindingSpec, Bindings, Chord};

thread_local! {
    static BINDINGS: RefCell<Bindings> = RefCell::new(Bindings::default());
}

pub fn set_bindings(specs: Vec<BindingSpec>) -> Vec<String> {
    let bindings = Bindings::from_specs(&specs);
    BINDINGS.with(|b| *b.borrow_mut() = bindings);
    crate::shortcuts::hotkey::set(&specs)
}

pub fn emit_shortcut(label: &str, action: &str) {
    emit_to_window(
        label,
        "shortcut",
        ShortcutEvent {
            action: action.to_string(),
            repeat: false,
            source: None,
            keys: None,
        },
    );
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutEvent {
    pub action: String,
    pub repeat: bool,
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<ShortcutKeys>,
}

#[derive(Serialize, Clone, Copy)]
pub struct ShortcutKeys {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub vk: u32,
}

pub fn key_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetKeyState(vk.0 as i32) < 0 }
}

pub fn bound_action(ctrl: bool, alt: bool, shift: bool, vk: u32) -> Option<String> {
    // AltGr is reported as Ctrl+Alt: leave those chords to text input.
    if ctrl && alt && key_down(VK_RMENU) {
        return None;
    }
    let chord = Chord { ctrl, alt, shift, vk };
    BINDINGS.with(|b| b.borrow().lookup(&chord).map(str::to_string))
}

/// Intercepts Zenith shortcuts in CEF `KeyboardHandler::on_pre_key_event` before the page sees
/// them. Returns `true` if handled and should be suppressed from the renderer.
pub fn handle_pre_key_event(event: &KeyEvent, tab: Option<&Arc<TabShared>>, label: &str) -> bool {
    if event.type_ != KeyEventType::RAWKEYDOWN && event.type_ != KeyEventType::KEYDOWN {
        return false;
    }
    let vk = event.windows_key_code as u32;
    let mods = event.modifiers;
    // CEF cef_event_flags_t bitmasks: SHIFT=1<<1, CONTROL=1<<2, ALT=1<<3, IS_REPEAT=1<<13.
    let shift = (mods & (1 << 1)) != 0 || key_down(VK_SHIFT);
    let ctrl = (mods & (1 << 2)) != 0 || key_down(VK_CONTROL);
    let alt = (mods & (1 << 3)) != 0 || key_down(VK_MENU);
    let repeat = (mods & (1 << 13)) != 0;
    let source = tab.map(|t| t.id.clone());

    if let Some(t) = tab {
        if vk == VK_ESCAPE.0 as u32 && !ctrl && !alt && !shift {
            if t.is_glance() {
                emit_to_window(
                    label,
                    "shortcut",
                    ShortcutEvent { action: "closeGlance".into(), repeat, source, keys: None },
                );
                return true;
            }
            if !repeat {
                emit_to_window(
                    label,
                    "shortcut",
                    ShortcutEvent { action: "escape".into(), repeat, source, keys: None },
                );
            }
            return false;
        }
    }

    if let Some(action) = bound_action(ctrl, alt, shift, vk) {
        let keys = tab.is_none().then_some(ShortcutKeys { ctrl, alt, shift, vk });
        emit_to_window(
            label,
            "shortcut",
            ShortcutEvent { action, repeat, source, keys },
        );
        return true;
    }
    false
}
