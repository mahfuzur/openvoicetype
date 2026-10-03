//! The contract with `scripts/dictate.sh`, the same as the macOS app's (follows `Dictation.swift` and its `ScriptRun`).
//!
//! - `transcribe <wav>` prints Whisper's text (empty: no speech). Watchdog 120 s.
//! - `refine` is launched when recording starts (it starts Claude, then waits); the transcript arrives on stdin when
//!   recording stops, and empty stdin ends it without work. Exit 0 = cleaned (or cleanup skipped), 3 = Whisper's text
//!   (cleanup failed), 4 = S1-mini replaced the online engine, 5 = the meaning guard used Whisper's text. Details in the
//!   JSON `VTT_RESULT_FILE`. Watchdog 45 s.
//! - `command` is the same for Command Mode, with its plan in `VTT_COMMAND_FILE`. Exit 0 = new text, 3 = nothing to
//!   paste. Watchdog 75 s.
//! - `whisper-server` / `s1-server` `start [--keep] | release | stop | status`.
//!
//! A watchdog stops a stuck run with SIGTERM, never SIGKILL: the script's exit trap then ends its pre-started Claude.

use crate::mode::DictationMode;
use crate::settings::Settings;
use serde::Deserialize;
#[cfg(unix)]
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Child, Command, Stdio};
#[cfg(unix)]
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(120);
pub const REFINE_TIMEOUT: Duration = Duration::from_secs(45);
/// Claude has 30 s for a command, and a stream the script can't read gets one more try as a one-shot call.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(75);
pub const TEST_CLEANUP_TIMEOUT: Duration = Duration::from_secs(60);

/// What `refine`/`command` reported in `VTT_RESULT_FILE`. Missing fields are empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct CleanupDetails {
    /// ok, s1, s1-fallback, skipped, failed-fallback-raw, guard-raw (failed for a command).
    pub status: String,
    /// claude, openai, s1 or none.
    pub engine: String,
    /// limit, auth, auth-mismatch, offline, timeout, config or error; empty when nothing failed.
    pub error: String,
    pub resets: String,
    #[serde(rename = "guard")]
    pub guard_reason: String,
    pub rejected: String,
    /// Whisper's text after the dictionary and output filter: what "swap to Whisper's text" pastes.
    pub raw: String,
}

impl CleanupDetails {
    pub fn decode(json: &[u8]) -> Option<Self> {
        serde_json::from_slice(json).ok()
    }
}

