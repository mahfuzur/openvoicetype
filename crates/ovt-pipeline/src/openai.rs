//! `refine_openai`: any OpenAI-compatible `/chat/completions` endpoint, with the same system prompt and user message as
//! Claude. The key goes in a header, never in a file or a process argument.

use crate::claude::{chomp, perl_string, EngineError};
use crate::config::{Config, Job};
use crate::refine::log;
use serde_json::{json, Value};
use std::time::Duration;

/// `openai_endpoint`: (host, port) of the base URL, for the online check.
pub fn endpoint(base_url: &str) -> (String, u16) {
    let (scheme, rest) = base_url.split_once("://").unwrap_or((base_url, base_url));
    let rest = rest.split('/').next().unwrap_or_default();
    let rest = rest.rsplit('@').next().unwrap_or_default();
    let default = if scheme == "http" { 80 } else { 443 };
    match rest.rsplit_once(':') {
        Some((host, port)) if !rest.starts_with('[') => (host.to_string(), port.parse().unwrap_or(default)),
        _ => (rest.to_string(), default),
    }
}

/// `openai_is_local`.
pub fn is_local(base_url: &str) -> bool {
    let (host, _) = endpoint(base_url);
    host == "localhost" || host == "::1" || host.starts_with("127.") || host.starts_with("[::1]")
}

/// An HTTP client like the script's curl calls: one overall time limit, any status is an answer, no redirects. `local`
/// servers (whisper-server, llama-server) never go through a proxy.
pub(crate) fn agent(timeout: Duration, local: bool) -> ureq::Agent {
    let mut config =
        ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).max_redirects(0);
    if local {
        config = config.proxy(None);
    }
    ureq::Agent::new_with_config(config.build())
}

