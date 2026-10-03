//! The Claude CLI (`claude_options`, `claude_command`, `claude_prestart`, `claude_send`, `refine`'s one-shot call,
//! `claude_parse`, `claude_plain`). The same flags, environment and failure classes as `dictate.sh`; the pre-start uses
//! an ordinary pipe as `claude.exe`'s stdin (the script's fifo doesn't work for a native Windows program).
//!
//! Every call runs the user's own `claude` in print mode from a neutral directory (the temp folder, so no project
//! CLAUDE.md is found), with no tools and no MCP servers. Never `--bare`: bare mode ignores the subscription login.

use crate::config::{Config, Job};
use crate::refine::log;
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Why the online engine failed: `engine_error`'s kind (limit, auth, auth-mismatch, offline, timeout, config, error),
/// the raw reset time (formatted later) and a short detail for the log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineError {
    pub kind: String,
    pub resets: String,
    pub detail: String,
}

impl EngineError {
    /// `engine_error <kind> [detail]`.
    pub fn new(kind: &str, detail: &str) -> Self {
        EngineError { kind: kind.into(), resets: String::new(), detail: detail.into() }
    }
}

/// `claude_parse`'s three outcomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    /// Exit 0: the answer.
    Answer(String),
    /// Exit 1: Claude failed; the error says why when the events did (else kind "error").
    Failed(EngineError),
    /// Exit 2: nothing understandable came back (the format may have changed).
    Unparsed,
}

/// `claude_parse`: reads the JSON events (stream-json lines, or the one-shot call's single object). A rate-limit
/// warning is logged to `log_file` like the script does.
pub fn parse(events: &str, log_file: Option<&Path>) -> Parsed {
    match parse_events(events, log_file) {
        Events::Answer(answer) => Parsed::Answer(answer),
        Events::Failed(error) => Parsed::Failed(error.unwrap_or_else(|| EngineError::new("error", ""))),
        Events::Unparsed => Parsed::Unparsed,
    }
}

/// `claude_parse` with exit 1 split in two: whether it wrote `ENGINE_ERR_FILE` (Some) or not (None, so the caller's
/// own `engine_error` applies, as in `claude_send`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Events {
    Answer(String),
    Failed(Option<EngineError>),
    Unparsed,
}

fn regex(cell: &'static OnceLock<fancy_regex::Regex>, pattern: &str) -> &'static fancy_regex::Regex {
    cell.get_or_init(|| fancy_regex::Regex::new(pattern).expect("valid regex"))
}

fn is_match(cell: &'static OnceLock<fancy_regex::Regex>, pattern: &str, text: &str) -> bool {
    regex(cell, pattern).is_match(text).unwrap_or(false)
}

pub(crate) fn parse_events(events: &str, log_file: Option<&Path>) -> Events {
    static LIMIT: OnceLock<fancy_regex::Regex> = OnceLock::new();
    static AUTH: OnceLock<fancy_regex::Regex> = OnceLock::new();
    static RESETS: OnceLock<fancy_regex::Regex> = OnceLock::new();
    static USED: OnceLock<fancy_regex::Regex> = OnceLock::new();
    let classify = |error: &Value| match perl_string(error).unwrap_or_default().as_str() {
        "rate_limit" | "billing_error" | "credits_required" => "limit",
        "authentication_failed" | "oauth_org_not_allowed" | "account_on_hold" => "auth",
        _ => "",
    };
    let mut answer: Option<String> = None;
    let (mut failed, mut bad, mut count) = (false, 0, 0);
    let (mut kind, mut retry_kind, mut resets, mut detail) = (String::new(), "", String::new(), String::new());
    for line in events.split('\n') {
        // perl's /\S/ on bytes: any byte but ASCII whitespace.
        if line.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)) {
            continue;
        }
        let event = match serde_json::from_str::<Value>(line) {
            Ok(event @ Value::Object(_)) if perl_true(&event["type"]) => event,
            _ => {
                bad += 1;
                continue;
            }
        };
        count += 1;
        match perl_string(&event["type"]).unwrap_or_default().as_str() {
            "rate_limit_event" => {
                let info = &event["rate_limit_info"];
                let info = if info.is_object() { info } else { &Value::Null };
                let status = perl_string(&info["status"]).unwrap_or_default();
                if status == "rejected" {
                    kind = "limit".into();
                    if let Some(at) = perl_string(&info["resetsAt"]) {
                        resets = at;
                    }
                } else if status == "allowed_warning" {
                    if let Some(path) = log_file {
                        let mut used = perl_string(&info["utilization"]).unwrap_or_else(|| "?".into());
                        if is_match(&USED, r"^[0-9.]+\n?\z", &used) && perl_number(&used) <= 1.0 {
                            used = format!("{:.0}%", perl_number(&used) * 100.0);
                        }
                        append_log(path, &format!("WARN claude usage: {used} of the plan limit used"));
                    }
                }
            }
            "system" if perl_string(&event["subtype"]).as_deref() == Some("api_retry") => {
                if retry_kind.is_empty() {
                    retry_kind = classify(&event["error"]);
                }
            }
            "assistant" => {
                if kind.is_empty() {
                    kind = classify(&event["error"]).into();
                }
                if let Value::Array(content) = &event["message"]["content"] {
                    let text: String = content
                        .iter()
                        .filter(|part| part.is_object() && perl_string(&part["type"]).as_deref() == Some("text"))
                        .map(|part| perl_string(&part["text"]).unwrap_or_default())
                        .collect();
                    if !text.is_empty() {
                        answer = Some(text);
                    }
                }
            }
            "result" => {
                // A reference (object, array, JSON boolean) counts as no text.
                let text = match &event["result"] {
                    Value::Object(_) | Value::Array(_) | Value::Bool(_) => String::new(),
                    value => perl_string(value).unwrap_or_default(),
                };
                if perl_true(&event["is_error"]) {
                    failed = true;
                    let status = match &event["api_error_status"] {
                        Value::Null => 0.0,
                        value => perl_number(&perl_string(value).unwrap_or_default()),
                    };
                    detail = squeeze(&text.chars().take(200).collect::<String>(), &['\t', '\n', '\x1f']);
                    if kind.is_empty() {
                        let limit = r"(?i)limit reached|hit your .{0,20}limit|(?:usage|rate|session|weekly) limit";
                        let auth = r"(?i)log ?in|sign ?in|authenticat|oauth|api key|credential";
                        if status == 429.0 || is_match(&LIMIT, limit, &text) {
                            kind = "limit".into();
                        } else if status == 401.0 || status == 403.0 || is_match(&AUTH, auth, &text) {
                            kind = "auth".into();
                        }
                    }
                    if let Ok(Some(found)) = regex(&RESETS, r"(?i)resets? (?:at )?([^.\x{b7}\n]+)").captures(&text) {
                        if resets.is_empty() || resets == "0" {
                            resets = found[1].to_string();
                        }
                    }
                } else if !text.is_empty() {
                    answer = Some(text);
                }
                break;
            }
            _ => {}
        }
    }
    if let (false, Some(answer)) = (failed, &answer) {
        if !answer.is_empty() {
            return Events::Answer(answer.clone());
        }
    }
    let reason = if kind.is_empty() { retry_kind.to_string() } else { kind };
    if failed || !reason.is_empty() {
        let kind = if reason.is_empty() { "error".into() } else { reason };
        return Events::Failed(Some(EngineError { kind, resets, detail }));
    }
    if bad > 0 || count == 0 {
        Events::Unparsed
    } else {
        Events::Failed(None)
    }
}

