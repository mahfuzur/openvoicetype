//! Writing modes and the app → mode mapping (follows `Modes.swift`). Each mode is `prompts/modes/<name>.md`.
//!
//! Linux names apps by their desktop-file id (`org.gnome.Ptyxis`, `com.slack.Slack` for a Flatpak) or, without one, by
//! their X11/Wayland class (`code`, `slack`). Both are compared lower-cased and without `.desktop`, and the tables list
//! the native and the Flatpak/Snap names.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DictationMode {
    Default,
    Chat,
    Email,
    Code,
    Notes,
    Raw,
}

impl DictationMode {
    pub const ALL: [DictationMode; 6] = [Self::Default, Self::Chat, Self::Email, Self::Code, Self::Notes, Self::Raw];

    /// The value dictate.sh takes in `VTT_MODE`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Chat => "chat",
            Self::Email => "email",
            Self::Code => "code",
            Self::Notes => "notes",
            Self::Raw => "raw",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.as_str() == value)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Chat => "Chat",
            Self::Email => "Email",
            Self::Code => "Code",
            Self::Notes => "Notes",
            Self::Raw => "Raw (no cleanup)",
        }
    }

    /// Picks the mode for the app that will receive the text: the user's own choice (Settings → Modes) first, then the
    /// built-in lists. Browsers stay `Default`: we can't tell which web app is open.
    pub fn for_app(app_id: Option<&str>, custom: &HashMap<String, DictationMode>) -> Self {
        let Some(app_id) = app_id else { return Self::Default };
        let id = normalize_app_id(app_id);
        if let Some(mode) = custom.get(&id).or_else(|| custom.get(app_id)) {
            return *mode;
        }
        if CHAT_APPS.contains(&id.as_str()) {
            Self::Chat
        } else if EMAIL_APPS.contains(&id.as_str()) {
            Self::Email
        } else if is_code_app(&id) {
            Self::Code
        } else if NOTES_APPS.contains(&id.as_str()) {
            Self::Notes
        } else {
            Self::Default
        }
    }
}

/// `org.gnome.Ptyxis.desktop` → `org.gnome.ptyxis`.
pub fn normalize_app_id(app_id: &str) -> String {
    let id = app_id.trim().to_lowercase();
    id.strip_suffix(".desktop").map(str::to_string).unwrap_or(id)
}

/// Editors, IDEs and terminals: identifiers stay exact, no Markdown, and the paste-target check trusts the window
/// over its title (these retitle themselves while they work).
pub fn is_code_app(normalized_id: &str) -> bool {
    CODE_APPS.contains(&normalized_id) || is_terminal(normalized_id) || is_jetbrains(normalized_id)
}

/// Terminals: paste with Ctrl+Shift+V, never send Ctrl+C or Ctrl+Z (they interrupt and suspend the program), and
/// Command Mode answers go to the clipboard (follows `SelectionReader.terminals`).
pub fn is_terminal(normalized_id: &str) -> bool {
    TERMINALS.contains(&normalized_id)
}

/// Editors whose Ctrl+C copies the whole line when nothing is selected (follows `SelectionReader.lineCopyEditors`).
pub fn copies_line_without_selection(normalized_id: &str) -> bool {
    LINE_COPY_EDITORS.contains(&normalized_id) || is_jetbrains(normalized_id)
}

fn is_jetbrains(id: &str) -> bool {
    id.starts_with("jetbrains-") || id.starts_with("com.jetbrains.")
}

const CHAT_APPS: &[&str] = &[
    "slack",
    "com.slack.slack",
    "discord",
    "com.discordapp.discord",
    "vesktop",
    "dev.vencord.vesktop",
    "teams-for-linux",
    "com.github.ismaelmartinez.teams_for_linux",
    "org.telegram.desktop",
    "telegram-desktop",
    "signal",
    "signal-desktop",
    "org.signal.signal",
    "element",
    "element-desktop",
    "im.riot.riot",
    "whatsapp-for-linux",
    "com.github.eneshecan.whatsappfordesktop",
    "zapzap",
    "com.rtosta.zapzap",
    "skypeforlinux",
    "com.skype.client",
    "org.gnome.fractal",
    "org.gnome.polari",
];

