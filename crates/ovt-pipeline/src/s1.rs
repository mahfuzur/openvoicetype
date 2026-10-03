//! `refine_s1`: S1-mini by Superwhisper in a local llama-server (`/v1/chat/completions`), with its own fixed system
//! prompt and the control line per mode. Starting the server is the caller's job (the app keeps it as a child process).

use crate::config::Config;

/// `S1_SYSTEM_PROMPT`, from the model card.
pub const SYSTEM_PROMPT: &str = "You are a text normalizer for speech-to-text transcripts. The input begins with a control line specifying the styling, structure, and context settings; clean the transcript to match those settings and output only the cleaned text.";

/// The cleanup, or None (the server isn't answering, an error, an empty answer).
pub fn refine(config: &Config, raw: &str) -> Option<String> {
    todo!("refine_s1")
}

/// `srv_healthy` for llama-server on `S1_PORT`.
pub fn healthy(config: &Config) -> bool {
    todo!("srv_healthy s1-server")
}
