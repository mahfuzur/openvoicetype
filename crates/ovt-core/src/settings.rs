//! Every user setting (follows `AppSettings.swift`), with the same names and defaults, stored as JSON in
//! `~/.config/voice-to-text/settings.json`. Unknown or missing keys take their default, and so does a value this build
//! can't read (a newer build's mode, a wrong type), key by key, so an older or newer file loads.

use crate::hotkey::{self, Combo};
use crate::mode::DictationMode;
use crate::models;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    Bottom,
    Top,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub hot_key: Combo,
    /// Hold the hotkey while speaking and release it to finish. Off = press to start and to stop.
    pub hold_to_talk: bool,
    /// Swaps the last paste between the cleaned text and Whisper's text.
    pub swap_hot_key: Combo,
    /// Command Mode: select text, press it, and say how to change it.
    pub command_hot_key: Combo,
    /// "claude" or "openai". Never S1-mini (it can't follow instructions).
    pub command_engine: String,

    pub refine: bool,
    /// "haiku" or "sonnet".
    pub claude_model: String,
    /// "claude", "s1" (S1-mini, fully offline) or "openai" (an OpenAI-compatible endpoint).
    pub cleanup_engine: String,
    pub s1_fallback: bool,
    /// Its key is in the Secret Service, per host.
    pub openai_base_url: String,
    pub openai_model: String,

    pub auto_paste: bool,
    pub rich_paste: bool,
    pub show_overlay: bool,
    pub overlay_position: OverlayPosition,
    pub play_sounds: bool,

    /// None = Auto (choose from the focused app).
    pub mode_override: Option<DictationMode>,
    /// The user's own app → mode choices (normalized app id → mode), checked before the built-in lists.
    pub app_modes: HashMap<String, DictationMode>,
    /// Display names for `app_modes`.
    pub app_names: HashMap<String, String>,

    /// None = the system default input. A PulseAudio/PipeWire source name.
    pub input_device: Option<String>,
    pub input_device_name: Option<String>,

    /// The Whisper model's file name in ~/.local/share/whisper.
    pub whisper_model: String,
    pub setup_completed: bool,
    pub check_for_updates: bool,
    /// Dictated text in the log, for debugging. Off by default: the log keeps timings and outcomes only.
    pub log_text: bool,
    pub launch_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hot_key: Combo::new(hotkey::DEFAULT_DICTATION),
            hold_to_talk: false,
            swap_hot_key: Combo::new(hotkey::DEFAULT_SWAP),
            command_hot_key: Combo::new(hotkey::DEFAULT_COMMAND),
            command_engine: "claude".into(),
            refine: true,
            claude_model: "haiku".into(),
            cleanup_engine: "claude".into(),
            s1_fallback: true,
            openai_base_url: "http://localhost:11434/v1".into(),
            openai_model: String::new(),
            auto_paste: true,
            rich_paste: true,
            show_overlay: true,
            overlay_position: OverlayPosition::Bottom,
            play_sounds: true,
            mode_override: None,
            app_modes: HashMap::new(),
            app_names: HashMap::new(),
            input_device: None,
            input_device_name: None,
            // Existing CLI installs keep the full model they already have; new ones default to the compressed one.
            whisper_model: if models::FULL_WHISPER.is_installed() {
                models::FULL_WHISPER.file_name.into()
            } else {
                models::DEFAULT_WHISPER.file_name.into()
            },
            // Anyone with a Whisper model already set things up with install.sh.
            setup_completed: models::WHISPER.iter().any(|m| m.is_installed()),
            check_for_updates: true,
            log_text: false,
            launch_at_login: false,
        }
    }
}

