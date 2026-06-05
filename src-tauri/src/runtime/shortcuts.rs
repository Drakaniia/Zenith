use serde::Serialize;
use tauri::menu::{Menu, MenuEvent, MenuItem, Submenu};
use tauri::AppHandle;

use crate::bridge::events::{emit_global, BROWSER_SHORTCUT};

const SHORTCUT_MENU_PREFIX: &str = "ampui-shortcut:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutDefinition {
    pub id: &'static str,
    pub command: &'static str,
    pub label: &'static str,
    pub accelerator: &'static str,
}

#[derive(Clone, Serialize)]
struct BrowserShortcutPayload {
    command: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeShortcutKey {
    pub virtual_key: u32,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

pub const SHORTCUTS: &[ShortcutDefinition] = &[
    ShortcutDefinition {
        id: "new-tab",
        command: "new-tab",
        label: "New Tab",
        accelerator: "CmdOrCtrl+T",
    },
    ShortcutDefinition {
        id: "new-window",
        command: "new-window",
        label: "New Window",
        accelerator: "CmdOrCtrl+N",
    },
    ShortcutDefinition {
        id: "new-incognito-window",
        command: "new-incognito-window",
        label: "New Private Window",
        accelerator: "CmdOrCtrl+Shift+N",
    },
    ShortcutDefinition {
        id: "restore-tab",
        command: "restore-tab",
        label: "Reopen Closed Tab",
        accelerator: "CmdOrCtrl+Shift+T",
    },
    ShortcutDefinition {
        id: "close-tab",
        command: "close-tab",
        label: "Close Tab",
        accelerator: "CmdOrCtrl+W",
    },
    ShortcutDefinition {
        id: "close-tab-f4",
        command: "close-tab",
        label: "Close Tab",
        accelerator: "CmdOrCtrl+F4",
    },
    ShortcutDefinition {
        id: "close-window",
        command: "close-window",
        label: "Close Window",
        accelerator: "CmdOrCtrl+Shift+W",
    },
    ShortcutDefinition {
        id: "close-window-alt-f4",
        command: "close-window",
        label: "Close Window",
        accelerator: "Alt+F4",
    },
    #[cfg(target_os = "macos")]
    ShortcutDefinition {
        id: "quit-browser",
        command: "quit-browser",
        label: "Quit",
        accelerator: "Cmd+Q",
    },
    ShortcutDefinition {
        id: "next-tab",
        command: "next-tab",
        label: "Next Tab",
        accelerator: "Ctrl+Tab",
    },
    ShortcutDefinition {
        id: "next-tab-page",
        command: "next-tab",
        label: "Next Tab",
        accelerator: "Ctrl+PageDown",
    },
    ShortcutDefinition {
        id: "previous-tab",
        command: "previous-tab",
        label: "Previous Tab",
        accelerator: "Ctrl+Shift+Tab",
    },
    ShortcutDefinition {
        id: "previous-tab-page",
        command: "previous-tab",
        label: "Previous Tab",
        accelerator: "Ctrl+PageUp",
    },
    ShortcutDefinition {
        id: "move-tab-left",
        command: "move-tab-left",
        label: "Move Tab Left",
        accelerator: "Ctrl+Shift+PageUp",
    },
    ShortcutDefinition {
        id: "move-tab-right",
        command: "move-tab-right",
        label: "Move Tab Right",
        accelerator: "Ctrl+Shift+PageDown",
    },
    ShortcutDefinition {
        id: "tab-1",
        command: "tab-1",
        label: "Select Tab 1",
        accelerator: "CmdOrCtrl+1",
    },
    ShortcutDefinition {
        id: "tab-2",
        command: "tab-2",
        label: "Select Tab 2",
        accelerator: "CmdOrCtrl+2",
    },
    ShortcutDefinition {
        id: "tab-3",
        command: "tab-3",
        label: "Select Tab 3",
        accelerator: "CmdOrCtrl+3",
    },
    ShortcutDefinition {
        id: "tab-4",
        command: "tab-4",
        label: "Select Tab 4",
        accelerator: "CmdOrCtrl+4",
    },
    ShortcutDefinition {
        id: "tab-5",
        command: "tab-5",
        label: "Select Tab 5",
        accelerator: "CmdOrCtrl+5",
    },
    ShortcutDefinition {
        id: "tab-6",
        command: "tab-6",
        label: "Select Tab 6",
        accelerator: "CmdOrCtrl+6",
    },
    ShortcutDefinition {
        id: "tab-7",
        command: "tab-7",
        label: "Select Tab 7",
        accelerator: "CmdOrCtrl+7",
    },
    ShortcutDefinition {
        id: "tab-8",
        command: "tab-8",
        label: "Select Tab 8",
        accelerator: "CmdOrCtrl+8",
    },
    ShortcutDefinition {
        id: "last-tab",
        command: "last-tab",
        label: "Select Last Tab",
        accelerator: "CmdOrCtrl+9",
    },
    ShortcutDefinition {
        id: "focus-address",
        command: "focus-address",
        label: "Focus Address Bar",
        accelerator: "CmdOrCtrl+L",
    },
    ShortcutDefinition {
        id: "focus-address-search",
        command: "focus-address",
        label: "Focus Address Bar",
        accelerator: "CmdOrCtrl+K",
    },
    ShortcutDefinition {
        id: "focus-address-edit",
        command: "focus-address",
        label: "Focus Address Bar",
        accelerator: "CmdOrCtrl+E",
    },
    ShortcutDefinition {
        id: "focus-address-alt-d",
        command: "focus-address",
        label: "Focus Address Bar",
        accelerator: "Alt+D",
    },
    ShortcutDefinition {
        id: "reload",
        command: "reload",
        label: "Reload",
        accelerator: "CmdOrCtrl+R",
    },
    ShortcutDefinition {
        id: "reload-f5",
        command: "reload",
        label: "Reload",
        accelerator: "F5",
    },
    ShortcutDefinition {
        id: "hard-reload",
        command: "hard-reload",
        label: "Hard Reload",
        accelerator: "CmdOrCtrl+Shift+R",
    },
    ShortcutDefinition {
        id: "hard-reload-f5",
        command: "hard-reload",
        label: "Hard Reload",
        accelerator: "CmdOrCtrl+F5",
    },
    ShortcutDefinition {
        id: "back",
        command: "back",
        label: "Back",
        accelerator: "Alt+Left",
    },
    ShortcutDefinition {
        id: "forward",
        command: "forward",
        label: "Forward",
        accelerator: "Alt+Right",
    },
    ShortcutDefinition {
        id: "home",
        command: "home",
        label: "Home",
        accelerator: "Alt+Home",
    },
    ShortcutDefinition {
        id: "find",
        command: "find",
        label: "Find",
        accelerator: "CmdOrCtrl+F",
    },
    ShortcutDefinition {
        id: "find-next",
        command: "find-next",
        label: "Find Next",
        accelerator: "CmdOrCtrl+G",
    },
    ShortcutDefinition {
        id: "find-next-f3",
        command: "find-next",
        label: "Find Next",
        accelerator: "F3",
    },
    ShortcutDefinition {
        id: "find-previous",
        command: "find-previous",
        label: "Find Previous",
        accelerator: "CmdOrCtrl+Shift+G",
    },
    ShortcutDefinition {
        id: "find-previous-f3",
        command: "find-previous",
        label: "Find Previous",
        accelerator: "Shift+F3",
    },
    #[cfg(not(target_os = "macos"))]
    ShortcutDefinition {
        id: "history",
        command: "history",
        label: "History",
        accelerator: "CmdOrCtrl+H",
    },
    #[cfg(target_os = "macos")]
    ShortcutDefinition {
        id: "history-macos",
        command: "history",
        label: "History",
        accelerator: "Cmd+Y",
    },
    #[cfg(target_os = "macos")]
    ShortcutDefinition {
        id: "hide-window",
        command: "hide-window",
        label: "Hide",
        accelerator: "Cmd+H",
    },
    #[cfg(target_os = "macos")]
    ShortcutDefinition {
        id: "minimize-window",
        command: "minimize-window",
        label: "Minimize",
        accelerator: "Cmd+M",
    },
    ShortcutDefinition {
        id: "downloads",
        command: "downloads",
        label: "Downloads",
        accelerator: "CmdOrCtrl+J",
    },
    ShortcutDefinition {
        id: "downloads-shift",
        command: "downloads",
        label: "Downloads",
        accelerator: "CmdOrCtrl+Shift+J",
    },
    ShortcutDefinition {
        id: "bookmarks",
        command: "bookmarks",
        label: "Bookmarks",
        accelerator: "CmdOrCtrl+Shift+O",
    },
    ShortcutDefinition {
        id: "toggle-bookmarks",
        command: "toggle-bookmarks",
        label: "Toggle Bookmarks",
        accelerator: "CmdOrCtrl+Shift+B",
    },
    ShortcutDefinition {
        id: "settings",
        command: "settings",
        label: "Settings",
        accelerator: "CmdOrCtrl+,",
    },
    ShortcutDefinition {
        id: "clear-browsing-settings",
        command: "settings",
        label: "Settings",
        accelerator: "CmdOrCtrl+Shift+Delete",
    },
    ShortcutDefinition {
        id: "bookmark-page",
        command: "bookmark-page",
        label: "Bookmark This Page",
        accelerator: "CmdOrCtrl+D",
    },
    ShortcutDefinition {
        id: "bookmark-all",
        command: "bookmark-all",
        label: "Bookmark All Tabs",
        accelerator: "CmdOrCtrl+Shift+D",
    },
    ShortcutDefinition {
        id: "print",
        command: "print",
        label: "Print",
        accelerator: "CmdOrCtrl+P",
    },
    ShortcutDefinition {
        id: "zoom-in",
        command: "zoom-in",
        label: "Zoom In",
        accelerator: "CmdOrCtrl+=",
    },
    ShortcutDefinition {
        id: "zoom-in-shift",
        command: "zoom-in",
        label: "Zoom In",
        accelerator: "CmdOrCtrl+Shift+=",
    },
    ShortcutDefinition {
        id: "zoom-out",
        command: "zoom-out",
        label: "Zoom Out",
        accelerator: "CmdOrCtrl+-",
    },
    ShortcutDefinition {
        id: "zoom-reset",
        command: "zoom-reset",
        label: "Reset Zoom",
        accelerator: "CmdOrCtrl+0",
    },
    ShortcutDefinition {
        id: "fullscreen",
        command: "toggle-fullscreen",
        label: "Fullscreen",
        accelerator: "F11",
    },
    ShortcutDefinition {
        id: "focus-web-content",
        command: "focus-web-content",
        label: "Focus Page",
        accelerator: "Ctrl+F6",
    },
    ShortcutDefinition {
        id: "focus-next-shell-area",
        command: "focus-next-shell-area",
        label: "Focus Next Browser Area",
        accelerator: "F6",
    },
    ShortcutDefinition {
        id: "focus-toolbar-start",
        command: "focus-toolbar-start",
        label: "Focus Toolbar",
        accelerator: "Alt+Shift+T",
    },
    ShortcutDefinition {
        id: "focus-toolbar-end",
        command: "focus-toolbar-end",
        label: "Focus Toolbar End",
        accelerator: "F10",
    },
    ShortcutDefinition {
        id: "open-browser-menu",
        command: "open-browser-menu",
        label: "Open Browser Menu",
        accelerator: "Alt+F",
    },
    ShortcutDefinition {
        id: "open-browser-menu-alt",
        command: "open-browser-menu",
        label: "Open Browser Menu",
        accelerator: "Alt+E",
    },
    ShortcutDefinition {
        id: "keyboard-help",
        command: "keyboard-help",
        label: "Keyboard Help",
        accelerator: "F1",
    },
    ShortcutDefinition {
        id: "caret-browsing",
        command: "caret-browsing",
        label: "Caret Browsing",
        accelerator: "F7",
    },
    ShortcutDefinition {
        id: "devtools-f12",
        command: "devtools",
        label: "Developer Tools",
        accelerator: "F12",
    },
    ShortcutDefinition {
        id: "devtools",
        command: "devtools",
        label: "Developer Tools",
        accelerator: "CmdOrCtrl+Shift+I",
    },
    ShortcutDefinition {
        id: "devtools-inspect",
        command: "devtools",
        label: "Inspect",
        accelerator: "CmdOrCtrl+Shift+C",
    },
    ShortcutDefinition {
        id: "save-page",
        command: "save-page",
        label: "Save Page",
        accelerator: "CmdOrCtrl+S",
    },
    ShortcutDefinition {
        id: "open-file",
        command: "open-file",
        label: "Open File",
        accelerator: "CmdOrCtrl+O",
    },
    ShortcutDefinition {
        id: "view-source",
        command: "view-source",
        label: "View Source",
        accelerator: "CmdOrCtrl+U",
    },
    ShortcutDefinition {
        id: "profile-switcher",
        command: "profile-switcher",
        label: "Profile Switcher",
        accelerator: "CmdOrCtrl+Shift+M",
    },
];

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::new(app)?;
    let shortcuts = Submenu::with_id(app, "ampui-shortcuts", "Shortcuts", true)?;

    for shortcut in SHORTCUTS {
        let item = MenuItem::with_id(
            app,
            menu_id(shortcut.id),
            shortcut.label,
            true,
            Some(shortcut.accelerator),
        )?;
        shortcuts.append(&item)?;
    }

    menu.append(&shortcuts)?;
    app.set_menu(menu)?;

    #[cfg(not(target_os = "macos"))]
    app.hide_menu()?;

    Ok(())
}

pub fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    if let Some(command) = command_from_menu_id(event.id().as_ref()) {
        let _ = emit_shortcut_global(app, command);
    }
}

