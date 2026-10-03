//! `refine_openai`: any OpenAI-compatible `/chat/completions` endpoint, with the same system prompt and user message as
//! Claude. The key goes in a header, never in a file or a process argument.

use crate::claude::EngineError;
use crate::config::Config;

/// `openai_endpoint`: (host, port) of the base URL, for the online check.
pub fn endpoint(base_url: &str) -> (String, u16) {
    todo!("openai_endpoint")
}

/// `openai_is_local`.
pub fn is_local(base_url: &str) -> bool {
    todo!("openai_is_local")
}

pub fn refine(config: &Config, system_prompt: &str, user_message: &str) -> Result<String, EngineError> {
    todo!("refine_openai")
}
