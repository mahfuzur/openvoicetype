//! `refine_text`, `try_online`, `try_s1`, `cmd_refine`, `cmd_command`, `write_result`: the cleanup dispatcher and its
//! report, the same statuses, exit codes and result-file JSON as the script.

use crate::claude::{self, blank, chomp, Prestart, Prestarted};
use crate::config::{Config, Job};
use crate::text::{self, Dictionary};
use crate::{online, openai, s1};
use std::collections::BTreeMap;
use std::time::Instant;

/// `REFINE_STATUS` and the details the result file carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// ok | s1 | s1-fallback | skipped | failed-fallback-raw | guard-raw (Command Mode: ok | failed)
    pub status: String,
    /// claude | openai | s1 | none
    pub engine: String,
    pub error: String,
    pub resets: String,
    pub guard: String,
    pub rejected: String,
    /// Whisper's text as it would be pasted, for the swap.
    pub raw: String,
    /// The text to paste.
    pub result: String,
    pub claude_ms: u64,
    pub openai_ms: u64,
    pub s1_ms: u64,
}

impl Report {
    /// `cmd_refine`'s exit code: 3 failed-fallback-raw, 4 s1-fallback, 5 guard-raw, else 0. Command Mode: 3 if failed.
    pub fn exit_code(&self) -> i32 {
        match self.status.as_str() {
            "failed-fallback-raw" | "failed" => 3,
            "s1-fallback" => 4,
            "guard-raw" => 5,
            _ => 0,
        }
    }

    /// `write_result`'s JSON (canonical key order).
    pub fn result_json(&self) -> String {
        let fields = BTreeMap::from([
            ("engine", &self.engine),
            ("error", &self.error),
            ("guard", &self.guard),
            ("raw", &self.raw),
            ("rejected", &self.rejected),
            ("resets", &self.resets),
            ("status", &self.status),
        ]);
        serde_json::to_string(&fields).unwrap_or_default()
    }

    fn new(result: &str, status: &str) -> Report {
        Report { result: result.into(), status: status.into(), engine: "none".into(), ..Report::default() }
    }
}

/// `log`: a line in dictate.log ("%Y-%m-%d %H:%M:%S <line>").
pub fn log(config: &Config, line: &str) {
    claude::append_log(&config.log_file, line);
}

fn ms(since: Instant) -> u64 {
    since.elapsed().as_millis() as u64
}

/// One cleanup, started when recording starts (`cmd_refine` / `cmd_command` before the transcript arrives): it
/// pre-starts Claude (or notes that it's offline) so it's ready when the transcript comes.
pub struct Session {
    config: Config,
    dictionary: Dictionary,
    prestarted: Option<Prestarted>,
    /// `PRESTART_OFFLINE`.
    offline_at_start: bool,
    s1_ready: Option<Box<dyn FnMut() -> bool + Send>>,
}

impl Session {
    pub fn start(config: Config) -> Session {
        let dictionary = Dictionary::load(&config.dictionary_file);
        let (prestarted, offline_at_start) = match Prestarted::start(&config, || text::system_prompt(&config)) {
            Prestart::Started(prestarted) => (Some(prestarted), false),
            Prestart::Offline => (None, true),
            Prestart::Off => (None, false),
        };
        Session { config, dictionary, prestarted, offline_at_start, s1_ready: None }
    }

    /// Offline when recording started, so a dictation's cleanup will fall back to S1-mini: the app should start loading
    /// it now (`claude_prestart`'s background `srv_start s1-server`).
    pub fn wants_s1(&self) -> bool {
        let c = &self.config;
        self.offline_at_start && c.job == Job::Cleanup && c.s1_fallback && c.mode != "code" && c.s1_model.is_file()
    }

    /// How S1-mini gets ready before it's used (`refine_s1`'s `srv_start s1-server`): the app starts its server and
    /// says whether it answers. Without one, S1-mini is used only if its server already answers.
    pub fn set_s1_starter(&mut self, start: impl FnMut() -> bool + Send + 'static) {
        self.s1_ready = Some(Box::new(start));
    }

