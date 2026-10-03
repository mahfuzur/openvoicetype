//! The pipeline's settings: the variables at the top of `dictate.sh`, from the same `VTT_*` environment the apps set
//! (`Config::from_env`), or built directly by the Windows app. `config.sh` (a bash file) isn't read.

use std::path::PathBuf;

/// What a run does: a dictation's cleanup (`refine`) or Command Mode (`command`). `dictate.sh`'s `JOB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    Cleanup,
    Command,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub job: Job,
    /// `MODE`: default, chat, email, code, notes or raw.
    pub mode: String,
    /// `APP_NAME` (`VTT_APP`): the target app, sent as context.
    pub app_name: String,

    /// `REFINE`, `REFINE_MIN_WORDS`.
    pub refine: bool,
    pub refine_min_words: usize,
    /// `CLEANUP`: claude, s1 or openai.
    pub cleanup: String,
    /// `COMMAND_ENGINE`: claude or openai.
    pub command_engine: String,
    pub s1_fallback: bool,
    pub meaning_guard: bool,

    pub claude_bin: String,
    pub claude_model: String,
    /// Seconds. Command Mode uses `command_timeout` instead.
    pub claude_timeout: u64,
    pub claude_thinking_tokens: u32,
    pub claude_prestart: bool,
    pub claude_use_api_key: bool,

    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_api_key: String,
    pub openai_timeout: u64,
    pub command_timeout: u64,

    pub s1_port: u16,
    pub s1_model: PathBuf,
    pub s1_timeout: u64,

    pub online_check: bool,
    pub online_check_host: String,
    /// `VTT_OFFLINE=on`: treat the network as down.
    pub force_offline: bool,

    /// `LANGUAGE`: Whisper's language ("en").
    pub language: String,
    /// `VOCAB` + `VTT_VOCAB`, comma-separated.
    pub vocab: String,
    pub whisper_prompt: bool,
    pub whisper_style: String,

    /// `prompts/` (system.md, command.md, modes/).
    pub prompts_dir: PathBuf,
    /// `PROMPT_FILE`: the user's own system prompt, used instead of system.md when it exists.
    pub prompt_file: PathBuf,
    pub dictionary_file: PathBuf,
    /// `VTT_COMMAND_FILE`: Command Mode's JSON (target, original, current, turns).
    pub command_file: Option<PathBuf>,

    pub state_dir: PathBuf,
    pub log_file: PathBuf,
    pub log_text: bool,
}

/// `WHISPER_STYLE`: a short fake transcript in the target style; Whisper copies its punctuation, digits and spelling.
pub const WHISPER_STYLE: &str = "Okay, here is the update. The meeting is on Thursday, March 3, 2026, at 2:30 PM, and the budget is $15,400, about 15% over. We deploy to AWS with GitHub Actions, Docker and the API. Please email dev.team@example.com.";

impl Default for Config {
    /// `dictate.sh`'s defaults, with this platform's folders (`ovt_core::paths`) and `prompts/` next to the program.
    fn default() -> Self {
        let config_dir = ovt_core::paths::config_dir();
        Config {
            job: Job::Cleanup,
            mode: "default".into(),
            app_name: String::new(),
            refine: true,
            refine_min_words: 4,
            cleanup: "claude".into(),
            command_engine: "claude".into(),
            s1_fallback: true,
            meaning_guard: true,
            claude_bin: "claude".into(),
            claude_model: "haiku".into(),
            claude_timeout: 15,
            claude_thinking_tokens: 0,
            claude_prestart: true,
            claude_use_api_key: false,
            openai_base_url: String::new(),
            openai_model: String::new(),
            openai_api_key: String::new(),
            openai_timeout: 15,
            command_timeout: 30,
            s1_port: 8178,
            s1_model: ovt_core::paths::s1_dir().join("s1-mini-q4_k_m.gguf"),
            s1_timeout: 10,
            online_check: true,
            online_check_host: "api.anthropic.com".into(),
            force_offline: false,
            language: "en".into(),
            vocab: String::new(),
            whisper_prompt: true,
            whisper_style: WHISPER_STYLE.into(),
            prompts_dir: default_prompts_dir(),
            prompt_file: config_dir.join("prompt.txt"),
            dictionary_file: config_dir.join("dictionary.txt"),
            command_file: None,
            state_dir: ovt_core::paths::state_dir(),
            log_file: ovt_core::paths::log_file(),
            log_text: false,
        }
    }
}