/// Runs of the given characters become one space (`s/[\t\n\x1f]+/ /g`).
fn squeeze(text: &str, chars: &[char]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if chars.contains(&c) {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// A JSON value as perl sees it in a string context; None for null (undef).
pub(crate) fn perl_string(value: &Value) -> Option<String> {
    Some(match value {
        Value::Null => return None,
        Value::Bool(b) => (if *b { "1" } else { "0" }).into(),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            (_, _, Some(f)) => f.to_string(),
            _ => n.to_string(),
        },
        Value::String(s) => s.clone(),
        Value::Array(_) => "ARRAY".into(),
        Value::Object(_) => "HASH".into(),
    })
}

/// perl's truth: undef, "", "0" and 0 are false.
fn perl_true(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !(s.is_empty() || s == "0"),
        _ => true,
    }
}

/// perl's numeric value of a string: its leading number, else 0.
fn perl_number(text: &str) -> f64 {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let mut end = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end = 1;
    }
    let digits = |from: usize| from + bytes[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    end = digits(end);
    if bytes.get(end) == Some(&b'.') {
        end = digits(end + 1);
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        let after = digits(exponent);
        if after > exponent {
            end = after;
        }
    }
    text[..end].parse().unwrap_or(0.0)
}

/// Appends a log line (`log`'s format) to `path`.
pub(crate) fn append_log(path: &Path, line: &str) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    if let Ok(mut file) = options.open(path) {
        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = file.write_all(format!("{stamp} {line}\n").as_bytes());
    }
}

// --- Processes ---

/// `$ERR_FILE`: error.log next to dictate.log.
pub(crate) fn error_log(config: &Config) -> PathBuf {
    config.log_file.with_file_name("error.log")
}

/// A program's stderr appended to `path` (else discarded).
pub(crate) fn append_to(path: &Path) -> Stdio {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    OpenOptions::new().create(true).append(true).open(path).map(Stdio::from).unwrap_or_else(|_| Stdio::null())
}

/// A command with no console window on Windows, and on Unix the script's `PATH` (Homebrew and `~/.local/bin` first, so
/// an npm-installed claude finds `node`).
pub(crate) fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // CREATE_NO_WINDOW
    #[cfg(unix)]
    if let Ok(path) = std::env::join_paths(search_path()) {
        command.env("PATH", path);
    }
    command
}