    /// The cleanup of `raw` (`refine_text`; Command Mode: the instruction), logged like the script logs it. Empty
    /// `raw` ends the session without work (status "skipped", nothing to paste).
    pub fn finish(mut self, raw: &str) -> Report {
        let raw = chomp(raw); // RAW_TEXT="$(cat)"
        match self.config.job {
            Job::Cleanup => self.cleanup(&raw),
            Job::Command => self.command(&raw),
        }
    }

    /// `cmd_refine` + `refine_text`.
    fn cleanup(&mut self, raw: &str) -> Report {
        if raw.is_empty() {
            return Report::new("", "skipped");
        }
        let mut r = Report::new(raw, "skipped");
        let c = self.config.clone();
        if c.refine && c.mode != "raw" && text::word_count(raw) >= c.refine_min_words {
            if c.cleanup == "s1" {
                // S1-mini has no code style: code mode keeps the raw text (post-processing still runs).
                if c.mode != "code" && !self.try_s1(&mut r, raw, "s1") {
                    r.status = "failed-fallback-raw".into();
                }
            } else {
                self.try_online(&mut r, raw);
                if r.status == "failed-fallback-raw" && c.s1_fallback && c.mode != "code" && c.s1_model.is_file() {
                    self.try_s1(&mut r, raw, "s1-fallback");
                }
            }
        }
        if c.meaning_guard && matches!(r.status.as_str(), "ok" | "s1" | "s1-fallback") {
            if let Some(reason) = text::meaning_guard(raw, &r.result) {
                log(
                    &c,
                    &format!("GUARD the cleanup dropped {}, using Whisper's text", text::log_safe(&reason, c.log_text)),
                );
                r.rejected = text::post_process(&c, &self.dictionary, &r.result);
                r.result = raw.into();
                r.status = "guard-raw".into();
                r.guard = reason;
            }
        }
        r.result = text::post_process(&c, &self.dictionary, &r.result);
        log(&c, &format!("REFINE {} mode={}{}", self.timings(&r), c.mode, self.app()));
        if c.log_text && r.status != "skipped" {
            log(&c, &format!("  cleaned: {}", r.result.replace('\n', " ⏎ ")));
        }
        self.with_raw(r, raw)
    }

    /// `cmd_command`: no S1-mini fallback and no meaning guard (an edit is meant to change the text).
    fn command(&mut self, instruction: &str) -> Report {
        if blank(instruction) {
            return Report::new("", "skipped");
        }
        let c = self.config.clone();
        let mut r = Report::new("", "failed");
        self.try_online(&mut r, instruction);
        if r.status == "ok" {
            r.result = text::post_process(&c, &self.dictionary, &r.result);
            if blank(&r.result) {
                r.status = "failed".into();
                r.error = "error".into();
            }
        } else {
            r.result = String::new();
            r.status = "failed".into();
        }
        log(&c, &format!("COMMAND target={} {} mode={}{}", command_target(&c), self.timings(&r), c.mode, self.app()));
        if c.log_text {
            log(&c, &format!("  instruction: {instruction}"));
        }
        self.with_raw(r, instruction)
    }

    /// `write_result`'s `raw`: Whisper's text as it would be pasted (dictionary, output filter), for the app's swap.
    fn with_raw(&self, mut r: Report, raw: &str) -> Report {
        r.raw = if r.status == "guard-raw" || r.engine == "none" {
            r.result.clone()
        } else {
            text::post_process(&self.config, &self.dictionary, raw)
        };
        r
    }