pub fn emit_shortcut_to_shell(
    app: &AppHandle,
    shell_label: &str,
    command: &'static str,
) -> Result<(), tauri::Error> {
    crate::bridge::events::emit_to_shell(
        app,
        shell_label,
        BROWSER_SHORTCUT,
        BrowserShortcutPayload { command },
    )
}

pub fn emit_shortcut_global(app: &AppHandle, command: &'static str) -> Result<(), tauri::Error> {
    emit_global(app, BROWSER_SHORTCUT, BrowserShortcutPayload { command })
}

pub fn command_from_menu_id(menu_id: &str) -> Option<&'static str> {
    let shortcut_id = menu_id.strip_prefix(SHORTCUT_MENU_PREFIX)?;

    SHORTCUTS
        .iter()
        .find(|shortcut| shortcut.id == shortcut_id)
        .map(|shortcut| shortcut.command)
}

pub fn command_for_native_key(input: NativeShortcutKey) -> Option<&'static str> {
    SHORTCUTS
        .iter()
        .find(|shortcut| native_key_matches_accelerator(input, shortcut.accelerator))
        .map(|shortcut| shortcut.command)
}

fn menu_id(shortcut_id: &str) -> String {
    format!("{SHORTCUT_MENU_PREFIX}{shortcut_id}")
}