/// The script's `PATH`: `/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH:/usr/lib/openvoicetype/bin`.
#[cfg(unix)]
fn search_path() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut dirs = vec![PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin"), home.join(".local/bin")];
    dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    dirs.push("/usr/lib/openvoicetype/bin".into());
    dirs
}

/// `command -v <name>`: a path is used as it is; a name is looked up. Windows: the native installer's
/// `%USERPROFILE%\.local\bin\<name>.exe` (not on `PATH`), then `PATH` (`.exe`, then npm's `.cmd`).
pub(crate) fn find_program(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if name.is_empty() {
        return None;
    }
    if path.is_absolute() || path.components().count() > 1 {
        return is_program(path).then(|| path.to_path_buf());
    }
    #[cfg(unix)]
    {
        search_path().into_iter().map(|dir| dir.join(name)).find(|p| is_program(p))
    }
    #[cfg(windows)]
    {
        let names: Vec<String> = if path.extension().is_some() {
            vec![name.to_string()]
        } else {
            vec![format!("{name}.exe"), format!("{name}.cmd")]
        };
        let mut candidates = Vec::new();
        if let Some(home) = std::env::var_os("USERPROFILE") {
            candidates.push(PathBuf::from(home).join(".local").join("bin").join(&names[0]));
        }
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            candidates.extend(names.iter().map(|n| dir.join(n)));
        }
        candidates.into_iter().find(|p| is_program(p))
    }
}

fn is_program(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// The claude CLI this config runs (`command -v "$CLAUDE_BIN"`), or None if it isn't installed.
pub fn binary(config: &Config) -> Option<PathBuf> {
    find_program(&config.claude_bin)
}

/// The neutral working directory: no project CLAUDE.md is found there.
fn neutral_dir() -> PathBuf {
    std::env::temp_dir()
}

pub(crate) struct Ran {
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}

fn read_all(mut from: impl Read + Send + 'static) -> mpsc::Receiver<Vec<u8>> {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = from.read_to_end(&mut buffer);
        let _ = send.send(buffer);
    });
    receive
}

/// Runs a command with `input` on its stdin (else none), collecting stdout (and stderr if the caller set it to piped),
/// and kills it after `limit` (perl's `alarm`).
pub(crate) fn run(command: &mut Command, input: Option<Vec<u8>>, limit: Option<Duration>) -> io::Result<Ran> {
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped());
    let mut child = command.spawn()?;
    if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
        // A thread: a big message mustn't block on a full pipe before the program reads it.
        std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
    }
    let stdout = child.stdout.take().map(read_all);
    let stderr = child.stderr.take().map(read_all);
    let deadline = limit.map(|limit| Instant::now() + limit);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // A grandchild could keep the pipe open: don't wait for it forever.
    let collect = |from: Option<mpsc::Receiver<Vec<u8>>>| {
        from.and_then(|r| r.recv_timeout(Duration::from_secs(if timed_out { 0 } else { 2 })).ok()).unwrap_or_default()
    };
    Ok(Ran { status, stdout: collect(stdout), stderr: collect(stderr), timed_out })
}

// --- Options and the command ---

/// The optional flags this CLI supports (`claude_options`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Options {
    pub safe_mode: bool,
    pub disable_slash_commands: bool,
    pub system_prompt_file: bool,
}

/// `file_stamp`: "size-mtime", following symlinks.
fn file_stamp(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(format!("{}-{mtime}", meta.len()))
}

/// `claude_options`: checked once per binary (path, size and date) and cached in `<state_dir>/claude-options`, in the
/// script's format (the signature, then one flag per line). `--system-prompt-file` isn't listed in `--help`, so it's
/// probed: with a missing file, a CLI that knows it says "not found".
pub(crate) fn options(config: &Config, bin: &Path) -> Options {
    let signature = format!("{} {}", bin.display(), file_stamp(bin).unwrap_or_default());
    let cache = config.state_dir.join("claude-options");
    let from_lines = |lines: &[&str]| Options {
        safe_mode: lines.contains(&"--safe-mode"),
        disable_slash_commands: lines.contains(&"--disable-slash-commands"),
        system_prompt_file: lines.contains(&"--system-prompt-file"),
    };
    if let Ok(text) = fs::read_to_string(&cache) {
        let lines: Vec<&str> = text.lines().collect();
        if lines.first() == Some(&signature.as_str()) {
            return from_lines(&lines[1..]);
        }
    }
    let probe = |args: &[&str], limit: u64, with_stderr: bool| {
        let mut command = command(bin);
        command.args(args).current_dir(neutral_dir());
        command.stderr(if with_stderr { Stdio::piped() } else { Stdio::null() });
        run(&mut command, None, Some(Duration::from_secs(limit))).ok()
    };
    let help = probe(&["--help"], 10, false)
        .filter(|ran| ran.status.is_some_and(|s| s.success()))
        .map(|ran| String::from_utf8_lossy(&ran.stdout).into_owned())
        .unwrap_or_default();
    let probe = probe(&["-p", "--system-prompt-file", "/nonexistent/vtt-probe"], 15, true)
        .map(|ran| String::from_utf8_lossy(&[ran.stdout, ran.stderr].concat()).into_owned())
        .unwrap_or_default();
    let mut lines = Vec::new();
    if help.contains("--safe-mode") {
        lines.push("--safe-mode");
    }
    if help.contains("--disable-slash-commands") {
        lines.push("--disable-slash-commands");
    }
    if probe.contains("not found") {
        lines.push("--system-prompt-file");
    }
    let _ =
        write_private(&cache, &format!("{signature}\n{}", lines.iter().map(|l| format!("{l}\n")).collect::<String>()));
    from_lines(&lines)
}

