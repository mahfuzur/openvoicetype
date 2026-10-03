//! `refine_text`, `try_online`, `try_s1`, `cmd_refine`, `cmd_command`, `write_result`: the cleanup dispatcher and its
//! report, the same statuses, exit codes and result-file JSON as the script.

use crate::config::Config;

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
        todo!("cmd_refine / cmd_command exit codes")
    }

    /// `write_result`'s JSON (canonical key order).
    pub fn result_json(&self) -> String {
        todo!("write_result")
    }
}