/// Whether a dictation's `refine` is expected to clean up (it always runs: with cleanup off or in Raw mode it still
/// applies the dictionary and the output filter). S1-mini has no code style, so it skips code mode.
pub fn uses_cleanup(settings: &Settings, mode: DictationMode) -> bool {
    settings.refine && mode != DictationMode::Raw && !(settings.cleanup_engine == "s1" && mode == DictationMode::Code)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefineOutcome {
    /// What to paste: the cleanup, or Whisper's text when there's none.
    pub text: String,
    /// Cleanup was expected but didn't happen: the text is Whisper's.
    pub cleanup_failed: bool,
    /// The online engine was unavailable and S1-mini cleaned the text up instead.
    pub offline_fallback: bool,
}

/// Reads a finished `refine` (follows `Dictation.stop`). `status` is None when the process couldn't run or was stopped
/// by a signal.
pub fn interpret_refine(status: Option<i32>, output: &str, raw: &str, uses_cleanup: bool) -> RefineOutcome {
    let cleaned = output.trim();
    let offline_fallback = uses_cleanup && status == Some(4) && !cleaned.is_empty();
    let cleanup_failed =
        uses_cleanup && !offline_fallback && status != Some(5) && (status != Some(0) || cleaned.is_empty());
    RefineOutcome {
        text: if cleaned.is_empty() { raw.to_string() } else { cleaned.to_string() },
        cleanup_failed,
        offline_fallback,
    }
}

/// A short name for the engine `refine` reported.
pub fn engine_name(engine: &str, settings: &Settings) -> String {
    match engine {
        "claude" => {
            let mut model = settings.claude_model.clone();
            if let Some(first) = model.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            format!("Claude {model}")
        }
        "openai" if settings.openai_model.is_empty() => "the API".into(),
        "openai" => settings.openai_model.clone(),
        "s1" => "S1-mini".into(),
        _ => "no cleanup".into(),
    }
}

/// Why the online engine failed, for the overlay and the test result; None when nothing failed or it was offline.
/// `engine` is the online engine that ran: the cleanup engine, or Command Mode's.
pub fn problem_description(details: &CleanupDetails, engine: &str) -> Option<String> {
    let claude = engine != "openai";
    Some(match details.error.as_str() {
        "limit" => {
            let resets = if details.resets.is_empty() { String::new() } else { format!(", resets {}", details.resets) };
            format!("{}{resets}", if claude { "Claude limit reached" } else { "API rate limit" })
        }
        "auth" => (if claude { "Claude isn't signed in" } else { "API key rejected" }).into(),
        "auth-mismatch" => "Claude Code changed how scripts sign in; see the log".into(),
        "config" => "set up the API in Settings".into(),
        "timeout" => (if claude { "Claude timed out" } else { "the API timed out" }).into(),
        _ => return None,
    })
}

/// Where the app's helpers and models are, for `environment`.
#[derive(Clone, Debug, Default)]
pub struct Locations {
    /// The packaged whisper-server, whisper-cli and llama-server (`/usr/lib/openvoicetype/bin`).
    pub bin_dir: Option<PathBuf>,
    /// The chosen Whisper model, if installed.
    pub whisper_model: Option<PathBuf>,
    /// The `claude` CLI found through the login shell (npm and nvm installs aren't on a desktop app's PATH).
    pub claude_bin: Option<PathBuf>,
}

/// The variables every run gets on top of the inherited environment (follows `Dictation.environment`).
pub fn environment(settings: &Settings, locations: &Locations, inherited_path: &str) -> Vec<(String, String)> {
    let on = |value: bool| if value { "on" } else { "off" }.to_string();
    let mut env = vec![
        ("VTT_QUIET".into(), "on".into()),
        ("VTT_REFINE".into(), on(settings.refine)),
        ("VTT_CLAUDE_MODEL".into(), settings.claude_model.clone()),
        ("VTT_CLEANUP".into(), settings.cleanup_engine.clone()),
        ("VTT_S1_FALLBACK".into(), on(settings.s1_fallback)),
        ("VTT_LOG_TEXT".into(), on(settings.log_text)),
        ("VTT_OPENAI_BASE_URL".into(), settings.openai_base_url.trim().to_string()),
        ("VTT_OPENAI_MODEL".into(), settings.openai_model.trim().to_string()),
        ("VTT_COMMAND_ENGINE".into(), settings.command_engine.clone()),
    ];
    if let Some(dir) = &locations.bin_dir {
        env.push(("VTT_BIN_DIR".into(), dir.display().to_string()));
    }
    if let Some(model) = &locations.whisper_model {
        env.push(("VTT_WHISPER_MODEL".into(), model.display().to_string()));
    }
    if let Some(claude) = &locations.claude_bin {
        env.push(("VTT_CLAUDE_BIN".into(), claude.display().to_string()));
        // An npm install is a node script that needs its `node` from the same directory.
        if let Some(dir) = claude.parent() {
            let path = if inherited_path.is_empty() { "/usr/bin:/bin" } else { inherited_path };
            env.push(("PATH".into(), format!("{}:{path}", dir.display())));
        }
    }
    env
}

/// The `VTT_MODE` and `VTT_APP` of a dictation.
pub fn context_env(mode: DictationMode, app_name: &str) -> Vec<(String, String)> {
    vec![("VTT_MODE".into(), mode.as_str().into()), ("VTT_APP".into(), app_name.into())]
}

/// A random hex id for per-run file names: the standard library's randomly keyed hasher (OS randomness, on every
/// platform) over the time, the process and a counter.
pub fn run_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::BuildHasher;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let half = |salt: u8| RandomState::new().hash_one((salt, count, now, std::process::id()));
    format!("{:016x}{:016x}", half(0), half(1))
}

/// Writes a file only this user can read (0600), failing if it exists.
pub fn private_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    // Windows: the folder (%TEMP%) is the user's own, and files inherit its permissions.
    options.open(path)?.write_all(contents)
}