/// Writes a file only the user can read.
fn write_private(path: &Path, content: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    create_private(path)?.write_all(content.as_bytes())
}

/// The system prompt in a private file for `--system-prompt-file`, removed when dropped.
struct PromptFile(PathBuf);

impl PromptFile {
    fn create(dir: &Path, content: &str) -> io::Result<Self> {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let name = format!("system.{}-{nanos}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed));
        let path = dir.join(name);
        write_private(&path, content)?;
        Ok(PromptFile(path))
    }
}

impl Drop for PromptFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// The API key rule of `claude_command` and `claude_plain`: an exported key would switch the user to API billing.
fn drop_api_key(command: &mut Command, config: &Config) {
    if !config.claude_use_api_key {
        command.env_remove("ANTHROPIC_API_KEY").env_remove("ANTHROPIC_AUTH_TOKEN");
    }
}

/// `claude_command`: print mode with our environment (extended thinking off, one retry at most, no auto-update, no
/// claude.ai connectors), no tools, no MCP servers, and the supported optional flags.
pub(crate) fn claude_command(config: &Config, bin: &Path, options: Options) -> Command {
    let mut command = command(bin);
    drop_api_key(&mut command, config);
    command
        .env("MAX_THINKING_TOKENS", config.claude_thinking_tokens.to_string())
        .env("CLAUDE_CODE_MAX_RETRIES", "1")
        .env("CLAUDE_CODE_STARTUP_FAILURE_RESULTS", "1")
        .env("DISABLE_AUTOUPDATER", "1")
        .env("ENABLE_CLAUDEAI_MCP_SERVERS", "false")
        .args(["-p", "--model", &config.claude_model, "--tools", "", "--strict-mcp-config", "--no-session-persistence"])
        .current_dir(neutral_dir());
    if options.safe_mode {
        command.arg("--safe-mode");
    }
    if options.disable_slash_commands {
        command.arg("--disable-slash-commands");
    }
    command
}

/// `claude_prompt_args`: the system prompt from a private file when the CLI supports it (it isn't then in the process
/// list), else as an argument.
fn prompt_args(
    command: &mut Command,
    config: &Config,
    options: Options,
    prompt: &str,
) -> io::Result<Option<PromptFile>> {
    if options.system_prompt_file {
        let file = PromptFile::create(&config.state_dir, prompt)?;
        command.arg("--system-prompt-file").arg(&file.0);
        Ok(Some(file))
    } else {
        command.arg("--system-prompt").arg(prompt);
        Ok(None)
    }
}

/// `CLAUDE_TIMEOUT`, which `cmd_command` sets to `COMMAND_TIMEOUT`.
fn timeout(config: &Config) -> Duration {
    Duration::from_secs(if config.job == Job::Command { config.command_timeout } else { config.claude_timeout })
}

/// `$(...)`: the shell drops trailing newlines from an answer.
pub(crate) fn chomp(text: &str) -> String {
    text.trim_end_matches('\n').to_string()
}

/// `[[ -z "$(trim "$x")" ]]`.
pub(crate) fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

// --- Pre-started Claude ---

/// What `Prestarted::start` did.
pub enum Prestart {
    /// Pre-starting doesn't apply (off, another engine, raw mode, no claude, or it couldn't start).
    Off,
    /// Offline when recording started (`PRESTART_OFFLINE`): cleanup skips Claude.
    Offline,
    Started(Prestarted),
}

/// A `claude -p` started before the transcript exists (`claude_prestart`): stream-json in and out, stdin held open.
pub struct Prestarted {
    child: Child,
    stdin: Option<ChildStdin>,
    output: Arc<(Mutex<Output>, Condvar)>,
    _prompt: Option<PromptFile>,
}

/// Claude's stdout so far, from the reader thread (the script's `$PRESTART_DIR/out`).
#[derive(Default)]
struct Output {
    lines: Vec<String>,
    bytes: usize,
    eof: bool,
}

impl Prestarted {
    /// `claude_prestart`. `system_prompt` is only built when a process starts.
    pub fn start(config: &Config, system_prompt: impl FnOnce() -> String) -> Prestart {
        if !config.claude_prestart || config.online_engine() != "claude" {
            return Prestart::Off;
        }
        if !(config.job == Job::Command || (config.refine && config.mode != "raw")) {
            return Prestart::Off;
        }
        let Some(bin) = binary(config) else { return Prestart::Off };
        if crate::online::is_offline(config, None) {
            log(
                config,
                match config.job {
                    Job::Command => "OFFLINE at start, Command Mode can't run",
                    Job::Cleanup => "OFFLINE at start, warming S1-mini",
                },
            );
            return Prestart::Offline;
        }
        match Self::spawn(config, &bin, &system_prompt()) {
            Ok(started) => Prestart::Started(started),
            Err(_) => Prestart::Off,
        }
    }

