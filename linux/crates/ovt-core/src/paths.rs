//! Where things live on Linux. These must match `scripts/dictate.sh`, which picks the same folders: the XDG base
//! directories, with the `voice-to-text` folder names the macOS app and the CLI have always used.

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

/// `~/.local/share`: the models go in `whisper/` and `s1-mini/`, shared with the CLI.
pub fn data_home() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

pub fn whisper_dir() -> PathBuf {
    data_home().join("whisper")
}

pub fn s1_dir() -> PathBuf {
    data_home().join("s1-mini")
}

/// Runtime state: pid files, per-run result and key files, recordings. `$XDG_RUNTIME_DIR` is private to the user and
/// cleared at logout (macOS uses its per-user `$TMPDIR` for this).
pub fn state_dir() -> PathBuf {
    let runtime = match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        // SAFETY: getuid never fails.
        _ => PathBuf::from(format!("/tmp/voice-to-text-{}", unsafe { libc::getuid() })),
    };
    runtime.join("voice-to-text")
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
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        assert_eq!(dictionary_file(), PathBuf::from("/home/u/.config/voice-to-text/dictionary.txt"));
        assert_eq!(whisper_dir(), PathBuf::from("/home/u/.local/share/whisper"));
        assert_eq!(state_dir(), PathBuf::from("/run/user/1000/voice-to-text"));
        assert_eq!(log_file(), PathBuf::from("/home/u/.local/state/voice-to-text/dictate.log"));
        std::env::set_var("XDG_CONFIG_HOME", "relative/ignored");
        assert_eq!(config_dir(), PathBuf::from("/home/u/.config/voice-to-text"));
        std::env::set_var("XDG_CONFIG_HOME", "/cfg");
        assert_eq!(config_dir(), PathBuf::from("/cfg/voice-to-text"));
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}