fn native_key_matches_accelerator(input: NativeShortcutKey, accelerator: &str) -> bool {
    let Some((modifier_text, key_text)) = accelerator.rsplit_once('+') else {
        return match_key(input.virtual_key, accelerator)
            && !input.ctrl
            && !input.shift
            && !input.alt
            && !input.meta;
    };

    let mut expected = NativeShortcutKey {
        virtual_key: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    };

    for modifier in modifier_text.split('+') {
        match modifier.to_ascii_lowercase().as_str() {
            "cmdorctrl" => {
                #[cfg(target_os = "macos")]
                {
                    expected.meta = true;
                }

                #[cfg(not(target_os = "macos"))]
                {
                    expected.ctrl = true;
                }
            }
            "cmd" | "command" | "meta" | "super" => expected.meta = true,
            "ctrl" | "control" => expected.ctrl = true,
            "shift" => expected.shift = true,
            "alt" | "option" => expected.alt = true,
            _ => return false,
        }
    }

    let Some(virtual_key) = virtual_key_for_accelerator_key(key_text) else {
        return false;
    };

    expected.virtual_key = virtual_key;
    input == expected
}

fn match_key(virtual_key: u32, key_text: &str) -> bool {
    virtual_key_for_accelerator_key(key_text) == Some(virtual_key)
}

