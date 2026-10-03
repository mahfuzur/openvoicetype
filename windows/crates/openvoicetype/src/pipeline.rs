//! The app's side of `ovt-pipeline` (follows `Dictation.environment` and `ClaudeCLI.swift`): the user's settings as a
//! pipeline `Config`, where the helpers and models are, and where `claude.exe` is. Pure, so it's tested on every platform.

use ovt_core::mode::DictationMode;
use ovt_core::settings::Settings;
use ovt_pipeline::config::Config;
use ovt_pipeline::servers::{Kind, Spec};
use std::path::{Path, PathBuf};

/// `WHISPER_PORT`, as in dictate.sh (S1-mini's port is in the pipeline's `Config`).
pub const WHISPER_PORT: u16 = 8179;

/// `helpers\` next to OpenVoiceType.exe: whisper-server.exe, whisper-cli.exe, llama-server.exe and their DLLs.
pub fn helpers_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent().map(|dir| dir.join("helpers")).unwrap_or_else(|| PathBuf::from("helpers"))
}

/// The chosen Whisper model in `%LOCALAPPDATA%\voice-to-text\whisper`.
pub fn whisper_model(settings: &Settings) -> PathBuf {
    ovt_core::paths::whisper_dir().join(&settings.whisper_model)
}

pub fn whisper_spec(settings: &Settings, helpers: &Path) -> Spec {
    Spec {
        kind: Kind::Whisper,
        binary: helpers.join("whisper-server.exe"),
        model: whisper_model(settings),
        port: WHISPER_PORT,
        language: "en".into(),
    }
}

pub fn s1_spec(config: &Config, helpers: &Path) -> Spec {
    Spec {
        kind: Kind::S1,
        binary: helpers.join("llama-server.exe"),
        model: config.s1_model.clone(),
        port: config.s1_port,
        language: "en".into(),
    }
}

/// One dictation's pipeline settings: `VTT_*`/plain environment overrides first (as dictate.sh reads them, for
/// testing: `VTT_OFFLINE=on`, `CLAUDE_TIMEOUT`…), then the user's settings, the target's mode and name, and the key.
pub fn config(
    base: Config,
    settings: &Settings,
    mode: DictationMode,
    app_name: &str,
    api_key: Option<String>,
) -> Config {
    Config {
        mode: mode.as_str().into(),
        app_name: app_name.into(),
        refine: settings.refine,
        cleanup: settings.cleanup_engine.clone(),
        command_engine: settings.command_engine.clone(),
        s1_fallback: settings.s1_fallback,
        claude_model: settings.claude_model.clone(),
        claude_bin: find_claude(|path| path.is_file(), |name| std::env::var_os(name))
            .map(|p| p.display().to_string())
            .unwrap_or(base.claude_bin.clone()),
        openai_base_url: settings.openai_base_url.trim().into(),
        openai_model: settings.openai_model.trim().into(),
        openai_api_key: api_key.unwrap_or(base.openai_api_key.clone()),
        log_text: settings.log_text,
        ..base
    }
}

/// `claude.exe` (docs/plans/M8-windows.md, setup step 3): the native installer's `%USERPROFILE%\.local\bin` (not on
/// `PATH`), WinGet's links folder, npm's `claude.cmd`, then `PATH`. `VTT_CLAUDE_BIN` wins, as in dictate.sh.
pub fn find_claude(
    exists: impl Fn(&Path) -> bool,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(path) = var("VTT_CLAUDE_BIN").map(PathBuf::from).filter(|p| exists(p)) {
        return Some(path);
    }
    let under = |name: &str, rest: &[&str]| {
        var(name).map(|base| rest.iter().fold(PathBuf::from(base), |path, part| path.join(part)))
    };
    let candidates = [
        under("USERPROFILE", &[".local", "bin", "claude.exe"]),
        under("LOCALAPPDATA", &["Microsoft", "WinGet", "Links", "claude.exe"]),
        under("APPDATA", &["npm", "claude.cmd"]),
    ];
    let on_path = var("PATH").into_iter().flat_map(|path| {
        std::env::split_paths(&path)
            .flat_map(|dir| [dir.join("claude.exe"), dir.join("claude.cmd")])
            .collect::<Vec<_>>()
    });
    candidates.into_iter().flatten().chain(on_path).find(|path| exists(path))
}