/// `prompts/` next to the program (the installed app), else one or two levels up (a build in the repo: target/<profile>).
fn default_prompts_dir() -> PathBuf {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
    exe.ancestors()
        .take(4)
        .map(|dir| dir.join("prompts"))
        .find(|dir| dir.join("system.md").is_file())
        .unwrap_or_else(|| exe.join("prompts"))
}

impl Config {
    /// The defaults, overridden by the environment the way `dictate.sh` reads it: `VTT_<NAME>` first (the apps), then
    /// the plain `<NAME>` (a user's environment or the eval), for the variables the script reads that way.
    pub fn from_env() -> Self {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// `from_env` with any variable source (for tests). An empty variable counts as unset, like the script's
    /// `${X:-default}`.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        let var = |name: &str| var(name).filter(|value| !value.is_empty());
        let mut c = Config::default();
        // VTT_X, then X (only where dictate.sh reads both).
        let both = |name: &str| var(&format!("VTT_{name}")).or_else(|| var(name));
        let on = |value: String| value == "on";
        let number = |value: Option<String>, default: u64| value.and_then(|v| v.trim().parse().ok()).unwrap_or(default);
        if let Some(v) = both("MODE") {
            c.mode = v;
        }
        if let Some(v) = var("VTT_APP") {
            c.app_name = v;
        }
        if let Some(v) = both("REFINE") {
            c.refine = on(v);
        }
        c.refine_min_words = number(var("REFINE_MIN_WORDS"), 4) as usize;
        if let Some(v) = both("CLEANUP") {
            c.cleanup = v;
        }
        if let Some(v) = both("COMMAND_ENGINE") {
            c.command_engine = v;
        }
        if let Some(v) = both("S1_FALLBACK") {
            c.s1_fallback = on(v);
        }
        if let Some(v) = var("MEANING_GUARD") {
            c.meaning_guard = on(v);
        }
        if let Some(v) = both("CLAUDE_BIN") {
            c.claude_bin = v;
        }
        if let Some(v) = both("CLAUDE_MODEL") {
            c.claude_model = v;
        }
        c.claude_timeout = number(var("CLAUDE_TIMEOUT"), 15);
        c.claude_thinking_tokens = number(var("CLAUDE_THINKING_TOKENS"), 0) as u32;
        if let Some(v) = var("CLAUDE_PRESTART") {
            c.claude_prestart = on(v);
        }
        if let Some(v) = var("CLAUDE_USE_API_KEY") {
            c.claude_use_api_key = on(v);
        }
        if let Some(v) = both("OPENAI_BASE_URL") {
            c.openai_base_url = v;
        }
        if let Some(v) = both("OPENAI_MODEL") {
            c.openai_model = v;
        }
        if let Some(v) = var("OPENAI_API_KEY") {
            c.openai_api_key = v;
        }
        // The app's key file: read and deleted at once, like the script does.
        if let Some(path) = var("VTT_OPENAI_KEY_FILE").filter(|p| !p.is_empty()) {
            if let Ok(key) = std::fs::read_to_string(&path) {
                c.openai_api_key = key.replace(['\r', '\n'], "");
            }
            let _ = std::fs::remove_file(path);
        }
        c.openai_timeout = number(var("OPENAI_TIMEOUT"), 15);
        c.command_timeout = number(var("COMMAND_TIMEOUT"), 30);
        c.s1_port = number(var("S1_PORT"), 8178) as u16;
        if let Some(v) = var("S1_MODEL") {
            c.s1_model = v.into();
        }
        c.s1_timeout = number(var("S1_TIMEOUT"), 10);
        if let Some(v) = var("ONLINE_CHECK") {
            c.online_check = on(v);
        }
        if let Some(v) = var("ONLINE_CHECK_HOST") {
            c.online_check_host = v;
        }
        c.force_offline = var("VTT_OFFLINE").is_some_and(on);
        if let Some(v) = var("LANGUAGE") {
            c.language = v;
        }
        c.vocab = [var("VOCAB"), var("VTT_VOCAB")].into_iter().flatten().collect::<Vec<_>>().join(",");
        if let Some(v) = var("WHISPER_PROMPT") {
            c.whisper_prompt = on(v);
        }
        if let Some(v) = var("WHISPER_STYLE") {
            c.whisper_style = v;
        }
        if let Some(v) = var("VTT_PROMPTS_DIR") {
            c.prompts_dir = v.into();
        }
        if let Some(v) = var("PROMPT_FILE") {
            c.prompt_file = v.into();
        }
        if let Some(v) = var("DICTIONARY_FILE") {
            c.dictionary_file = v.into();
        }
        c.command_file = var("VTT_COMMAND_FILE").filter(|v| !v.is_empty()).map(PathBuf::from);
        if let Some(v) = var("VTT_LOG_FILE") {
            c.log_file = v.into();
        }
        if let Some(v) = both("LOG_TEXT") {
            c.log_text = on(v);
        }
        c
    }