/// An HTTP status and body, or why there's none ("timeout" or "error").
pub(crate) fn answer(
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<(u16, String), EngineError> {
    let read = response.and_then(|mut response| {
        let status = response.status().as_u16();
        response.body_mut().read_to_vec().map(|body| (status, String::from_utf8_lossy(&body).into_owned()))
    });
    match read {
        Ok(answer) => Ok(answer),
        Err(ureq::Error::Timeout(_)) => Err(EngineError::new("timeout", "")),
        Err(ureq::Error::Io(e)) if e.kind() == std::io::ErrorKind::TimedOut => Err(EngineError::new("timeout", "")),
        Err(e) => Err(EngineError::new("error", &format!("request failed: {e}"))),
    }
}

/// `choices[0].message.content` without a `<think>…</think>` block (the first one, as the script's `s#…#…#s`), with the
/// trailing newlines the shell drops.
pub(crate) fn content(body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap_or_default();
    let content = match &value["choices"][0]["message"]["content"] {
        Value::String(text) => text.clone(),
        other @ (Value::Number(_) | Value::Bool(_)) => perl_string(other).unwrap_or_default(),
        _ => String::new(),
    };
    chomp(&strip_think(&content))
}

fn strip_think(text: &str) -> String {
    if let Some(start) = text.find("<think>") {
        if let Some(end) = text[start + 7..].find("</think>") {
            return format!("{}{}", &text[..start], &text[start + 7 + end + 8..]);
        }
    }
    text.to_string()
}

/// `OPENAI_TIMEOUT`, which `cmd_command` sets to `COMMAND_TIMEOUT`.
fn timeout(config: &Config) -> Duration {
    Duration::from_secs(if config.job == Job::Command { config.command_timeout } else { config.openai_timeout })
}

/// `refine_openai`: some models (OpenAI's reasoning ones) only take the default temperature, so a 400 that mentions it
/// is retried once without it.
pub fn refine(config: &Config, system_prompt: &str, user_message: &str) -> Result<String, EngineError> {
    if config.openai_base_url.is_empty() || config.openai_model.is_empty() {
        return Err(EngineError::new("config", "base URL or model not set"));
    }
    let base = config.openai_base_url.strip_suffix('/').unwrap_or(&config.openai_base_url);
    let url = format!("{base}/chat/completions");
    let agent = agent(timeout(config), false);
    let mut temperature = true;
    let (code, body) = loop {
        let mut request = json!({
            "model": config.openai_model,
            "messages": [{"role": "system", "content": system_prompt}, {"role": "user", "content": user_message}],
        });
        if temperature {
            request["temperature"] = json!(0);
        }
        let mut call = agent.post(&url).header("Content-Type", "application/json");
        if !config.openai_api_key.is_empty() {
            call = call.header("Authorization", &format!("Bearer {}", config.openai_api_key));
        }
        let (code, body) = answer(call.send(request.to_string()))?;
        if code == 400 && temperature && body.contains("temperature") {
            temperature = false;
            continue;
        }
        break (code, body);
    };
    match code {
        200 => {}
        401 | 403 => return Err(EngineError::new("auth", &format!("HTTP {code}"))),
        429 => return Err(EngineError::new("limit", "HTTP 429")),
        _ => {
            // Only the server's error message: some proxies echo the request (the transcript) in the body.
            let error = serde_json::from_str::<Value>(&body).map(|v| v["error"].clone()).unwrap_or_default();
            let text = match &error {
                Value::Object(_) => perl_string(&error["message"]).unwrap_or_default(),
                Value::Array(_) => String::new(),
                other => perl_string(other).unwrap_or_default(),
            };
            let message: String = squeeze_whitespace(&text).chars().take(160).collect();
            let suffix = if message.is_empty() { String::new() } else { format!(": {message}") };
            log(config, &format!("WARN openai HTTP {code}{suffix}"));
            return Err(EngineError::new("error", &format!("HTTP {code}")));
        }
    }
    let out = content(&body);
    if out.trim().is_empty() {
        return Err(EngineError::new("error", "empty answer"));
    }
    Ok(out)
}

/// `s/\s+/ /g`.
fn squeeze_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// One canned HTTP answer.
    pub(crate) struct Reply {
        pub status: u16,
        pub body: String,
        pub delay: Duration,
    }

    pub(crate) fn reply(status: u16, body: &str) -> Reply {
        Reply { status, body: body.into(), delay: Duration::ZERO }
    }

    /// A tiny HTTP server: answers one connection per reply, in order, and returns the requests it got.
    pub(crate) fn serve(replies: Vec<Reply>) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for reply in replies {
                let Ok((mut stream, _)) = listener.accept() else { break };
                let mut data = Vec::new();
                let mut buffer = [0u8; 65536];
                loop {
                    let n = stream.read(&mut buffer).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buffer[..n]);
                    let text = String::from_utf8_lossy(&data);
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_string())
                            })
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(0);
                        if data.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8_lossy(&data).into_owned());
                std::thread::sleep(reply.delay);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.status,
                    reply.body.len(),
                    reply.body
                );
            }
            requests
        });
        (port, handle)
    }

    pub(crate) fn test_config(name: &str) -> Config {
        let dir = crate::claude::tests::test_dir(name);
        Config { log_file: dir.join("dictate.log"), state_dir: dir.join("state"), ..Config::default() }
    }

    fn config(name: &str, port: u16) -> Config {
        Config {
            openai_base_url: format!("http://127.0.0.1:{port}/v1/"),
            openai_model: "test-model".into(),
            openai_api_key: "sk-test".into(),
            ..test_config(name)
        }
    }

    #[test]
    fn endpoints() {
        assert_eq!(endpoint("http://localhost:11434/v1"), ("localhost".into(), 11434));
        assert_eq!(endpoint("https://api.openai.com/v1"), ("api.openai.com".into(), 443));
        assert_eq!(endpoint("http://user:pw@example.com/v1"), ("example.com".into(), 80));
        assert_eq!(endpoint("http://[::1]:8080/v1"), ("[::1]:8080".into(), 80));
        assert!(
            is_local("http://localhost:11434/v1") && is_local("http://127.0.0.1:1234") && is_local("http://[::1]:8080")
        );
        assert!(!is_local("https://api.groq.com/openai/v1"));
    }

    #[test]
    fn retries_without_temperature_and_strips_think() {
        let ok = r#"{"choices":[{"message":{"content":"<think>hmm</think>Hello there.\n"}}]}"#;
        let (port, server) =
            serve(vec![reply(400, r#"{"error":{"message":"Unsupported value: 'temperature'"}}"#), reply(200, ok)]);
        assert_eq!(refine(&config("openai-ok", port), "SYSTEM", "USER"), Ok("Hello there.".into()));
        let requests = server.join().unwrap();
        assert!(requests[0].contains("\"temperature\":0") && !requests[1].contains("temperature"));
        assert!(requests[0].starts_with("POST /v1/chat/completions "));
        assert!(requests[0].to_ascii_lowercase().contains("authorization: bearer sk-test"));
        assert!(requests[1].contains(r#"{"content":"SYSTEM","role":"system"}"#), "{}", requests[1]);
    }

    #[test]
    fn classifies_failures() {
        let (port, server) = serve(vec![
            reply(401, "{}"),
            reply(429, "{}"),
            reply(500, r#"{"error":{"message":"bad\n  things"}}"#),
            reply(200, r#"{"choices":[{"message":{"content":"  "}}]}"#),
        ]);
        let config = config("openai-fail", port);
        assert_eq!(refine(&config, "S", "U"), Err(EngineError::new("auth", "HTTP 401")));
        assert_eq!(refine(&config, "S", "U"), Err(EngineError::new("limit", "HTTP 429")));
        assert_eq!(refine(&config, "S", "U"), Err(EngineError::new("error", "HTTP 500")));
        assert_eq!(refine(&config, "S", "U"), Err(EngineError::new("error", "empty answer")));
        server.join().unwrap();
        let log = std::fs::read_to_string(&config.log_file).unwrap();
        assert!(log.ends_with(" WARN openai HTTP 500: bad things\n"), "{log}");
        let unset = Config { openai_model: String::new(), ..config };
        assert_eq!(refine(&unset, "S", "U"), Err(EngineError::new("config", "base URL or model not set")));
    }

    #[test]
    fn times_out() {
        let (port, _server) = serve(vec![Reply { status: 200, body: "{}".into(), delay: Duration::from_secs(3) }]);
        let config = Config { openai_timeout: 1, ..config("openai-timeout", port) };
        let error = refine(&config, "S", "U").unwrap_err();
        assert_eq!(error.kind, "timeout", "{error:?}");
    }
}
