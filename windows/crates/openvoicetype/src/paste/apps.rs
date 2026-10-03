//! Windows apps by their executable: the app id the rest of the code uses (`Code.exe` → `code`), the writing mode for
//! the app (follows `Modes.swift`), and the terminals that paste with Shift+Insert (follows `SelectionReader.terminals`).
//!
//! `ovt_core::mode`'s tables hold Linux and macOS names; the Windows names live here until they move there.

use ovt_core::mode::DictationMode;
use std::collections::HashMap;

/// `C:\…\Code.exe` or `Code.exe` → `code`: the file name, lower-cased, without `.exe`. The key of `appModes` too.
pub fn app_id(exe: &str) -> String {
    let name = exe.rsplit(['\\', '/']).next().unwrap_or(exe).to_lowercase();
    name.strip_suffix(".exe").map(str::to_string).unwrap_or(name)
}

/// The mode for the app that will receive the text: the user's own choice first, then the Windows table, then the
/// shared tables (which know `code`, `slack`, `thunderbird`, `obsidian`…).
pub fn mode_for(exe: &str, custom: &HashMap<String, DictationMode>) -> DictationMode {
    let id = app_id(exe);
    if let Some(mode) = custom.get(&id) {
        return *mode;
    }
    windows_mode(&id).unwrap_or_else(|| DictationMode::for_app(Some(&id), custom))
}

fn windows_mode(id: &str) -> Option<DictationMode> {
    if CHAT.contains(&id) {
        Some(DictationMode::Chat)
    } else if EMAIL.contains(&id) {
        Some(DictationMode::Email)
    } else if is_code(id) {
        Some(DictationMode::Code)
    } else if NOTES.contains(&id) {
        Some(DictationMode::Notes)
    } else {
        None
    }
}

/// Editors, IDEs and terminals: the paste-target check trusts the window over its title there (they retitle
/// themselves while they work), and rich text is never offered.
pub fn is_code(id: &str) -> bool {
    CODE.contains(&id) || TERMINALS.contains(&id) || is_jetbrains(id) || ovt_core::mode::is_code_app(id)
}

/// Terminals take Shift+Insert (Ctrl+V is a key the shell or program may use) and never get Ctrl+C or Ctrl+Z.
/// `window_class` catches consoles whatever process owns them (`ConsoleWindowClass`, Windows Terminal's).
pub fn is_terminal(id: &str, window_class: &str) -> bool {
    TERMINALS.contains(&id) || TERMINAL_CLASSES.contains(&window_class)
}

/// JetBrains IDEs: `idea64`, `pycharm64`, `rider64`… (the 64-bit launchers).
fn is_jetbrains(id: &str) -> bool {
    const IDES: &[&str] = &[
        "idea",
        "pycharm",
        "webstorm",
        "goland",
        "clion",
        "rider",
        "phpstorm",
        "rubymine",
        "datagrip",
        "rustrover",
        "studio",
        "dataspell",
        "aqua",
        "fleet",
    ];
    id.strip_suffix("64").is_some_and(|name| IDES.contains(&name))
}

const CHAT: &[&str] = &[
    "slack",
    "discord",
    "teams",
    "ms-teams",
    "whatsapp",
    "telegram",
    "signal",
    "element",
    "skype",
    "zoom",
    "messenger",
];

const EMAIL: &[&str] = &["outlook", "olk", "hxoutlook", "thunderbird", "mailbird", "mailclient", "spark"];

const CODE: &[&str] =
    &["code", "code - insiders", "vscodium", "cursor", "windsurf", "zed", "devenv", "notepad++", "sublime_text"];

const TERMINALS: &[&str] = &[
    "windowsterminal",
    "openconsole",
    "conhost",
    "cmd",
    "powershell",
    "pwsh",
    "powershell_ise",
    "wsl",
    "wslhost",
    "bash",
    "mintty",
    "wezterm-gui",
    "alacritty",
    "hyper",
    "tabby",
    "putty",
    "kitty",
    "warp",
];

const TERMINAL_CLASSES: &[&str] = &["ConsoleWindowClass", "CASCADIA_HOSTING_WINDOW_CLASS", "PuTTY", "mintty"];

const NOTES: &[&str] =
    &["obsidian", "notion", "onenote", "onenoteim", "logseq", "joplin", "notepad", "winword", "wordpad"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_from_paths() {
        assert_eq!(app_id(r"C:\Users\u\AppData\Local\Programs\Microsoft VS Code\Code.exe"), "code");
        assert_eq!(app_id("WINWORD.EXE"), "winword");
        assert_eq!(app_id("/mnt/c/Windows/notepad.exe"), "notepad");
        assert_eq!(app_id("slack"), "slack");
    }

    #[test]
    fn modes() {
        let none = HashMap::new();
        assert_eq!(mode_for("Slack.exe", &none), DictationMode::Chat);
        assert_eq!(mode_for("ms-teams.exe", &none), DictationMode::Chat);
        assert_eq!(mode_for("OUTLOOK.EXE", &none), DictationMode::Email);
        assert_eq!(mode_for("Code.exe", &none), DictationMode::Code);
        assert_eq!(mode_for("WindowsTerminal.exe", &none), DictationMode::Code);
        assert_eq!(mode_for("pycharm64.exe", &none), DictationMode::Code);
        assert_eq!(mode_for("WINWORD.EXE", &none), DictationMode::Notes);
        assert_eq!(mode_for("chrome.exe", &none), DictationMode::Default);
        let custom = HashMap::from([("chrome".to_string(), DictationMode::Email)]);
        assert_eq!(mode_for("chrome.exe", &custom), DictationMode::Email);
    }

    #[test]
    fn terminals() {
        assert!(is_terminal("windowsterminal", "CASCADIA_HOSTING_WINDOW_CLASS"));
        assert!(is_terminal("cmd", ""));
        assert!(is_terminal("someshell", "ConsoleWindowClass"));
        assert!(is_terminal("mintty", ""));
        assert!(!is_terminal("code", "Chrome_WidgetWin_1"));
        assert!(!is_terminal("notepad", "Notepad"));
        assert!(is_code("pwsh") && is_code("rider64") && !is_code("chrome"));
    }
}