/// The name an API key is stored under in the Secret Service: the endpoint's host and explicit port, so an OpenAI key and
/// a Groq key can both be kept (follows `APIKeychain.account`). The service is "OpenVoiceType cleanup API key", as on macOS.
pub fn api_key_account(base_url: &str) -> String {
    let trimmed = base_url.trim();
    let Some((_, rest)) = trimmed.split_once("://") else { return base_url.to_string() };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, after) = bracketed.split_once(']').unwrap_or((bracketed, ""));
        (host, after.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return base_url.to_string();
    }
    match port.filter(|p| !p.is_empty()) {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

impl Settings {
    pub fn uses_s1(&self) -> bool {
        self.refine && self.cleanup_engine == "s1"
    }

    pub fn uses_api(&self) -> bool {
        self.refine && self.cleanup_engine == "openai"
    }

    /// Loads the file; a missing or broken file gives the defaults (a broken one is kept as settings.json.bad).
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else { return Self::default() };
        Self::from_json(&text).unwrap_or_else(|| {
            let _ = std::fs::rename(path, path.with_extension("json.bad"));
            Self::default()
        })
    }

    /// Reads each key on its own, like `AppSettings` does: a value that doesn't decode keeps its default, and in a map
    /// (`appModes`) only the entries that don't decode are dropped. None if the text isn't a JSON object.
    pub fn from_json(text: &str) -> Option<Self> {
        let serde_json::Value::Object(file) = serde_json::from_str(text).ok()? else { return None };
        let mut merged = serde_json::to_value(Self::default()).ok()?;
        let decodes = |value: &serde_json::Value| serde_json::from_value::<Self>(value.clone()).is_ok();
        for (key, value) in file {
            let mut candidate = merged.clone();
            candidate[&key] = value.clone();
            if decodes(&candidate) {
                merged = candidate;
            } else if let serde_json::Value::Object(entries) = value {
                for (name, entry) in entries {
                    let mut candidate = merged.clone();
                    candidate[&key][&name] = entry;
                    if decodes(&candidate) {
                        merged = candidate;
                    }
                }
            }
        }
        serde_json::from_value(merged).ok()
    }

    /// Writes atomically, readable by the user only (Windows: %APPDATA% is the user's own).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temporary = path.with_extension("json.tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(serde_json::to_string_pretty(self)?.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(temporary, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_mac() {
        let s = Settings::default();
        assert_eq!(s.hot_key.0, "<Control><Alt>space");
        assert!(s.refine && s.s1_fallback && s.auto_paste && s.rich_paste && s.show_overlay && s.play_sounds);
        assert!(!s.log_text && !s.hold_to_talk);
        assert_eq!((s.claude_model.as_str(), s.cleanup_engine.as_str()), ("haiku", "claude"));
        assert_eq!(s.openai_base_url, "http://localhost:11434/v1");
    }

    #[test]
    fn api_key_accounts() {
        // The cases from LogicSelfTest.swift, plus IPv6 and credentials in the URL.
        assert_eq!(api_key_account("http://localhost:11434/v1"), "localhost:11434");
        assert_eq!(api_key_account("https://api.openai.com/v1"), "api.openai.com");
        assert_eq!(api_key_account(" https://user:pw@api.groq.com/openai/v1 "), "api.groq.com");
        assert_eq!(api_key_account("http://[::1]:8080/v1"), "::1:8080");
        assert_eq!(api_key_account("not a url"), "not a url");
    }

    #[test]
    fn partial_files_load() {
        let s: Settings = serde_json::from_str(r#"{"cleanupEngine":"s1","modeOverride":"chat","unknown":1}"#).unwrap();
        assert!(s.uses_s1());
        assert_eq!(s.mode_override, Some(DictationMode::Chat));
        assert_eq!(s.claude_model, "haiku");
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"hotKey\":\"<Control><Alt>space\""));
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
    }

    #[test]
    fn unreadable_values_keep_their_default() {
        // A newer build's mode, an unknown overlay position and a wrong type lose only themselves.
        let s = Settings::from_json(
            r#"{"cleanupEngine":"s1","modeOverride":"poetry","overlayPosition":"left","refine":"on",
                "appModes":{"code":"code","firefox":"poetry"}}"#,
        )
        .unwrap();
        assert_eq!(s.cleanup_engine, "s1");
        assert_eq!(s.mode_override, None);
        assert_eq!(s.overlay_position, OverlayPosition::Bottom);
        assert!(s.refine);
        assert_eq!(s.app_modes, HashMap::from([("code".to_string(), DictationMode::Code)]));
        assert!(Settings::from_json("[1]").is_none() && Settings::from_json("{broken").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn saves_privately() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ovt-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let s = Settings { log_text: true, ..Default::default() };
        s.save(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(Settings::load(&path), s);
        std::fs::write(&path, "{broken").unwrap();
        assert!(!Settings::load(&path).log_text);
        assert!(dir.join("settings.json.bad").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
