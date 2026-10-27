//! Where things live on Linux. These must match `scripts/dictate.sh`, which picks the same folders: the XDG base
//! directories, with the `voice-to-text` folder names the macOS app and the CLI have always used, and the app's own
//! `openvoicetype` folder for the models (`app_data_dir` also names the macOS and Windows ones).

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// An XDG variable, if set to an absolute path (the spec says to ignore relative ones), else `$HOME/<fallback>`.
fn xdg(variable: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(variable).map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        _ => home().join(fallback),
    }
}

/// `~/.config/voice-to-text`: config.sh (the CLI's), dictionary.txt, prompt.txt and the app's settings.
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("voice-to-text")
}

pub fn dictionary_file() -> PathBuf {
    config_dir().join("dictionary.txt")
}

/// The app's settings (the macOS app keeps them in UserDefaults).
pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

/// `~/.local/share`. Up to v0.5 the models were in its `whisper/` and `s1-mini/` (see `models::move_old_models`).
pub fn data_home() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

/// The app's own data folder, the same on every platform as in `ModelManager.swift`, `dictate.sh` and `install.sh`.
pub fn app_data_dir() -> PathBuf {
    app_data_dir_on(std::env::consts::OS)
}

fn app_data_dir_on(os: &str) -> PathBuf {
    match os {
        "macos" => home().join("Library/Application Support/OpenVoiceType"),
        "windows" => match std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            Some(path) if path.is_absolute() => path,
            _ => home().join("AppData").join("Local"),
        }
        .join("OpenVoiceType"),
        _ => data_home().join("openvoicetype"),
    }
}

/// The downloaded models, shared with the CLI: `whisper/` and `s1-mini/`.
pub fn models_dir() -> PathBuf {
    app_data_dir().join("models")
}

pub fn whisper_dir() -> PathBuf {
    models_dir().join("whisper")
}

pub fn s1_dir() -> PathBuf {
    models_dir().join("s1-mini")
}

/// Runtime state: pid files, per-run result and key files, recordings. `$XDG_RUNTIME_DIR` is private to the user and
/// cleared at logout (macOS uses its per-user `$TMPDIR` for this). Without a usable one (unset, or another user's),
/// `/tmp/voice-to-text-<uid>`, one level deep like dictate.sh, so its ownership check covers the folder itself.
pub fn state_dir() -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: getuid never fails.
    let uid = unsafe { libc::getuid() };
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(path) if path.is_absolute() && std::fs::metadata(&path).is_ok_and(|m| m.is_dir() && m.uid() == uid) => {
            path.join("voice-to-text")
        }
        _ => PathBuf::from(format!("/tmp/voice-to-text-{uid}")),
    }
}

/// dictate.log and error.log (macOS: `~/Library/Logs/voice-to-text`).
pub fn log_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("voice-to-text")
}

pub fn log_file() -> PathBuf {
    log_dir().join("dictate.log")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(whisper_dir(), PathBuf::from("/home/u/.local/share/openvoicetype/models/whisper"));
        assert_eq!(app_data_dir_on("macos"), PathBuf::from("/home/u/Library/Application Support/OpenVoiceType"));
        std::env::set_var("LOCALAPPDATA", "/c/Users/u/AppData/Local");
        assert_eq!(app_data_dir_on("windows"), PathBuf::from("/c/Users/u/AppData/Local/OpenVoiceType"));
        std::env::remove_var("LOCALAPPDATA");
        assert_eq!(app_data_dir_on("windows"), PathBuf::from("/home/u/AppData/Local/OpenVoiceType"));
        std::env::set_var("XDG_DATA_HOME", "/data");
        assert_eq!(s1_dir(), PathBuf::from("/data/openvoicetype/models/s1-mini"));
        std::env::remove_var("XDG_DATA_HOME");
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
}