    fn spawn(config: &Config, bin: &Path, system_prompt: &str) -> io::Result<Prestarted> {
        let options = options(config, bin);
        let mut command = claude_command(config, bin, options);
        command.args(["--input-format", "stream-json", "--output-format", "stream-json", "--verbose"]);
        let prompt = prompt_args(&mut command, config, options, system_prompt)?;
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(append_to(&error_log(config)));
        let mut child = command.spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or_else(|| io::Error::other("no stdout"))?;
        let output = Arc::new((Mutex::new(Output::default()), Condvar::new()));
        let shared = Arc::clone(&output);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut buffer = Vec::new();
            let (lock, ready) = &*shared;
            loop {
                buffer.clear();
                match reader.read_until(b'\n', &mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let line = String::from_utf8_lossy(buffer.strip_suffix(b"\n").unwrap_or(&buffer)).into_owned();
                        let mut out = lock.lock().unwrap_or_else(|e| e.into_inner());
                        out.bytes += n;
                        out.lines.push(line);
                        ready.notify_all();
                    }
                }
            }
            lock.lock().unwrap_or_else(|e| e.into_inner()).eof = true;
            ready.notify_all();
        });
        Ok(Prestarted { child, stdin, output, _prompt: prompt })
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// `claude_send`: one user message, then the wait with the script's early exits. Ok(answer), Err(Failed) or
    /// Err(Unparsed) (then the caller retries with a one-shot call).
    pub fn send(mut self, config: &Config, user_message: &str) -> Result<String, Parsed> {
        static RETRY: OnceLock<fancy_regex::Regex> = OnceLock::new();
        let message = serde_json::json!({"type": "user", "message": {"role": "user", "content": user_message}});
        let written = match self.stdin.as_mut() {
            Some(stdin) => stdin.write_all(format!("{message}\n").as_bytes()).and_then(|_| stdin.flush()),
            None => Err(io::ErrorKind::BrokenPipe.into()),
        };
        if written.is_err() {
            return Err(Parsed::Unparsed);
        }
        // Wait for the result up to the timeout; stop early when waiting can't help.
        let sent = Instant::now();
        let limit = timeout(config);
        let (mut answer_at, mut timed_out) = (None::<Instant>, false);
        let output = Arc::clone(&self.output);
        let (lock, ready) = &*output;
        let mut out = lock.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            let any = |needle: &str| out.lines.iter().any(|line| line.contains(needle));
            if any(r#""type":"result""#) {
                break;
            }
            if !self.alive() {
                // It's gone: take what it printed.
                let until = Instant::now() + Duration::from_secs(1);
                while !out.eof && Instant::now() < until {
                    out = ready.wait_timeout(out, Duration::from_millis(50)).unwrap_or_else(|e| e.into_inner()).0;
                }
                break;
            }
            if out.lines.iter().any(|line| !line.starts_with('{')) {
                break; // a line that isn't JSON: the format changed
            }
            if any(r#""status":"rejected""#) {
                break; // usage limit reached
            }
            let retry =
                r#""error":"(rate_limit|billing_error|authentication_failed|oauth_org_not_allowed|account_on_hold)""#;
            if out.lines.iter().any(|line| line.contains(r#""api_retry""#) && is_match(&RETRY, retry, line)) {
                break;
            }
            let now = Instant::now();
            if now - sent >= limit || (now - sent >= Duration::from_secs(5) && out.bytes == 0) {
                timed_out = true; // no event at all after 5 s: stuck
                break;
            }
            if answer_at.is_none() && any(r#""type":"assistant""#) {
                answer_at = Some(now);
            }
            if answer_at.is_some_and(|at| now - at >= Duration::from_millis(1500)) {
                break; // an answer but no result after 1.5 s: use the answer
            }
            out = ready.wait_timeout(out, Duration::from_millis(50)).unwrap_or_else(|e| e.into_inner()).0;
        }
        let (events, empty) = (out.lines.join("\n"), out.bytes == 0);
        drop(out);
        if empty {
            return Err(if self.alive() { Parsed::Failed(EngineError::new("timeout", "")) } else { Parsed::Unparsed });
        }
        match parse_events(&events, Some(&config.log_file)) {
            Events::Answer(answer) => Ok(answer),
            Events::Failed(Some(error)) => Err(Parsed::Failed(error)),
            Events::Failed(None) => {
                Err(Parsed::Failed(EngineError::new(if timed_out { "timeout" } else { "error" }, "")))
            }
            Events::Unparsed => Err(Parsed::Unparsed),
        }
    }
}