    /// `try_online`: the selected online engine (Claude, or the OpenAI-compatible endpoint). On failure the status is
    /// failed-fallback-raw, and the error and reset time say why.
    fn try_online(&mut self, r: &mut Report, raw: &str) {
        let c = self.config.clone();
        let engine = c.online_engine().to_string();
        r.status = "failed-fallback-raw".into();
        let offline = if engine == "openai" {
            let (host, port) = openai::endpoint(&c.openai_base_url);
            !openai::is_local(&c.openai_base_url) && online::is_offline(&c, Some((&host, port)))
        } else {
            // A pre-started process means the online check already passed when recording started.
            self.offline_at_start || (self.prestarted.is_none() && online::is_offline(&c, None))
        };
        if offline {
            log(&c, &format!("OFFLINE skipping {engine}"));
            r.error = "offline".into();
            return;
        }
        let system_prompt = text::system_prompt(&c);
        let message = match c.job {
            Job::Command => text::command_message(&c, &self.dictionary, raw),
            Job::Cleanup => text::user_message(&c, &self.dictionary, raw),
        };
        let t0 = Instant::now();
        let outcome = if engine == "openai" {
            let outcome = openai::refine(&c, &system_prompt, &message);
            r.openai_ms = ms(t0);
            outcome
        } else {
            let outcome = claude::refine(&c, self.prestarted.take(), &system_prompt, &message);
            r.claude_ms = ms(t0);
            outcome
        };
        let error = match outcome {
            Ok(out) => {
                r.result = out;
                r.status = "ok".into();
                r.engine = (if engine == "openai" { "openai" } else { "claude" }).into();
                return;
            }
            Err(error) => error,
        };
        r.error = if error.kind.is_empty() { "error".into() } else { error.kind };
        if engine == "claude" && r.error == "auth" && claude::signed_in(&c) {
            // The --bare canary: Anthropic plans to make bare mode (no subscription login) the default for -p.
            r.error = "auth-mismatch".into();
            log(
                &c,
                "WARN claude -p says it isn't signed in, but 'claude auth status' says it is: Claude Code may have changed \
                 how print mode signs in (see https://code.claude.com/docs/en/headless)",
            );
        }
        r.resets = if error.resets.is_empty() { String::new() } else { text::format_resets(&error.resets) };
        let resets = if r.resets.is_empty() { String::new() } else { format!(" (resets {})", r.resets) };
        let detail = if error.detail.is_empty() { String::new() } else { format!(": {}", error.detail) };
        log(&c, &format!("WARN {engine} failed: {}{resets}{detail}", r.error));
    }

    /// `try_s1`: S1-mini; on success the result, engine s1 and `status`.
    fn try_s1(&mut self, r: &mut Report, raw: &str, status: &str) -> bool {
        let t0 = Instant::now();
        let ready = match self.s1_ready.as_mut() {
            Some(start) => start(),
            None => s1::healthy(&self.config),
        };
        let out = if ready { s1::refine(&self.config, raw) } else { None };
        r.s1_ms = ms(t0);
        match out {
            Some(out) => {
                r.result = out;
                r.status = status.into();
                r.engine = "s1".into();
                true
            }
            None => false,
        }
    }

    /// `cleanup_timings`: stage times, the outcome, and why the online engine failed.
    fn timings(&self, r: &Report) -> String {
        let mut line = format!("claude={}ms s1={}ms", r.claude_ms, r.s1_ms);
        if self.config.online_engine() == "openai" {
            line += &format!(" openai={}ms", r.openai_ms);
        }
        line += &format!(" refine={}", r.status);
        if !r.error.is_empty() {
            line += &format!(" error={}", r.error);
        }
        line
    }

    fn app(&self) -> String {
        if self.config.app_name.is_empty() {
            String::new()
        } else {
            format!(" app=\"{}\"", self.config.app_name)
        }
    }
}

