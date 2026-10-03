//! The Claude CLI (`claude_options`, `claude_command`, `claude_prestart`, `claude_send`, `refine`'s one-shot call,
//! `claude_parse`, `claude_plain`). The same flags, environment and failure classes as `dictate.sh`; the pre-start uses
//! an ordinary pipe as `claude.exe`'s stdin (the script's fifo doesn't work for a native Windows program).

use crate::config::Config;

/// Why the online engine failed: `engine_error`'s kind (limit, auth, auth-mismatch, offline, timeout, config, error),
/// the raw reset time (formatted later) and a short detail for the log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineError {
    pub kind: String,
    pub resets: String,
    pub detail: String,
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
pub fn parse(events: &str, log_file: Option<&std::path::Path>) -> Parsed {
    todo!("claude_parse")
}

/// A `claude -p` started before the transcript exists (`claude_prestart`): stream-json in and out, stdin held open.
pub struct Prestarted {
    // private
}

impl Prestarted {
    /// Starts it, or None when pre-starting doesn't apply (off, raw mode, no claude, offline: then `offline` is set
    /// so cleanup skips Claude, as `PRESTART_OFFLINE` does).
    pub fn start(config: &Config) -> Option<Self> {
        todo!("claude_prestart")
    }

    /// `claude_send`: one user message, then the wait with the script's early exits. Ok(answer), Err(Failed) or
    /// Err(Unparsed) (then the caller retries with a one-shot call).
    pub fn send(self, config: &Config, user_message: &str) -> Result<String, Parsed> {
        todo!("claude_send")
    }
}

/// `refine`'s one-shot `claude -p --output-format json`, with `CLAUDE_TIMEOUT`.
pub fn one_shot(config: &Config, user_message: &str) -> Result<String, EngineError> {
    todo!("refine (one-shot)")
}

/// `claude_plain auth status --json` says signed in (the `auth-mismatch` canary).
pub fn signed_in(config: &Config) -> bool {
    todo!("claude_plain auth status")
}