    /// The online engine for this job: `CLEANUP` for a dictation (claude or openai; s1 has none), `COMMAND_ENGINE`
    /// (anything but openai is claude) for Command Mode. `dictate.sh`'s `ONLINE_ENGINE`.
    pub fn online_engine(&self) -> &str {
        match self.job {
            Job::Command if self.command_engine == "openai" => "openai",
            Job::Command => "claude",
            Job::Cleanup => &self.cleanup,
        }
    }

    /// Command Mode's timeouts replace Claude's and the endpoint's (`cmd_command`).
    pub fn engine_timeout(&self) -> u64 {
        match (self.job, self.online_engine()) {
            (Job::Command, _) => self.command_timeout,
            (_, "openai") => self.openai_timeout,
            _ => self.claude_timeout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn reads_the_environment_like_the_script() {
        let env: HashMap<&str, &str> = HashMap::from([
            ("VTT_MODE", "chat"),
            ("MODE", "email"),
            ("VTT_REFINE", "off"),
            ("VTT_CLEANUP", "openai"),
            ("CLAUDE_TIMEOUT", "20"),
            ("VTT_OFFLINE", "on"),
            ("VOCAB", "Kubernetes"),
            ("VTT_VOCAB", "OpenVoiceType"),
            ("VTT_LOG_TEXT", "on"),
        ]);
        let c = Config::from_vars(|name| env.get(name).map(|v| v.to_string()));
        assert_eq!((c.mode.as_str(), c.refine, c.cleanup.as_str()), ("chat", false, "openai"));
        assert_eq!((c.claude_timeout, c.force_offline, c.log_text), (20, true, true));
        assert_eq!(c.vocab, "Kubernetes,OpenVoiceType");
        assert_eq!(c.online_engine(), "openai");
        let d = Config::from_vars(|name| (name == "VTT_MODE" || name == "WHISPER_STYLE").then(String::new));
        assert_eq!((d.mode.as_str(), d.claude_model.as_str(), d.s1_port), ("default", "haiku", 8178));
        assert_eq!(d.whisper_style, WHISPER_STYLE, "an empty variable is unset");
        assert_eq!(Config { job: Job::Command, ..d }.online_engine(), "claude");
    }
}