/// Where the cleanup engine's API key is in Credential Manager: a generic credential named `<account>.<service>`, the
/// `keyring` crate's naming, with the same service and account as the Mac's Keychain (`APIKeychain.swift`).
pub fn api_key_target(base_url: &str) -> String {
    format!("{}.{API_KEY_SERVICE}", ovt_core::settings::api_key_account(base_url))
}

pub const API_KEY_SERVICE: &str = "OpenVoiceType cleanup API key";

/// The API key for `base_url` from Credential Manager, if one is stored. (Settings → Cleanup will store it: W5.)
#[cfg(windows)]
pub fn stored_api_key(base_url: &str) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::Security::Credentials::{CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC};
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: CredReadW allocates the credential, which we read and then free.
    unsafe {
        CredReadW(&HSTRING::from(api_key_target(base_url)), CRED_TYPE_GENERIC, None, &mut credential).ok()?;
        let stored = &*credential;
        let blob = std::slice::from_raw_parts(stored.CredentialBlob, stored.CredentialBlobSize as usize);
        let key = decode_secret(blob);
        CredFree(credential as *const _);
        key
    }
}

/// A stored secret: `keyring` writes UTF-16LE (an ASCII key then has a zero in every other byte); a UTF-8 blob from
/// another tool has no zero bytes.
pub fn decode_secret(blob: &[u8]) -> Option<String> {
    if !blob.contains(&0) {
        return String::from_utf8(blob.to_vec()).ok();
    }
    if !blob.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = blob.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    String::from_utf16(&units).ok().map(|text| text.trim_end_matches('\0').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::OsString;

    #[test]
    fn maps_settings_to_the_pipeline() {
        let settings = Settings {
            refine: false,
            cleanup_engine: "openai".into(),
            claude_model: "sonnet".into(),
            openai_base_url: " https://api.groq.com/openai/v1 ".into(),
            log_text: true,
            ..Settings::default()
        };
        let base = Config { claude_timeout: 20, ..Config::default() };
        let config = config(base, &settings, DictationMode::Chat, "Slack", Some("sk-1".into()));
        assert_eq!((config.mode.as_str(), config.app_name.as_str()), ("chat", "Slack"));
        assert_eq!((config.refine, config.cleanup.as_str(), config.claude_model.as_str()), (false, "openai", "sonnet"));
        assert_eq!(config.openai_base_url, "https://api.groq.com/openai/v1");
        assert_eq!((config.openai_api_key.as_str(), config.log_text, config.claude_timeout), ("sk-1", true, 20));
    }

    #[test]
    fn finds_claude() {
        let env: HashMap<&str, OsString> = HashMap::from([
            ("USERPROFILE", OsString::from("/home/u")),
            ("APPDATA", OsString::from("/home/u/roaming")),
            ("PATH", OsString::from("/bin")),
        ]);
        let var = |name: &str| env.get(name).cloned();
        let native = PathBuf::from("/home/u/.local/bin/claude.exe");
        assert_eq!(find_claude(|p| p == native, var), Some(native.clone()));
        let npm = PathBuf::from("/home/u/roaming/npm/claude.cmd");
        assert_eq!(find_claude(|p| p == npm, var), Some(npm));
        let path = PathBuf::from("/bin/claude.exe");
        assert_eq!(find_claude(|p| p == path, var), Some(path));
        assert_eq!(find_claude(|_| false, var), None);
    }

    #[test]
    fn api_keys() {
        assert_eq!(api_key_target("https://api.openai.com/v1"), "api.openai.com.OpenVoiceType cleanup API key");
        let utf16: Vec<u8> = "sk-é".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode_secret(&utf16).as_deref(), Some("sk-é"));
        assert_eq!(decode_secret(b"sk-abc").as_deref(), Some("sk-abc"));
    }

    #[test]
    fn helper_paths() {
        let spec = whisper_spec(&Settings::default(), Path::new("helpers"));
        assert_eq!(spec.binary, Path::new("helpers").join("whisper-server.exe"));
        assert_eq!(spec.port, WHISPER_PORT);
        assert!(spec.model.starts_with(ovt_core::paths::whisper_dir()));
    }
}