/// Command Mode's target from `VTT_COMMAND_FILE` (selection, last_dictation, write or copy), "write" if unknown.
fn command_target(config: &Config) -> String {
    let job = config
        .command_file
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_default();
    let target = match &job["target"] {
        serde_json::Value::String(target) => target.clone(),
        other => claude::perl_string(other).unwrap_or_default(),
    };
    if target.is_empty() {
        "write".into()
    } else {
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes() {
        let code = |status: &str| Report { status: status.into(), ..Report::default() }.exit_code();
        assert_eq!(
            ["ok", "s1", "skipped", "failed-fallback-raw", "s1-fallback", "guard-raw", "failed"].map(code),
            [0, 0, 0, 3, 4, 5, 3]
        );
    }

    #[test]
    fn result_json_is_canonical() {
        let report = Report {
            status: "guard-raw".into(),
            engine: "claude".into(),
            guard: "a number (12)".into(),
            rejected: "Line \"one\"\nCafé\u{1f}".into(),
            raw: "We have 12.".into(),
            result: "not in the file".into(),
            claude_ms: 5,
            ..Report::default()
        };
        assert_eq!(
            report.result_json(),
            r#"{"engine":"claude","error":"","guard":"a number (12)","raw":"We have 12.","rejected":"Line \"one\"\nCafé\u001f","resets":"","status":"guard-raw"}"#
        );
    }

    #[test]
    fn reads_the_command_target() {
        let dir = crate::claude::tests::test_dir("target");
        let file = dir.join("command.json");
        let config = Config { command_file: Some(file.clone()), ..Config::default() };
        assert_eq!(command_target(&config), "write");
        std::fs::write(&file, r#"{"target":"selection","original":"x"}"#).unwrap();
        assert_eq!(command_target(&config), "selection");
        std::fs::write(&file, r#"{"target":""}"#).unwrap();
        assert_eq!(command_target(&config), "write");
        assert_eq!(command_target(&Config::default()), "write");
    }

    #[test]
    fn empty_input_is_skipped_without_work() {
        let config = Config {
            dictionary_file: "/nonexistent/dictionary.txt".into(),
            ..crate::openai::tests::test_config("empty")
        };
        let report = Session::start(Config { claude_prestart: false, ..config.clone() }).finish("\n");
        assert_eq!((report.status.as_str(), report.exit_code(), report.result.as_str()), ("skipped", 0, ""));
        let command = Config { job: Job::Command, claude_prestart: false, ..config.clone() };
        assert_eq!(Session::start(command).finish("  ").status, "skipped");
        assert!(!config.log_file.exists(), "nothing logged");
    }

    /// The whole dictation with tests/fake-claude (needs text.rs: the prompts, post_process, the guard).
    #[cfg(unix)]
    mod with_text {
        use super::*;
        use std::path::Path;

        fn config(name: &str) -> Config {
            let dir = crate::claude::tests::test_dir(name);
            Config {
                claude_bin: Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fake-claude")
                    .to_string_lossy()
                    .into_owned(),
                prompts_dir: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../prompts"),
                prompt_file: dir.join("prompt.txt"),
                dictionary_file: dir.join("dictionary.txt"),
                state_dir: dir.join("state"),
                log_file: dir.join("dictate.log"),
                s1_model: dir.join("no-s1.gguf"),
                online_check: false,
                ..Config::default()
            }
        }

        #[test]
        #[ignore = "needs text.rs"]
        fn cleans_up_with_the_prestarted_claude() {
            let config = config("session-ok");
            let report = Session::start(config.clone()).finish("we need to deploy it to the cluster today\n");
            assert_eq!((report.status.as_str(), report.engine.as_str()), ("ok", "claude"));
            assert_eq!(report.result, "WE NEED TO DEPLOY IT TO THE CLUSTER TODAY");
            assert_eq!(report.raw, "we need to deploy it to the cluster today");
            let log = std::fs::read_to_string(&config.log_file).unwrap();
            assert!(log.contains(" REFINE claude="), "{log}");
            assert!(log.contains(" refine=ok mode=default\n"), "{log}");
        }

        #[test]
        #[ignore = "needs text.rs"]
        fn offline_without_s1_pastes_whisper_text() {
            let config = Config { force_offline: true, app_name: "Notes".into(), ..config("session-offline") };
            let report = Session::start(config.clone()).finish("we need to deploy it to the cluster today");
            assert_eq!(
                (report.status.as_str(), report.error.as_str(), report.exit_code()),
                ("failed-fallback-raw", "offline", 3)
            );
            assert_eq!(report.result, "we need to deploy it to the cluster today");
            let log = std::fs::read_to_string(&config.log_file).unwrap();
            assert!(log.contains(" OFFLINE skipping claude\n"), "{log}");
            assert!(log.contains(" refine=failed-fallback-raw error=offline mode=default app=\"Notes\"\n"), "{log}");
        }

        #[test]
        #[ignore = "needs text.rs"]
        fn too_short_is_skipped() {
            let report = Session::start(config("session-short")).finish("hello there");
            assert_eq!((report.status.as_str(), report.engine.as_str(), report.exit_code()), ("skipped", "none", 0));
        }
    }
}
