//! Keyboard shortcut bindings shared by the chrome UI and every tab webview.
//!
//! Key events are intercepted in WebView2's `AcceleratorKeyPressed` event, which fires while a
//! page has focus, before the page sees the key. Bindings are expressed as strings such as
//! `"Ctrl+Shift+T"` so the settings UI and this module agree on one format.

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub vk: u32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BindingSpec {
    pub action: String,
    pub keys: Vec<String>,
    /// Also registered system-wide, so the shortcut works while another app has the focus
    /// (see `hotkey.rs`).
    #[serde(default)]
    pub global: bool,
}

#[derive(Default)]
pub struct Bindings {
    map: HashMap<Chord, String>,
}

impl Bindings {
    pub fn from_specs(specs: &[BindingSpec]) -> Self {
        let mut map = HashMap::new();
        for spec in specs {
            for key in &spec.keys {
                if let Some(chord) = parse_chord(key) {
                    if !is_reserved(&chord) {
                        map.insert(chord, spec.action.clone());
                    }
                }
            }
        }
        Self { map }
    }

    pub fn lookup(&self, chord: &Chord) -> Option<&str> {
        self.map.get(chord).map(String::as_str)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Editing and clipboard chords that pages and text fields must always receive.
pub fn is_reserved(chord: &Chord) -> bool {
    let plain_ctrl = chord.ctrl && !chord.alt;
    let letter = |c: u8| chord.vk == c as u32;
    if plain_ctrl
        && !chord.shift
        && (letter(b'C') || letter(b'V') || letter(b'X') || letter(b'A') || letter(b'Z') || letter(b'Y'))
    {
        return true;
    }
    if plain_ctrl && chord.shift && (letter(b'Z') || letter(b'V')) {
        return true;
    }
    // Ctrl+Backspace/Delete/arrows/Home/End are word and caret navigation in text fields.
    if plain_ctrl && matches!(chord.vk, 0x08 | 0x2E | 0x25..=0x28 | 0x24 | 0x23) {
        return true;
    }
    // A binding needs a modifier unless it is a function key.
    let function_key = (0x70..=0x87).contains(&chord.vk);
    !chord.ctrl && !chord.alt && !function_key
}

pub fn parse_chord(text: &str) -> Option<Chord> {
    let mut chord = Chord {
        ctrl: false,
        alt: false,
        shift: false,
        vk: 0,
    };
    let mut key = None;
    // "Ctrl++" style is not used; the plus key is spelled "Plus".
    for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => chord.ctrl = true,
            "alt" => chord.alt = true,
            "shift" => chord.shift = true,
            _ => key = Some(vk_from_name(part)?),
        }
    }
    chord.vk = key?;
    Some(chord)
}

pub fn vk_from_name(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    if lower.len() == 1 {
        let c = lower.as_bytes()[0];
        if c.is_ascii_lowercase() {
            return Some(c.to_ascii_uppercase() as u32);
        }
        if c.is_ascii_digit() {
            return Some(c as u32);
        }
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x6F + n);
        }
    }
    if let Some(n) = lower.strip_prefix("numpad").and_then(|n| n.parse::<u32>().ok()) {
        if n <= 9 {
            return Some(0x60 + n);
        }
    }
    Some(match lower.as_str() {
        "tab" => 0x09,
        "enter" | "return" => 0x0D,
        "escape" | "esc" => 0x1B,
        "space" => 0x20,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" | "arrowleft" => 0x25,
        "up" | "arrowup" => 0x26,
        "right" | "arrowright" => 0x27,
        "down" | "arrowdown" => 0x28,
        "insert" => 0x2D,
        "delete" => 0x2E,
        "backspace" => 0x08,
        "plus" | "equal" | "=" => 0xBB,
        "minus" | "-" => 0xBD,
        "comma" | "," => 0xBC,
        "period" | "." => 0xBE,
        "slash" | "/" => 0xBF,
        "backquote" | "`" => 0xC0,
        "bracketleft" | "[" => 0xDB,
        "backslash" | "\\" => 0xDC,
        "bracketright" | "]" => 0xDD,
        "quote" | "'" => 0xDE,
        "semicolon" | ";" => 0xBA,
        "numpadadd" => 0x6B,
        "numpadsubtract" => 0x6D,
        "numpadmultiply" => 0x6A,
        "numpaddivide" => 0x6F,
        "numpaddecimal" => 0x6E,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_chords() {
        let c = parse_chord("Ctrl+Shift+T").unwrap();
        assert!(c.ctrl && c.shift && !c.alt);
        assert_eq!(c.vk, b'T' as u32);
        assert_eq!(parse_chord("Alt+Left").unwrap().vk, 0x25);
        assert_eq!(parse_chord("F5").unwrap().vk, 0x74);
        assert_eq!(parse_chord("Ctrl+Plus").unwrap().vk, 0xBB);
        assert_eq!(parse_chord("Ctrl+9").unwrap().vk, b'9' as u32);
        assert!(parse_chord("Ctrl+Nope").is_none());
        assert!(parse_chord("Ctrl").is_none());
    }

    #[test]
    fn reserved_keys_are_never_bound() {
        assert!(is_reserved(&parse_chord("Ctrl+C").unwrap()));
        assert!(is_reserved(&parse_chord("Ctrl+V").unwrap()));
        assert!(is_reserved(&parse_chord("Ctrl+Shift+Z").unwrap()));
        assert!(is_reserved(&parse_chord("Ctrl+Backspace").unwrap()));
        assert!(is_reserved(&parse_chord("K").unwrap()));
        assert!(!is_reserved(&parse_chord("F11").unwrap()));
        assert!(!is_reserved(&parse_chord("Ctrl+T").unwrap()));
        let bindings = Bindings::from_specs(&[BindingSpec {
            action: "copy".into(),
            keys: vec!["Ctrl+C".into(), "Ctrl+Shift+C".into()],
            global: false,
        }]);
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings.lookup(&parse_chord("Ctrl+Shift+C").unwrap()),
            Some("copy")
        );
    }
}
