//! Global hotkeys, stored as GTK accelerator strings (`<Control><Alt>space`), which the GNOME extension, GTK and the
//! portals all understand (follows `HotKey.swift`, where combos are Carbon key codes).
//!
//! The rule mirrors macOS: there a hotkey needs ⌃ or ⌥, because ⌘ shortcuts belong to every app. On Linux, Ctrl is the
//! app-shortcut key (Ctrl+V, Ctrl+Q), so a hotkey needs Alt or Super; a function key alone is fine too.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Combo(pub String);

pub const DEFAULT_DICTATION: &str = "<Control><Alt>space";
pub const DEFAULT_COMMAND: &str = "<Control><Alt><Shift>space";
pub const DEFAULT_SWAP: &str = "<Control><Alt>z";
/// Registered only while recording, to cancel.
pub const ESCAPE: &str = "Escape";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_: bool,
    /// The key name (`space`, `z`, `F5`), as GTK spells it.
    pub key: String,
}

impl Combo {
    pub fn new(accelerator: &str) -> Self {
        Combo(accelerator.to_string())
    }

    pub fn parse(&self) -> Option<Parsed> {
        let mut parsed = Parsed::default();
        let mut rest = self.0.trim();
        while let Some(stripped) = rest.strip_prefix('<') {
            let end = stripped.find('>')?;
            match stripped[..end].to_ascii_lowercase().as_str() {
                "control" | "ctrl" | "primary" => parsed.control = true,
                "alt" | "mod1" => parsed.alt = true,
                "shift" => parsed.shift = true,
                "super" | "meta" | "mod4" => parsed.super_ = true,
                _ => return None,
            }
            rest = &stripped[end + 1..];
        }
        if rest.is_empty() || rest.contains(['<', '>', ' ']) {
            return None;
        }
        parsed.key = rest.to_string();
        Some(parsed)
    }

    /// Can this be a global hotkey? (See the module note.)
    pub fn is_valid_global(&self) -> bool {
        self.parse().is_some_and(|p| p.alt || p.super_ || is_function_key(&p.key))
    }

    /// For menus and the overlay: "Ctrl+Alt+Space".
    pub fn label(&self) -> String {
        let Some(p) = self.parse() else { return self.0.clone() };
        let mut parts: Vec<String> = Vec::new();
        if p.control {
            parts.push("Ctrl".into());
        }
        if p.alt {
            parts.push("Alt".into());
        }
        if p.shift {
            parts.push("Shift".into());
        }
        if p.super_ {
            parts.push("Super".into());
        }
        let key = match p.key.as_str() {
            "space" => "Space".to_string(),
            "Escape" => "Esc".to_string(),
            "Return" => "Enter".to_string(),
            key if key.chars().count() == 1 => key.to_uppercase(),
            key => key.to_string(),
        };
        parts.push(key);
        parts.join("+")
    }
}

fn is_function_key(key: &str) -> bool {
    key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=24).contains(&n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_labels() {
        assert_eq!(Combo::new(DEFAULT_DICTATION).label(), "Ctrl+Alt+Space");
        assert_eq!(Combo::new(DEFAULT_COMMAND).label(), "Ctrl+Alt+Shift+Space");
        assert_eq!(Combo::new(DEFAULT_SWAP).label(), "Ctrl+Alt+Z");
        assert_eq!(
            Combo::new("<Super>F9").parse().unwrap(),
            Parsed { super_: true, key: "F9".into(), ..Default::default() }
        );
        assert!(Combo::new("<Hyper>x").parse().is_none());
        assert!(Combo::new("<Control>").parse().is_none());
    }

    #[test]
    fn global_hotkey_rule() {
        assert!(Combo::new(DEFAULT_DICTATION).is_valid_global());
        assert!(Combo::new("<Super>d").is_valid_global());
        assert!(Combo::new("F8").is_valid_global());
        assert!(!Combo::new("<Control>v").is_valid_global()); // every app's paste
        assert!(!Combo::new("<Control><Shift>space").is_valid_global());
        assert!(!Combo::new("d").is_valid_global());
    }
}