fn virtual_key_for_accelerator_key(key_text: &str) -> Option<u32> {
    let key = key_text.to_ascii_lowercase();

    if key.len() == 1 {
        let byte = key.as_bytes()[0];

        if byte.is_ascii_alphabetic() {
            return Some(byte.to_ascii_uppercase() as u32);
        }

        if byte.is_ascii_digit() {
            return Some(byte as u32);
        }
    }

    if let Some(function_key) = key.strip_prefix('f') {
        let number = function_key.parse::<u32>().ok()?;

        if (1..=24).contains(&number) {
            return Some(0x70 + number - 1);
        }
    }

    match key.as_str() {
        "tab" => Some(0x09),
        "pageup" => Some(0x21),
        "pagedown" => Some(0x22),
        "home" => Some(0x24),
        "left" | "arrowleft" => Some(0x25),
        "right" | "arrowright" => Some(0x27),
        "delete" => Some(0x2e),
        "," => Some(0xbc),
        "=" | "+" => Some(0xbb),
        "-" => Some(0xbd),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use muda::accelerator::Accelerator;

    use super::{
        command_for_native_key, command_from_menu_id, menu_id, NativeShortcutKey, SHORTCUTS,
    };

    const VK_T: u32 = 0x54;
    const VK_W: u32 = 0x57;
    const VK_F5: u32 = 0x74;
    const VK_LEFT: u32 = 0x25;

    #[test]
    fn every_shortcut_accelerator_parses() {
        for shortcut in SHORTCUTS {
            shortcut
                .accelerator
                .parse::<Accelerator>()
                .unwrap_or_else(|error| {
                    panic!("{} failed to parse: {error}", shortcut.accelerator)
                });
        }
    }

    #[test]
    fn menu_ids_are_unique() {
        let mut ids = HashSet::new();

        for shortcut in SHORTCUTS {
            assert!(
                ids.insert(shortcut.id),
                "duplicate shortcut id {}",
                shortcut.id
            );
        }
    }

    #[test]
    fn accelerators_are_unique() {
        let mut accelerators = HashSet::new();

        for shortcut in SHORTCUTS {
            assert!(
                accelerators.insert(shortcut.accelerator),
                "duplicate accelerator {}",
                shortcut.accelerator
            );
        }
    }

    #[test]
    fn menu_id_resolves_registered_command() {
        for shortcut in SHORTCUTS {
            assert_eq!(
                command_from_menu_id(&menu_id(shortcut.id)),
                Some(shortcut.command)
            );
        }
    }

    #[test]
    fn non_shortcut_menu_id_is_ignored() {
        assert_eq!(command_from_menu_id("not-a-browser-shortcut"), None);
    }

    #[test]
    fn native_key_shortcuts_match_browser_commands() {
        assert_eq!(
            command_for_native_key(NativeShortcutKey {
                virtual_key: VK_T,
                ctrl: true,
                shift: false,
                alt: false,
                meta: false,
            }),
            Some("new-tab")
        );
        assert_eq!(
            command_for_native_key(NativeShortcutKey {
                virtual_key: VK_T,
                ctrl: true,
                shift: true,
                alt: false,
                meta: false,
            }),
            Some("restore-tab")
        );
        assert_eq!(
            command_for_native_key(NativeShortcutKey {
                virtual_key: VK_LEFT,
                ctrl: false,
                shift: false,
                alt: true,
                meta: false,
            }),
            Some("back")
        );
        assert_eq!(
            command_for_native_key(NativeShortcutKey {
                virtual_key: VK_F5,
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
            }),
            Some("reload")
        );
    }

    #[test]
    fn native_key_shortcuts_require_matching_modifiers() {
        assert_eq!(
            command_for_native_key(NativeShortcutKey {
                virtual_key: VK_T,
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
            }),
            None
        );
        assert_eq!(
            command_for_native_key(NativeShortcutKey {
                virtual_key: VK_T,
                ctrl: true,
                shift: false,
                alt: true,
                meta: false,
            }),
            None
        );
        assert_eq!(
            command_for_native_key(NativeShortcutKey {
                virtual_key: VK_W,
                ctrl: true,
                shift: true,
                alt: false,
                meta: false,
            }),
            Some("close-window")
        );
    }

    #[test]
    fn documented_browser_shortcuts_are_registered() {
        let accelerators: HashSet<&str> = SHORTCUTS
            .iter()
            .map(|shortcut| shortcut.accelerator)
            .collect();
        let expected = [
            "CmdOrCtrl+T",
            "CmdOrCtrl+W",
            "Ctrl+Tab",
            "Ctrl+Shift+Tab",
            "CmdOrCtrl+L",
            "CmdOrCtrl+R",
            "F5",
            "CmdOrCtrl+Shift+R",
            "Alt+Left",
            "Alt+Right",
            "CmdOrCtrl+F",
            #[cfg(not(target_os = "macos"))]
            "CmdOrCtrl+H",
            #[cfg(target_os = "macos")]
            "Cmd+Y",
            "CmdOrCtrl+D",
            "CmdOrCtrl+,",
            "CmdOrCtrl+Shift+J",
            "CmdOrCtrl+1",
            "CmdOrCtrl+2",
            "CmdOrCtrl+3",
            "CmdOrCtrl+4",
            "CmdOrCtrl+5",
            "CmdOrCtrl+6",
            "CmdOrCtrl+7",
            "CmdOrCtrl+8",
            "CmdOrCtrl+9",
        ];

        for accelerator in expected {
            assert!(
                accelerators.contains(accelerator),
                "missing documented shortcut {accelerator}"
            );
        }
    }
}