/// Key and result files a crash left behind (each normally lives for one run).
pub fn remove_leftovers(state_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(state_dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("key-") || name.starts_with("result-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(unix)]
pub struct ScriptOutput {
    /// None: stopped by a signal (the watchdog) or couldn't run.
    pub status: Option<i32>,
    pub stdout: String,
    /// `refine`/`command` only.
    pub details: Option<CleanupDetails>,
}

/// One `dictate.sh` process whose stdin is written later, so it can start work (Claude) before its input exists.
#[cfg(unix)]
pub struct ScriptRun {
    child: Child,
    /// Deleted when the process exits (the result file, a key file, the command file).
    files: Vec<PathBuf>,
    result_file: Option<PathBuf>,
    /// `command` only: where the plan goes (`VTT_COMMAND_FILE`), written once the selection is read.
    pub command_file: Option<PathBuf>,
    pub args: Vec<String>,
    /// Set by `finish` and `terminate`; otherwise `drop` closes stdin and reaps the process.
    reaped: bool,
}

#[cfg(unix)]
impl Drop for ScriptRun {
    /// A run dropped without `finish` (a replaced pre-start, an early return): closing stdin ends the script, and a
    /// thread reaps it and then removes its files, so it leaves no zombie (Rust's `Child` doesn't reap on drop).
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        drop(self.child.stdin.take());
        let pid = self.child.id() as libc::pid_t;
        let files = std::mem::take(&mut self.files);
        std::thread::spawn(move || {
            // SAFETY: waitpid(2) on our own child, which nothing else reaps.
            unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
            files.iter().for_each(|f| drop(std::fs::remove_file(f)));
        });
    }
}

#[cfg(unix)]
pub struct LaunchOptions<'a> {
    pub script: &'a Path,
    pub args: &'a [&'a str],
    /// Added to the inherited environment (`environment` + `context_env`).
    pub env: Vec<(String, String)>,
    /// The per-run files go here (`paths::state_dir`).
    pub state_dir: &'a Path,
    /// For a refine or command using the API engine: the key, handed over in a private file the script deletes at once.
    pub api_key: Option<&'a str>,
}

