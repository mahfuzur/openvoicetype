//! Where things live. These must match `scripts/dictate.sh`, which picks the same folders, with the `voice-to-text`
//! folder names the macOS app and the CLI have always used:
//! - Linux: the XDG base directories;
//! - Windows: `%APPDATA%\\voice-to-text` for settings, `%LOCALAPPDATA%\\voice-to-text` for models and logs, and
//!   `%TEMP%\\voice-to-text` for runtime state (docs/plans/M8-windows.md).

use std::path::PathBuf;

/// `config.sh` (the CLI's), `dictionary.txt`, `prompt.txt` and the app's settings.
pub fn config_dir() -> PathBuf {
    platform::config_home().join("voice-to-text")
}

pub fn dictionary_file() -> PathBuf {
    config_dir().join("dictionary.txt")
}

/// The app's settings (the macOS app keeps them in UserDefaults).
pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

/// Where the models go, in `whisper/` and `s1-mini/`, shared with the CLI.
pub fn data_home() -> PathBuf {
    platform::data_home()
}

pub fn whisper_dir() -> PathBuf {
    data_home().join("whisper")
}

pub fn s1_dir() -> PathBuf {
    data_home().join("s1-mini")
}

/// Runtime state: pid files, per-run result and key files, recordings.
pub fn state_dir() -> PathBuf {
    platform::state_dir()
}

/// dictate.log and error.log (macOS: `~/Library/Logs/voice-to-text`).
pub fn log_dir() -> PathBuf {
    platform::log_dir()
}

pub fn log_file() -> PathBuf {
    log_dir().join("dictate.log")
}

#[cfg(unix)]
mod platform {
    use std::path::PathBuf;

    fn home() -> PathBuf {
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
    }

    /// An XDG variable, if set to an absolute path (the spec says to ignore relative ones), else `$HOME/<fallback>`.
    fn xdg(variable: &str, fallback: &str) -> PathBuf {
        match std::env::var_os(variable).map(PathBuf::from) {
            Some(path) if path.is_absolute() => path,
            _ => home().join(fallback),
        }
    }

    pub fn config_home() -> PathBuf {
        xdg("XDG_CONFIG_HOME", ".config")
    }

    /// `~/.local/share`.
    pub fn data_home() -> PathBuf {
        xdg("XDG_DATA_HOME", ".local/share")
    }

    /// `$XDG_RUNTIME_DIR` is private to the user and cleared at logout (macOS uses its per-user `$TMPDIR` for this).
    /// Without a usable one (unset, or another user's), `/tmp/voice-to-text-<uid>`, one level deep like dictate.sh, so
    /// its ownership check covers the folder itself.
    pub fn state_dir() -> PathBuf {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: getuid never fails.
        let uid = unsafe { libc::getuid() };
        match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
            Some(path)
                if path.is_absolute() && std::fs::metadata(&path).is_ok_and(|m| m.is_dir() && m.uid() == uid) =>
            {
                path.join("voice-to-text")
            }
            _ => PathBuf::from(format!("/tmp/voice-to-text-{uid}")),
        }
    }

    pub fn log_dir() -> PathBuf {
        xdg("XDG_STATE_HOME", ".local/state").join("voice-to-text")
    }
}

#[cfg(windows)]
mod platform {
    use std::path::PathBuf;

    /// A folder from the environment, else `%USERPROFILE%\\<fallback>`.
    fn known(variable: &str, fallback: &str) -> PathBuf {
        match std::env::var_os(variable).map(PathBuf::from) {
            Some(path) if path.is_absolute() => path,
            _ => std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(fallback),
        }
    }

    /// `%APPDATA%` (roaming): settings follow the user.
    pub fn config_home() -> PathBuf {
        known("APPDATA", "AppData\\Roaming")
    }

    /// `%LOCALAPPDATA%\\voice-to-text`: models are large and stay on this machine.
    pub fn data_home() -> PathBuf {
        known("LOCALAPPDATA", "AppData\\Local").join("voice-to-text")
    }

    /// `%TEMP%\\voice-to-text`: in the user's own profile (Windows reads `TMP` first, then `TEMP`; both are normally the same).
    pub fn state_dir() -> PathBuf {
        std::env::temp_dir().join("voice-to-text")
    }

    pub fn log_dir() -> PathBuf {
        data_home().join("logs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn follows_the_script() {
        // One test changes the environment, so these checks run in sequence.
        std::env::set_var("HOME", "/home/u");
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::remove_var("XDG_STATE_HOME");
        let runtime = std::env::temp_dir().join(format!("ovt-runtime-{}", std::process::id()));
        std::fs::create_dir_all(&runtime).unwrap();
        std::env::set_var("XDG_RUNTIME_DIR", &runtime);
        assert_eq!(dictionary_file(), PathBuf::from("/home/u/.config/voice-to-text/dictionary.txt"));
        assert_eq!(whisper_dir(), PathBuf::from("/home/u/.local/share/whisper"));
        assert_eq!(state_dir(), runtime.join("voice-to-text"));
        std::env::set_var("XDG_RUNTIME_DIR", "/nonexistent/run");
        let uid = unsafe { libc::getuid() };
        assert_eq!(state_dir(), PathBuf::from(format!("/tmp/voice-to-text-{uid}")));
        std::fs::remove_dir_all(runtime).unwrap();
        assert_eq!(log_file(), PathBuf::from("/home/u/.local/state/voice-to-text/dictate.log"));
        std::env::set_var("XDG_CONFIG_HOME", "relative/ignored");
        assert_eq!(config_dir(), PathBuf::from("/home/u/.config/voice-to-text"));
        std::env::set_var("XDG_CONFIG_HOME", "/cfg");
        assert_eq!(config_dir(), PathBuf::from("/cfg/voice-to-text"));
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    #[cfg(windows)]
    #[test]
    fn windows_folders() {
        std::env::set_var("APPDATA", r"C:\Users\u\AppData\Roaming");
        std::env::set_var("LOCALAPPDATA", r"C:\Users\u\AppData\Local");
        assert_eq!(dictionary_file(), PathBuf::from(r"C:\Users\u\AppData\Roaming\voice-to-text\dictionary.txt"));
        assert_eq!(whisper_dir(), PathBuf::from(r"C:\Users\u\AppData\Local\voice-to-text\whisper"));
        assert_eq!(log_file(), PathBuf::from(r"C:\Users\u\AppData\Local\voice-to-text\logs\dictate.log"));
        assert!(state_dir().ends_with("voice-to-text"));
    }
}