const EMAIL_APPS: &[&str] = &[
    "thunderbird",
    "org.mozilla.thunderbird",
    "net.thunderbird.thunderbird",
    "org.gnome.evolution",
    "evolution",
    "org.gnome.geary",
    "geary",
    "org.kde.kmail2",
    "kmail",
    "mailspring",
    "com.getmailspring.mailspring",
    "betterbird",
    "eu.betterbird.betterbird",
];

const CODE_APPS: &[&str] = &[
    "code",
    "code-oss",
    "code-url-handler",
    "com.visualstudio.code",
    "code-insiders",
    "com.visualstudio.code.insiders",
    "codium",
    "com.vscodium.codium",
    "vscodium",
    "cursor",
    "windsurf",
    "dev.zed.zed",
    "zed",
    "sublime_text",
    "com.sublimetext.three",
    "org.gnome.builder",
    "org.kde.kate",
    "org.kde.kdevelop",
    "neovide",
];

const TERMINALS: &[&str] = &[
    "org.gnome.terminal",
    "gnome-terminal-server",
    "org.gnome.ptyxis",
    "org.gnome.console",
    "kgx",
    "org.kde.konsole",
    "konsole",
    "kitty",
    "alacritty",
    "org.alacritty.alacritty",
    "com.mitchellh.ghostty",
    "ghostty",
    "org.wezfurlong.wezterm",
    "wezterm",
    "xterm",
    "uxterm",
    "tilix",
    "com.gexperts.tilix",
    "terminator",
    "foot",
    "footclient",
    "xfce4-terminal",
    "dev.warp.warp",
    "com.raggesilver.blackbox",
    "guake",
    "yakuake",
    "io.elementary.terminal",
];

const LINE_COPY_EDITORS: &[&str] = &[
    "code",
    "code-oss",
    "code-url-handler",
    "com.visualstudio.code",
    "code-insiders",
    "com.visualstudio.code.insiders",
    "codium",
    "com.vscodium.codium",
    "vscodium",
    "cursor",
    "windsurf",
    "dev.zed.zed",
    "zed",
    "sublime_text",
    "com.sublimetext.three",
];

const NOTES_APPS: &[&str] = &[
    "obsidian",
    "md.obsidian.obsidian",
    "notion-app",
    "notion-snap-reborn",
    "logseq",
    "com.logseq.logseq",
    "joplin",
    "net.cozic.joplin_desktop",
    "org.gnome.texteditor",
    "org.gnome.gedit",
    "gedit",
    "org.kde.kwrite",
    "libreoffice-writer",
    "org.libreoffice.libreoffice.writer",
    "org.gnome.notes",
    "com.github.flxzt.rnote",
    "org.standardnotes.standardnotes",
    "anytype",
    "io.anytype.anytype",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_apps() {
        let none = HashMap::new();
        assert_eq!(DictationMode::for_app(Some("com.slack.Slack.desktop"), &none), DictationMode::Chat);
        assert_eq!(DictationMode::for_app(Some("org.mozilla.Thunderbird"), &none), DictationMode::Email);
        assert_eq!(DictationMode::for_app(Some("org.gnome.Ptyxis"), &none), DictationMode::Code);
        assert_eq!(DictationMode::for_app(Some("jetbrains-idea"), &none), DictationMode::Code);
        assert_eq!(DictationMode::for_app(Some("md.obsidian.Obsidian"), &none), DictationMode::Notes);
        assert_eq!(DictationMode::for_app(Some("firefox"), &none), DictationMode::Default);
        assert_eq!(DictationMode::for_app(None, &none), DictationMode::Default);
        let custom = HashMap::from([("firefox".to_string(), DictationMode::Email)]);
        assert_eq!(DictationMode::for_app(Some("Firefox.desktop"), &custom), DictationMode::Email);
    }

    #[test]
    fn terminals_and_line_copy() {
        assert!(is_terminal("org.gnome.ptyxis") && is_terminal("kitty") && !is_terminal("code"));
        assert!(copies_line_without_selection("code") && copies_line_without_selection("jetbrains-pycharm"));
        assert!(!copies_line_without_selection("org.gnome.texteditor"));
    }

    #[test]
    fn parses_mode_names() {
        for mode in DictationMode::ALL {
            assert_eq!(DictationMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(serde_json::to_string(&DictationMode::Chat).unwrap(), "\"chat\"");
    }
}