#[cfg(unix)]
impl ScriptRun {
    pub fn launch(options: LaunchOptions) -> std::io::Result<ScriptRun> {
        let mut env = options.env;
        let mut files = Vec::new();
        let (mut result_file, mut command_file) = (None, None);
        let job = options.args.first().copied().unwrap_or_default();
        if job == "refine" || job == "command" {
            std::fs::create_dir_all(options.state_dir)?;
            let result = options.state_dir.join(format!("result-{}.json", run_id()));
            env.push(("VTT_RESULT_FILE".into(), result.display().to_string()));
            files.push(result.clone());
            result_file = Some(result);
            if job == "command" {
                let command = options.state_dir.join(format!("result-command-{}.json", run_id()));
                env.push(("VTT_COMMAND_FILE".into(), command.display().to_string()));
                files.push(command.clone());
                command_file = Some(command);
            }
            if let Some(key) = options.api_key.filter(|k| !k.is_empty()) {
                let key_file = options.state_dir.join(format!("key-{}", run_id()));
                private_file(&key_file, key.as_bytes())?;
                env.push(("VTT_OPENAI_KEY_FILE".into(), key_file.display().to_string()));
                files.push(key_file);
            }
        }
        let spawned = Command::new("/bin/bash")
            .arg(options.script)
            .args(options.args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Ok(child) => Ok(ScriptRun {
                child,
                files,
                result_file,
                command_file,
                args: options.args.iter().map(|a| a.to_string()).collect(),
                reaped: false,
            }),
            Err(error) => {
                files.iter().for_each(|f| drop(std::fs::remove_file(f)));
                Err(error)
            }
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Writes `input` (None writes nothing), closes stdin, waits, and returns the exit status, stdout and the details
    /// `refine` wrote. Blocks: call it off the UI thread. With `timeout`, a run still going after it gets SIGTERM (its
    /// output is then usually empty, and the caller falls back).
    pub fn finish(mut self, input: Option<&str>, timeout: Option<Duration>) -> ScriptOutput {
        let pid = self.child.id() as libc::pid_t;
        // Set once the process has exited but before it's reaped, so the watchdog can never signal a reused pid.
        let exited = Arc::new(Mutex::new(false));
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        if let Some(timeout) = timeout {
            let exited = exited.clone();
            std::thread::spawn(move || {
                if done_rx.recv_timeout(timeout).is_err() {
                    let exited = exited.lock().unwrap();
                    if !*exited {
                        // SAFETY: plain kill(2) on our own unreaped child.
                        unsafe { libc::kill(pid, libc::SIGTERM) };
                    }
                }
            });
        }
        if let Some(mut stdin) = self.child.stdin.take() {
            if let Some(input) = input {
                // The script may have exited already: the write then fails (SIGPIPE is ignored in Rust programs).
                let _ = stdin.write_all(input.as_bytes());
            }
        }
        let mut stdout = String::new();
        if let Some(mut pipe) = self.child.stdout.take() {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            stdout = String::from_utf8_lossy(&bytes).into_owned();
        }
        wait_without_reaping(pid);
        *exited.lock().unwrap() = true;
        let _ = done_tx.send(());
        let status = self.child.wait().ok().and_then(|s| s.code());
        let details = self
            .result_file
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|json| CleanupDetails::decode(&json));
        self.remove_files();
        self.reaped = true;
        ScriptOutput { status, stdout, details }
    }

    /// Ends the run at once (SIGTERM) and removes its files.
    pub fn terminate(mut self) {
        // SAFETY: the child hasn't been reaped (we own it), so the pid is still ours.
        unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM) };
        let _ = self.child.wait();
        self.remove_files();
        self.reaped = true;
    }

    fn remove_files(&mut self) {
        for file in self.files.drain(..) {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// Blocks until the process exits, leaving it a zombie (reaped later by `Child::wait`).
#[cfg(unix)]
fn wait_without_reaping(pid: libc::pid_t) {
    loop {
        // SAFETY: waitid with WNOWAIT only observes; `info` is plain data.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, libc::WEXITED | libc::WNOWAIT) };
        if result == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_result_file() {
        // The case from LogicSelfTest.swift.
        let json = br#"{"engine":"openai","error":"limit","guard":"a number (12)","rejected":"We have users.","resets":"3:45 PM","status":"guard-raw"}"#;
        let details = CleanupDetails::decode(json).unwrap();
        assert_eq!(details.guard_reason, "a number (12)");
        assert_eq!(details.rejected, "We have users.");
        assert_eq!(details.resets, "3:45 PM");
        assert_eq!(details.engine, "openai");
        assert_eq!(details.raw, "");
    }

    #[test]
    fn interprets_exit_codes() {
        let ok = interpret_refine(Some(0), "Cleaned.\n", "um cleaned", true);
        assert_eq!((ok.text.as_str(), ok.cleanup_failed, ok.offline_fallback), ("Cleaned.", false, false));
        let raw = interpret_refine(Some(3), "um cleaned", "um cleaned", true);
        assert!(raw.cleanup_failed && !raw.offline_fallback);
        let s1 = interpret_refine(Some(4), "Cleaned.", "um cleaned", true);
        assert!(s1.offline_fallback && !s1.cleanup_failed);
        let guard = interpret_refine(Some(5), "we have 12 users", "we have 12 users", true);
        assert!(!guard.cleanup_failed);
        let killed = interpret_refine(None, "", "um cleaned", true);
        assert_eq!((killed.text.as_str(), killed.cleanup_failed), ("um cleaned", true));
        let off = interpret_refine(Some(0), "text", "text", false);
        assert!(!off.cleanup_failed);
    }

    #[test]
    fn describes_problems() {
        let mut details = CleanupDetails { error: "limit".into(), resets: "3:45 PM".into(), ..Default::default() };
        assert_eq!(problem_description(&details, "claude").unwrap(), "Claude limit reached, resets 3:45 PM");
        details.resets.clear();
        assert_eq!(problem_description(&details, "openai").unwrap(), "API rate limit");
        details.error = "offline".into();
        assert_eq!(problem_description(&details, "claude"), None);
        let settings = Settings::default();
        assert_eq!(engine_name("claude", &settings), "Claude Haiku");
        assert_eq!(engine_name("openai", &settings), "the API");
    }

    #[test]
    fn cleanup_rules() {
        let mut settings = Settings::default();
        assert!(uses_cleanup(&settings, DictationMode::Code));
        assert!(!uses_cleanup(&settings, DictationMode::Raw));
        settings.cleanup_engine = "s1".into();
        assert!(!uses_cleanup(&settings, DictationMode::Code));
        settings.refine = false;
        assert!(!uses_cleanup(&settings, DictationMode::Default));
    }

    #[test]
    fn builds_the_environment() {
        let settings = Settings::default();
        let locations = Locations {
            bin_dir: Some("/usr/lib/openvoicetype/bin".into()),
            whisper_model: None,
            claude_bin: Some("/home/u/.nvm/versions/node/v22/bin/claude".into()),
        };
        let env: std::collections::HashMap<_, _> = environment(&settings, &locations, "/usr/bin").into_iter().collect();
        assert_eq!(env["VTT_QUIET"], "on");
        assert_eq!(env["VTT_REFINE"], "on");
        assert_eq!(env["VTT_LOG_TEXT"], "off");
        assert_eq!(env["VTT_BIN_DIR"], "/usr/lib/openvoicetype/bin");
        assert_eq!(env["PATH"], "/home/u/.nvm/versions/node/v22/bin:/usr/bin");
        assert!(!env.contains_key("VTT_WHISPER_MODEL"));
    }

    #[cfg(unix)]
    fn fake_script(dir: &Path, body: &str) -> PathBuf {
        let script = dir.join("fake-dictate.sh");
        std::fs::write(&script, body).unwrap();
        script
    }

    #[cfg(unix)]
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ovt-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn runs_the_contract() {
        // A stand-in for dictate.sh refine: reads stdin, writes the result file, reads and deletes the key file.
        let dir = temp_dir("run");
        let script = fake_script(
            &dir,
            r#"input="$(cat)"; key="$(cat "$VTT_OPENAI_KEY_FILE")"; rm -f "$VTT_OPENAI_KEY_FILE"
printf '{"status":"ok","engine":"claude","raw":"%s"}' "$input" >"$VTT_RESULT_FILE"
printf 'CLEANED %s %s' "$input" "$key"; exit 4"#,
        );
        let run = ScriptRun::launch(LaunchOptions {
            script: &script,
            args: &["refine"],
            env: vec![],
            state_dir: &dir,
            api_key: Some("sk-test"),
        })
        .unwrap();
        let output = run.finish(Some("hello"), Some(Duration::from_secs(10)));
        assert_eq!(output.status, Some(4));
        assert_eq!(output.stdout, "CLEANED hello sk-test");
        assert_eq!(output.details.unwrap().raw, "hello");
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("fake-dictate.sh")], "per-run files are removed");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dropped_run_is_reaped() {
        let dir = temp_dir("dropped");
        let script = fake_script(&dir, "cat >/dev/null");
        let run = ScriptRun::launch(LaunchOptions {
            script: &script,
            args: &["refine"],
            env: vec![],
            state_dir: &dir,
            api_key: None,
        })
        .unwrap();
        let pid = run.pid() as libc::pid_t;
        drop(run);
        // SAFETY: kill(2) with signal 0 only checks that the pid exists; a zombie still would.
        let reaped = || unsafe { libc::kill(pid, 0) } != 0;
        let files_left = || std::fs::read_dir(&dir).unwrap().count() > 1;
        let done = (0..100).any(|_| {
            std::thread::sleep(Duration::from_millis(20));
            reaped() && !files_left()
        });
        assert!(done, "the dropped run is reaped (reaped: {}) and its files removed", reaped());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_sends_sigterm() {
        // The script's EXIT trap must run (that's what ends a pre-started Claude), so the watchdog can't use SIGKILL.
        let dir = temp_dir("watchdog");
        let marker = dir.join("trap-ran");
        let script = fake_script(
            &dir,
            // Like dictate.sh's pre-started Claude, the background job doesn't hold stdout (else reading it would wait).
            &format!("trap 'touch {}' EXIT; cat >/dev/null; sleep 30 >/dev/null & wait", marker.display()),
        );
        let run = ScriptRun::launch(LaunchOptions {
            script: &script,
            args: &["refine"],
            env: vec![],
            state_dir: &dir,
            api_key: None,
        })
        .unwrap();
        let started = std::time::Instant::now();
        let output = run.finish(Some("x"), Some(Duration::from_millis(300)));
        assert!(started.elapsed() < Duration::from_secs(5));
        // Like dictate.sh (an EXIT trap, no TERM trap), bash runs the trap and then dies of the signal: no exit code.
        assert_eq!(output.status, None);
        assert!(marker.exists(), "the EXIT trap ran");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn empty_input_ends_a_prestart() {
        let dir = temp_dir("prestart");
        let script = fake_script(&dir, r#"input="$(cat)"; [ -n "$input" ] || exit 0; echo never"#);
        let run = ScriptRun::launch(LaunchOptions {
            script: &script,
            args: &["refine"],
            env: vec![],
            state_dir: &dir,
            api_key: None,
        })
        .unwrap();
        let output = run.finish(None, None);
        assert_eq!((output.status, output.stdout.as_str()), (Some(0), ""));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
