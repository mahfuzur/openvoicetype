//! The notification-area icon and its short menu (follows `MenuBarIcon.swift` and `AppDelegate.menuNeedsUpdate`):
//! a status line, Mode, Cleanup engine, Copy Last Dictation, Settings…, Open Log, Quit.

pub mod icon;

#[cfg(windows)]
mod menu;
#[cfg(windows)]
pub use menu::Tray;

use ovt_core::mode::DictationMode;

/// What a menu item does. Its menu id is `id()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// None = Auto (from the app in front).
    Mode(Option<DictationMode>),
    /// claude, s1 or openai.
    Engine(String),
    CopyLast,
    Settings,
    OpenLog,
    Quit,
}

/// The engines the menu offers: settings value and title.
pub const ENGINES: [(&str, &str); 3] = [("claude", "Claude"), ("s1", "S1-mini (offline)"), ("openai", "API")];

impl Action {
    pub fn id(&self) -> String {
        match self {
            Action::Mode(None) => "mode:auto".into(),
            Action::Mode(Some(mode)) => format!("mode:{}", mode.as_str()),
            Action::Engine(engine) => format!("engine:{engine}"),
            Action::CopyLast => "copy-last".into(),
            Action::Settings => "settings".into(),
            Action::OpenLog => "open-log".into(),
            Action::Quit => "quit".into(),
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        if let Some(mode) = id.strip_prefix("mode:") {
            return if mode == "auto" {
                Some(Action::Mode(None))
            } else {
                DictationMode::parse(mode).map(|m| Action::Mode(Some(m)))
            };
        }
        if let Some(engine) = id.strip_prefix("engine:") {
            return ENGINES.iter().any(|(value, _)| *value == engine).then(|| Action::Engine(engine.into()));
        }
        match id {
            "copy-last" => Some(Action::CopyLast),
            "settings" => Some(Action::Settings),
            "open-log" => Some(Action::OpenLog),
            "quit" => Some(Action::Quit),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        let mut actions = vec![Action::Mode(None), Action::CopyLast, Action::Settings, Action::OpenLog, Action::Quit];
        actions.extend(DictationMode::ALL.map(|mode| Action::Mode(Some(mode))));
        actions.extend(ENGINES.map(|(engine, _)| Action::Engine(engine.into())));
        for action in actions {
            assert_eq!(Action::from_id(&action.id()), Some(action));
        }
        assert_eq!(Action::from_id("mode:poetry"), None);
        assert_eq!(Action::from_id("engine:gpt"), None);
    }
}
