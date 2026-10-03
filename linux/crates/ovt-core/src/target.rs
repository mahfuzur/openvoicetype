//! Where a dictation's text should go (follows `PasteTarget.swift`). The app captures it when recording starts and again
//! just before pasting; `compare` decides whether it's still the same place, so text never lands in another app, another
//! window or another chat channel because focus moved meanwhile.
//!
//! Capturing is the desktop backend's job (the GNOME extension, KWin, sway/Hyprland IPC, EWMH, plus AT-SPI for password
//! fields). This module only holds what was captured and the rules.

use crate::mode::{is_code_app, normalize_app_id};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PasteTarget {
    pub pid: u32,
    /// The desktop-file id or window class (see `mode`).
    pub app_id: Option<String>,
    pub app_name: String,
    /// A stable id for the focused window from the backend (the macOS app compares AXUIElements). None when unknown.
    pub window: Option<String>,
    pub window_title: Option<String>,
    /// A password field has focus (AT-SPI `ROLE_PASSWORD_TEXT`). False when unknown: like macOS without
    /// Accessibility, dictation is refused only when it's known.
    pub is_secure: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetCheck {
    Same,
    /// Focus moved; the value names where to, for the overlay ("Copied: you switched to …").
    Changed(String),
    Secure,
}

impl PasteTarget {
    /// Compares with a fresh capture of what has focus now.
    pub fn compare(&self, now: &PasteTarget) -> TargetCheck {
        if now.is_secure {
            return TargetCheck::Secure;
        }
        if now.pid != self.pid {
            return TargetCheck::Changed(now.app_name.clone());
        }
        if let (Some(window), Some(now_window)) = (&self.window, &now.window) {
            if window != now_window {
                return TargetCheck::Changed(format!("another {} window", now.app_name));
            }
        }
        // Terminals and editors retitle themselves while they work (a job name, a spinner, a file name), so there the
        // same window is enough.
        let same_window = self.window.is_some() && now.window.is_some();
        if same_window && self.is_code_app() {
            return TargetCheck::Same;
        }
        if let (Some(title), Some(now_title)) = (&self.window_title, &now.window_title) {
            if normalized(title) != normalized(now_title) {
                return TargetCheck::Changed(if now_title.is_empty() {
                    format!("another {} view", now.app_name)
                } else {
                    format!("\u{201C}{}\u{201D}", shortened(now_title))
                });
            }
        }
        TargetCheck::Same
    }

    /// Editors, terminals and IDEs, where the title isn't a reliable sign of where the text goes.
    pub fn is_code_app(&self) -> bool {
        self.app_id.as_deref().map(normalize_app_id).is_some_and(|id| is_code_app(&id))
    }

    pub fn is_terminal(&self) -> bool {
        self.app_id.as_deref().map(normalize_app_id).is_some_and(|id| crate::mode::is_terminal(&id))
    }
}

/// Unread counts, activity spinners and "Edited" come and go in titles ("(3) Inbox", "• Slack", "⠋ claude",
/// "Notes — Edited"): they don't mean the user moved.
pub fn normalized(title: &str) -> String {
    static PATTERNS: OnceLock<[Regex; 4]> = OnceLock::new();
    let [count, leading, edited, spaces] = PATTERNS.get_or_init(|| {
        [
            Regex::new(r"\(\d+\)").unwrap(),
            Regex::new(r"^[^\p{L}\p{N}#@(\[]+").unwrap(),
            Regex::new(r"\s+[—–-]\s+Edited$").unwrap(),
            Regex::new(r"\s+").unwrap(),
        ]
    });
    let text = count.replace_all(title, "");
    let text = leading.replace_all(&text, "");
    let text = edited.replace_all(&text, "");
    let text = spaces.replace_all(&text, " ");
    text.trim_matches(|c: char| c == ' ' || c == '\t').to_string()
}

fn shortened(title: &str) -> String {
    if title.chars().count() > 40 {
        title.chars().take(40).collect::<String>() + "…"
    } else {
        title.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(app: &str, window: &str, title: &str) -> PasteTarget {
        PasteTarget {
            pid: 42,
            app_id: Some(app.into()),
            app_name: app.into(),
            window: Some(window.into()),
            window_title: Some(title.into()),
            is_secure: false,
        }
    }

    #[test]
    fn normalizes_titles() {
        // The vectors from LogicSelfTest.swift.
        assert_eq!(normalized("(3) Inbox - Gmail"), normalized("Inbox (12) - Gmail"));
        assert_ne!(normalized("#general - Slack"), normalized("#random - Slack"));
        assert_eq!(normalized("⠋ claude"), normalized("✳ claude"));
        assert_eq!(normalized("Notes — Edited"), "Notes");
        assert_eq!(normalized("#general"), "#general");
    }

    #[test]
    fn compares_targets() {
        let slack = target("com.slack.Slack", "w1", "#general - Slack");
        assert_eq!(slack.compare(&slack), TargetCheck::Same);
        assert_eq!(slack.compare(&target("com.slack.Slack", "w1", "(2) #general - Slack")), TargetCheck::Same);
        assert_eq!(
            slack.compare(&target("com.slack.Slack", "w1", "#random - Slack")),
            TargetCheck::Changed("\u{201C}#random - Slack\u{201D}".into())
        );
        assert_eq!(
            slack.compare(&target("com.slack.Slack", "w2", "#general - Slack")),
            TargetCheck::Changed("another com.slack.Slack window".into())
        );
        let mut other = target("firefox", "w1", "x");
        other.pid = 7;
        assert_eq!(slack.compare(&other), TargetCheck::Changed("firefox".into()));
        let mut secure = slack.clone();
        secure.is_secure = true;
        assert_eq!(slack.compare(&secure), TargetCheck::Secure);
    }

    #[test]
    fn code_apps_trust_the_window() {
        let terminal = target("org.gnome.Ptyxis", "w1", "~/src");
        assert_eq!(terminal.compare(&target("org.gnome.Ptyxis", "w1", "claude — building")), TargetCheck::Same);
        assert!(terminal.is_terminal() && terminal.is_code_app());
    }
}
