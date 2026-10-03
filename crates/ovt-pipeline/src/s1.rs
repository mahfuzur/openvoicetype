//! `refine_s1`: S1-mini by Superwhisper in a local llama-server (`/v1/chat/completions`), with its own fixed system
//! prompt and the control line per mode. Starting the server is the caller's job (the app keeps it as a child process).

use crate::config::Config;
use crate::openai::{agent, answer, content};
use crate::text;
use serde_json::json;
use std::time::Duration;

/// `S1_SYSTEM_PROMPT`, from the model card.
pub const SYSTEM_PROMPT: &str = "You are a text normalizer for speech-to-text transcripts. The input begins with a control line specifying the styling, structure, and context settings; clean the transcript to match those settings and output only the cleaned text.";

/// The cleanup, or None (the server isn't answering, an error, an empty answer).
pub fn refine(config: &Config, raw: &str) -> Option<String> {
    // Output is about as long as the input; the cap stops a runaway generation.
    let max_tokens = text::word_count(raw) * 3 + 100;
    let user = format!("{}\n{raw}", text::s1_control_line(&config.mode));
    complete(config.s1_port, Duration::from_secs(config.s1_timeout), &user, max_tokens)
}

/// The request itself: S1-mini isn't instruction-following, so only its system prompt and the control line steer it.
fn complete(port: u16, timeout: Duration, user: &str, max_tokens: usize) -> Option<String> {
    let request = json!({
        "messages": [{"role": "system", "content": SYSTEM_PROMPT}, {"role": "user", "content": user}],
        "temperature": 0,
        "max_tokens": max_tokens,
    });
    let response = agent(timeout, true)
        .post(&format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .header("Content-Type", "application/json")
        .send(request.to_string());
    // curl --fail: an HTTP error is a failure.
    let (_, body) = answer(response).ok().filter(|(status, _)| *status < 400)?;
    Some(content(&body)).filter(|out| !out.trim().is_empty())
}

/// `srv_healthy` for llama-server on `S1_PORT`.
pub fn healthy(config: &Config) -> bool {
    healthy_on(config.s1_port)
}

/// `srv_healthy`: `/health` answers `"ok"` within 1 s (llama-server answers 503 while it loads the model).
pub(crate) fn healthy_on(port: u16) -> bool {
    let response = agent(Duration::from_secs(1), true).get(&format!("http://127.0.0.1:{port}/health")).call();
    answer(response).is_ok_and(|(_, body)| body.contains("\"ok\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::tests::{reply, serve};

    #[test]
    fn sends_the_control_line_and_reads_the_answer() {
        let (port, server) = serve(vec![
            reply(200, r#"{"choices":[{"message":{"content":"<think></think>Hello there.\n"}}]}"#),
            reply(500, "{}"),
            reply(200, r#"{"choices":[{"message":{"content":""}}]}"#),
        ]);
        let user = "[Styling: semi-formal] [Structure: lists] [Context: general]\nhello there";
        assert_eq!(complete(port, Duration::from_secs(5), user, 106), Some("Hello there.".into()));
        assert_eq!(complete(port, Duration::from_secs(5), user, 106), None);
        assert_eq!(complete(port, Duration::from_secs(5), user, 106), None);
        let request = &server.join().unwrap()[0];
        assert!(request.starts_with("POST /v1/chat/completions "));
        assert!(request.contains(r#""max_tokens":106"#) && request.contains(r#""temperature":0"#));
        assert!(request.contains(
            r#"{"content":"[Styling: semi-formal] [Structure: lists] [Context: general]\nhello there","role":"user"}"#
        ));
    }

    #[test]
    fn health() {
        let (port, server) =
            serve(vec![reply(503, r#"{"error":{"message":"Loading model"}}"#), reply(200, r#"{"status":"ok"}"#)]);
        assert!(!healthy_on(port));
        assert!(healthy_on(port));
        server.join().unwrap();
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert!(!healthy_on(closed));
    }
}
