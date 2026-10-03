//! `ovt_core::hotkey::Combo` (GTK accelerator strings such as `<Control><Alt>space`) → the `MOD_*` flags and virtual-key
//! code `RegisterHotKey` takes (follows `HotKey.swift`, where combos are Carbon key codes). Pure, so it's tested on every
//! platform; the numbers are the Win32 constants.

use ovt_core::hotkey::Combo;

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;
/// Holding the keys doesn't repeat WM_HOTKEY (the release is seen by the keyboard hook).
pub const MOD_NOREPEAT: u32 = 0x4000;

pub const VK_ESCAPE: u32 = 0x1B;

/// A combo as `RegisterHotKey` wants it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinCombo {
    /// `MOD_ALT | MOD_CONTROL | …` (without `MOD_NOREPEAT`).
    pub modifiers: u32,
    pub vk: u32,
}

/// None if the combo doesn't parse or names a key this table doesn't know.
pub fn win_combo(combo: &Combo) -> Option<WinCombo> {
    let parsed = combo.parse()?;
    let mut modifiers = 0;
    for (on, flag) in
        [(parsed.control, MOD_CONTROL), (parsed.alt, MOD_ALT), (parsed.shift, MOD_SHIFT), (parsed.super_, MOD_WIN)]
    {
        if on {
            modifiers |= flag;
        }
    }
    Some(WinCombo { modifiers, vk: virtual_key(&parsed.key)? })
}

/// GTK key names → virtual-key codes. Punctuation keys (`VK_OEM_*`) are where they are on a US layout.
pub fn virtual_key(key: &str) -> Option<u32> {
    let named = match key.to_ascii_lowercase().as_str() {
        "space" => 0x20,
        "escape" => VK_ESCAPE,
        "return" | "enter" => 0x0D,
        "tab" => 0x09,
        "backspace" => 0x08,
        "insert" => 0x2D,
        "delete" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "page_up" | "prior" => 0x21,
        "page_down" | "next" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "pause" => 0x13,
        "minus" => 0xBD,
        "equal" => 0xBB,
        "comma" => 0xBC,
        "period" => 0xBE,
        "slash" => 0xBF,
        "semicolon" => 0xBA,
        "apostrophe" => 0xDE,
        "bracketleft" => 0xDB,
        "bracketright" => 0xDD,
        "backslash" => 0xDC,
        "grave" => 0xC0,
        _ => 0,
    };
    if named != 0 {
        return Some(named);
    }
    if let Some(n) = key.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        return (1..=24).contains(&n).then_some(0x70 + n - 1);
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_uppercase() as u32),
        (Some(c), None) if c.is_ascii_digit() => Some(c as u32),
        _ => None,
    }
}

/// The left, right and generic virtual keys of each modifier.
const MODIFIER_KEYS: [(u32, [u32; 3]); 4] = [
    (MOD_CONTROL, [0x11, 0xA2, 0xA3]),
    (MOD_ALT, [0x12, 0xA4, 0xA5]),
    (MOD_SHIFT, [0x10, 0xA0, 0xA1]),
    (MOD_WIN, [0x5B, 0x5C, 0x5B]),
];

/// Hold to talk: does releasing `released_vk` end the hold of `combo`? Its main key or any of its modifiers does
/// (the Mac app's release event fires the same way).
pub fn releases(combo: WinCombo, released_vk: u32) -> bool {
    released_vk == combo.vk
        || MODIFIER_KEYS.iter().any(|(flag, keys)| combo.modifiers & flag != 0 && keys.contains(&released_vk))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovt_core::hotkey::{DEFAULT_COMMAND, DEFAULT_DICTATION, DEFAULT_SWAP};

    #[test]
    fn parses_the_defaults() {
        let dictation = win_combo(&Combo::new(DEFAULT_DICTATION)).unwrap();
        assert_eq!(dictation, WinCombo { modifiers: MOD_CONTROL | MOD_ALT, vk: 0x20 });
        let command = win_combo(&Combo::new(DEFAULT_COMMAND)).unwrap();
        assert_eq!(command.modifiers, MOD_CONTROL | MOD_ALT | MOD_SHIFT);
        assert_eq!(win_combo(&Combo::new(DEFAULT_SWAP)).unwrap().vk, b'Z' as u32);
        assert_eq!(win_combo(&Combo::new("<Super>F9")).unwrap(), WinCombo { modifiers: MOD_WIN, vk: 0x78 });
        assert_eq!(win_combo(&Combo::new("<Alt>7")).unwrap().vk, b'7' as u32);
        assert!(win_combo(&Combo::new("<Alt>F25")).is_none());
        assert!(win_combo(&Combo::new("<Alt>dead_acute")).is_none());
        assert!(win_combo(&Combo::new("<Hyper>x")).is_none());
    }

    #[test]
    fn release_of_key_or_modifier() {
        let combo = WinCombo { modifiers: MOD_CONTROL | MOD_ALT, vk: 0x20 };
        assert!(releases(combo, 0x20));
        assert!(releases(combo, 0xA2)); // left Ctrl
        assert!(releases(combo, 0xA5)); // right Alt
        assert!(!releases(combo, 0xA0)); // Shift isn't part of it
        assert!(!releases(combo, b'A' as u32));
    }
}