impl Drop for Prestarted {
    /// `claude_cleanup`: closes its stdin, ends it and reaps it.
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `refine` (Claude): the pre-started process when there is one, else (or when its stream wasn't understood: a Claude
/// Code update may have changed the flags or format) a one-shot call.
pub fn refine(
    config: &Config,
    prestarted: Option<Prestarted>,
    system_prompt: &str,
    user_message: &str,
) -> Result<String, EngineError> {
    if let Some(prestarted) = prestarted {
        match prestarted.send(config, user_message) {
            Ok(out) if !blank(&chomp(&out)) => return Ok(chomp(&out)),
            Ok(_) | Err(Parsed::Answer(_)) => return Err(EngineError::new("error", "")),
            Err(Parsed::Failed(error)) => return Err(error),
            Err(Parsed::Unparsed) => log(
                config,
                &format!(
                    "WARN claude stream-json not understood (see {}), using a one-shot call",
                    error_log(config).display()
                ),
            ),
        }
    }
    one_shot(config, system_prompt, user_message)
}

/// `refine`'s one-shot `claude -p --output-format json`, with `CLAUDE_TIMEOUT` (`COMMAND_TIMEOUT` in Command Mode).
pub fn one_shot(config: &Config, system_prompt: &str, user_message: &str) -> Result<String, EngineError> {
    let error = |detail: &str| Err(EngineError::new("error", detail));
    let Some(bin) = binary(config) else { return error("") };
    let options = options(config, &bin);
    let mut command = claude_command(config, &bin, options);
    command.args(["--output-format", "json"]);
    let Ok(_prompt) = prompt_args(&mut command, config, options, system_prompt) else { return error("") };
    command.stderr(append_to(&error_log(config)));
    let limit = timeout(config);
    let ran = match run(&mut command, Some(user_message.as_bytes().to_vec()), Some(limit)) {
        Ok(ran) => ran,
        Err(_) => return error(""),
    };
    if ran.timed_out {
        return Err(EngineError::new("timeout", ""));
    }
    // An error still prints its JSON result (and exits 1): the output is parsed whatever the exit status.
    match parse_events(&String::from_utf8_lossy(&ran.stdout), Some(&config.log_file)) {
        Events::Answer(answer) if blank(&chomp(&answer)) => error("empty answer"),
        Events::Answer(answer) => Ok(chomp(&answer)),
        Events::Failed(Some(failed)) => Err(failed),
        _ => error(""),
    }
}

/// `claude_plain auth status --json` says signed in (the `auth-mismatch` canary). 10 s at most.
pub fn signed_in(config: &Config) -> bool {
    static SIGNED_IN: OnceLock<fancy_regex::Regex> = OnceLock::new();
    let Some(bin) = binary(config) else { return false };
    let mut command = command(&bin);
    drop_api_key(&mut command, config);
    command.args(["auth", "status", "--json"]).stderr(Stdio::null());
    run(&mut command, None, Some(Duration::from_secs(10)))
        .is_ok_and(|ran| is_match(&SIGNED_IN, r#""loggedIn": *true"#, &String::from_utf8_lossy(&ran.stdout)))
}

/// Creates (or empties) a file only the user can read (the script's `umask 077`).
pub(crate) fn create_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// golden/claude.json, made by golden/make-claude.sh from dictate.sh's own `claude_parse`.
    #[test]
    fn parse_matches_the_script() {
        let inputs: Value = serde_json::from_str(include_str!("../golden/claude-inputs.json")).unwrap();
        let golden: Value = serde_json::from_str(include_str!("../golden/claude.json")).unwrap();
        let cases = golden["cases"].as_array().unwrap();
        assert_eq!(cases.len(), inputs["cases"].as_array().unwrap().len());
        let dir = test_dir("golden");
        for (input, want) in inputs["cases"].as_array().unwrap().iter().zip(cases) {
            let name = input["name"].as_str().unwrap();
            assert_eq!(want["name"].as_str(), Some(name));
            let events: String = input["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|line| match line {
                    Value::String(s) => format!("{s}\n"),
                    other => format!("{other}\n"),
                })
                .collect();
            let log_file = dir.join(format!("{name}.log"));
            let (exit, stdout, error) = match parse_events(&events, Some(&log_file)) {
                Events::Answer(answer) => (0, answer, String::new()),
                Events::Failed(None) => (1, String::new(), String::new()),
                Events::Failed(Some(e)) => (1, String::new(), format!("{}\x1f{}\x1f{}\n", e.kind, e.resets, e.detail)),
                Events::Unparsed => (2, String::new(), String::new()),
            };
            let log = fs::read_to_string(&log_file).unwrap_or_default();
            let log: String = log.lines().map(|line| format!("<time> {}\n", &line[20..])).collect();
            assert_eq!(exit, want["exit"].as_i64().unwrap(), "{name}: exit");
            assert_eq!(stdout, want["stdout"].as_str().unwrap(), "{name}: stdout");
            assert_eq!(error, want["error"].as_str().unwrap(), "{name}: error");
            assert_eq!(log, want["log"].as_str().unwrap(), "{name}: log");
        }
        // The public form: exit 1 without a reason is kind "error".
        assert_eq!(parse(r#"{"type":"system","subtype":"init"}"#, None), Parsed::Failed(EngineError::new("error", "")));
    }

    #[test]
    fn perl_numbers_and_strings() {
        assert_eq!(perl_number("429"), 429.0);
        assert_eq!(perl_number(" 0.1.2"), 0.1);
        assert_eq!(perl_number("abc"), 0.0);
        assert_eq!(perl_number("1e3x"), 1000.0);
        assert_eq!(perl_string(&serde_json::json!(1790000000)).unwrap(), "1790000000");
        assert_eq!(perl_string(&serde_json::json!(0.85)).unwrap(), "0.85");
        assert!(!perl_true(&serde_json::json!("0")) && perl_true(&serde_json::json!("false")));
        assert_eq!(squeeze("a\t\tb\nc\x1fd", &['\t', '\n', '\x1f']), "a b c d");
    }

    #[test]
    fn the_command_drops_the_api_key_and_never_uses_bare() {
        let config = Config { claude_model: "haiku".into(), ..Config::default() };
        let all = Options { safe_mode: true, disable_slash_commands: true, system_prompt_file: true };
        let command = claude_command(&config, Path::new("claude"), all);
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(
            args,
            [
                "-p",
                "--model",
                "haiku",
                "--tools",
                "",
                "--strict-mcp-config",
                "--no-session-persistence",
                "--safe-mode",
                "--disable-slash-commands"
            ]
        );
        let envs: Vec<_> = command
            .get_envs()
            .filter(|(k, _)| *k != "PATH")
            .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned())))
            .collect();
        let get = |name: &str| envs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        assert_eq!(get("ANTHROPIC_API_KEY"), Some(None));
        assert_eq!(get("ANTHROPIC_AUTH_TOKEN"), Some(None));
        assert_eq!(get("MAX_THINKING_TOKENS"), Some(Some("0".into())));
        assert_eq!(get("CLAUDE_CODE_MAX_RETRIES"), Some(Some("1".into())));
        assert_eq!(get("ENABLE_CLAUDEAI_MCP_SERVERS"), Some(Some("false".into())));
        assert_eq!(command.get_current_dir(), Some(std::env::temp_dir().as_path()));
        let keep =
            claude_command(&Config { claude_use_api_key: true, ..config }, Path::new("claude"), Options::default());
        assert!(!keep.get_envs().any(|(k, _)| k == "ANTHROPIC_API_KEY"));
        assert!(!keep.get_args().any(|a| a == "--safe-mode" || a == "--bare"));
    }

    /// A fresh folder for one test.
    pub(crate) fn test_dir(name: &str) -> PathBuf {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ovt-pipeline-test-{}-{name}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// tests/fake-claude: its "cleanup" is the transcript in capitals; FAKE_MODE plays the failures.
    #[cfg(unix)]
    mod fake {
        use super::*;

        fn config(name: &str, mode: &str) -> Config {
            let dir = test_dir(name);
            // The mode reaches the fake through its name: the tests share one environment.
            let fake = dir.join("claude");
            let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake-claude");
            fs::write(
                &fake,
                format!(
                    "#!/bin/sh\nFAKE_MODE={mode} FAKE_LOG='{}' exec '{}' \"$@\"\n",
                    dir.join("args").display(),
                    script.display()
                ),
            )
            .unwrap();
            fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
            Config {
                claude_bin: fake.to_string_lossy().into_owned(),
                state_dir: dir.join("state"),
                log_file: dir.join("logs/dictate.log"),
                online_check: false,
                claude_timeout: 4,
                ..Config::default()
            }
        }

        const MESSAGE: &str = "<context app=\"\" mode=\"default\"/>\n<transcript>\nhello there\n</transcript>\n";

        fn args(config: &Config) -> String {
            fs::read_to_string(config.log_file.parent().unwrap().parent().unwrap().join("args")).unwrap_or_default()
        }

        fn started(config: &Config) -> Prestarted {
            match Prestarted::start(config, || "PROMPT".into()) {
                Prestart::Started(p) => p,
                _ => panic!("not started"),
            }
        }

        #[test]
        fn options_are_probed_once_and_cached() {
            let config = config("options", "ok");
            let bin = binary(&config).unwrap();
            let want = Options { safe_mode: true, disable_slash_commands: true, system_prompt_file: true };
            assert_eq!(options(&config, &bin), want);
            let cache = fs::read_to_string(config.state_dir.join("claude-options")).unwrap();
            let mut lines = cache.lines();
            assert!(lines.next().unwrap().starts_with(&format!("{} ", bin.display())));
            assert_eq!(lines.collect::<Vec<_>>(), ["--safe-mode", "--disable-slash-commands", "--system-prompt-file"]);
            fs::write(
                config.state_dir.join("claude-options"),
                format!("{}\n--safe-mode\n", cache.lines().next().unwrap()),
            )
            .unwrap();
            assert_eq!(options(&config, &bin), Options { safe_mode: true, ..Options::default() });
        }

        #[test]
        fn one_shot_cleans_up() {
            let config = config("one-shot", "ok");
            assert_eq!(one_shot(&config, "PROMPT", MESSAGE), Ok("HELLO THERE".into()));
            let args = args(&config);
            assert!(args.contains("-p --model haiku --tools  --strict-mcp-config --no-session-persistence --safe-mode"));
            assert!(args.contains("--output-format json --system-prompt-file "), "{args}");
            let tmp = fs::canonicalize(std::env::temp_dir()).unwrap();
            assert!(args.contains(&format!("cwd={}", tmp.display())), "{args}");
            assert!(!args.contains("--bare"));
            // The prompt file is gone.
            assert!(!fs::read_dir(&config.state_dir).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("system.")));
        }

        #[test]
        fn one_shot_reports_a_limit() {
            let config = config("limit", "limit");
            let error = one_shot(&config, "PROMPT", MESSAGE).unwrap_err();
            assert_eq!((error.kind.as_str(), error.resets.as_str()), ("limit", "1790000000"));
        }

        #[test]
        fn one_shot_times_out() {
            let config = Config { claude_timeout: 1, ..config("timeout", "slow") };
            let t0 = Instant::now();
            assert_eq!(one_shot(&config, "PROMPT", MESSAGE).unwrap_err().kind, "timeout");
            assert!(t0.elapsed() < Duration::from_secs(4));
        }

        #[test]
        fn prestarted_answers() {
            let config = config("prestart", "ok");
            let p = started(&config);
            assert_eq!(p.send(&config, MESSAGE), Ok("HELLO THERE".into()));
            assert!(args(&config).contains("--input-format stream-json --output-format stream-json --verbose"));
        }

        #[test]
        fn prestarted_stream_not_understood_falls_back_to_one_shot() {
            let config = config("garbage", "garbage");
            assert_eq!(started(&config).send(&config, MESSAGE), Err(Parsed::Unparsed));
            assert_eq!(refine(&config, Some(started(&config)), "PROMPT", MESSAGE), Ok("HELLO THERE".into()));
            let log = fs::read_to_string(&config.log_file).unwrap();
            assert!(log.contains(" WARN claude stream-json not understood (see "), "{log}");
        }

        #[test]
        fn prestarted_died_without_a_word() {
            let config = config("die", "die");
            assert_eq!(started(&config).send(&config, MESSAGE), Err(Parsed::Unparsed));
        }

        #[test]
        fn prestarted_stuck_times_out() {
            let config = Config { claude_timeout: 1, ..config("stuck", "slow") };
            let t0 = Instant::now();
            assert_eq!(started(&config).send(&config, MESSAGE), Err(Parsed::Failed(EngineError::new("timeout", ""))));
            assert!(t0.elapsed() < Duration::from_secs(3));
        }

        #[test]
        fn prestarted_limit_ends_the_wait() {
            let config = Config { claude_timeout: 10, ..config("stream-limit", "stream-limit") };
            let t0 = Instant::now();
            let error = match started(&config).send(&config, MESSAGE) {
                Err(Parsed::Failed(error)) => error,
                other => panic!("{other:?}"),
            };
            assert_eq!((error.kind.as_str(), error.resets.as_str()), ("limit", "1790000000"));
            assert!(t0.elapsed() < Duration::from_secs(3));
        }

        #[test]
        fn prestarted_answer_without_result_is_used() {
            let config = Config { claude_timeout: 10, ..config("assistant", "assistant") };
            let t0 = Instant::now();
            assert_eq!(started(&config).send(&config, MESSAGE), Ok("HELLO THERE".into()));
            let waited = t0.elapsed();
            assert!(waited >= Duration::from_millis(1500) && waited < Duration::from_secs(4), "{waited:?}");
        }

        #[test]
        fn prestart_off_and_offline() {
            let config = config("off", "ok");
            let raw = Config { mode: "raw".into(), ..config.clone() };
            assert!(matches!(Prestarted::start(&raw, || unreachable!()), Prestart::Off));
            let openai = Config { cleanup: "openai".into(), ..config.clone() };
            assert!(matches!(Prestarted::start(&openai, || unreachable!()), Prestart::Off));
            let offline = Config { force_offline: true, ..config.clone() };
            assert!(matches!(Prestarted::start(&offline, || unreachable!()), Prestart::Offline));
            assert!(fs::read_to_string(&config.log_file).unwrap().contains(" OFFLINE at start, warming S1-mini\n"));
        }

        #[test]
        fn signed_in_reads_auth_status() {
            assert!(signed_in(&config("auth", "ok")));
            assert!(!signed_in(&config("auth-out", "signed-out")));
        }
    }
}
