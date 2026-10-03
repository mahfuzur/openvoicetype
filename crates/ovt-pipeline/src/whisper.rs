//! `transcribe` / `transcribe_wav`: Whisper through the warm whisper-server (`/inference`), whisper-cli as the
//! fallback; `wav_seconds` from the header. Starting and stopping the server is the app's job.

use crate::claude::{command, create_private, error_log, run};
use crate::config::Config;
use crate::openai::agent;
use crate::refine::log;
use crate::text::{self, Dictionary};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// `MIN_SECONDS`: shorter recordings are skipped.
pub const MIN_SECONDS: f64 = 0.5;
/// `WHISPER_SERVER_TIMEOUT`.
const SERVER_TIMEOUT: Duration = Duration::from_secs(60);

/// `wav_seconds`: the length from a 16-bit PCM WAV header, or None.
///
/// The data chunk's length over the format's byte rate, rounded to milliseconds as the script prints it ("%.3f"). A
/// data length of 0 or 0xFFFFFFFF (a recorder that never finished the header), or past the end, means "to the end".
pub fn wav_seconds(path: &std::path::Path) -> Option<f64> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let (mut at, mut rate) = (12usize, 0u32);
    let u32_at = |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    while at + 8 <= data.len() {
        let (id, length) = (&data[at..at + 4], u32_at(at + 4) as usize);
        at += 8;
        let left = data.len() - at;
        if id == b"data" {
            let length = if length == 0 || length == 0xFFFF_FFFF || length > left { left } else { length };
            if rate == 0 {
                return None;
            }
            return format!("{:.3}", length as f64 / rate as f64).parse().ok();
        }
        // perl's read: 0 bytes (an empty chunk, or the end of the file) fails.
        let read = (length + length % 2).min(left);
        if read == 0 {
            return None;
        }
        if id == b"fmt " {
            if read < 8 {
                return None;
            }
            rate = if read >= 12 { u32_at(at + 8) } else { 0 };
        }
        at += read;
    }
    None
}

pub struct Transcript {
    /// Empty: no speech (too short, silence, a hallucination).
    pub text: String,
    pub seconds: f64,
    pub whisper_ms: u64,
}

/// `transcribe_wav`: the warm whisper-server on `port` (the caller starts it), else `whisper_cli` with `model`.
/// Logged like `cmd_transcribe`. `language` is `LANGUAGE` ("en"; whisper-server gets its own at launch).
pub fn transcribe(
    config: &Config,
    wav: &std::path::Path,
    port: u16,
    whisper_cli: &std::path::Path,
    model: &std::path::Path,
    language: &str,
) -> std::io::Result<Transcript> {
    let nothing = |seconds| Ok(Transcript { text: String::new(), seconds, whisper_ms: 0 });
    if std::fs::metadata(wav).map(|m| m.len() == 0).unwrap_or(true) {
        log(config, "EMPTY no audio file");
        return nothing(0.0);
    }
    // An unreadable header counts as MIN_SECONDS, so the recording isn't dropped: Whisper decides.
    let (seconds, shown) = match wav_seconds(wav) {
        Some(seconds) => (seconds, format!("{seconds:.3}")),
        None => (MIN_SECONDS, "0.5".to_string()),
    };
    if seconds < MIN_SECONDS {
        log(config, &format!("SKIP audio too short ({shown}s)"));
        return nothing(seconds);
    }
    let t0 = Instant::now();
    if !model.is_file() {
        log(config, &format!("ERROR Whisper model not found: {}", model.display()));
        return Err(io::Error::new(io::ErrorKind::NotFound, format!("Whisper model not found: {}", model.display())));
    }
    let prompt = text::whisper_prompt(config, &Dictionary::load(&config.dictionary_file));
    let out = match inference(port, wav, language, &prompt) {
        Ok(out) => out,
        Err(reached) => {
            // A server that isn't running is the normal case for the fallback; one that failed is worth a line.
            if reached {
                log(config, "WARN whisper-server failed, using whisper-cli");
            }
            whisper_cli_run(config, whisper_cli, model, wav, language, &prompt)?
        }
    };
    let text = text::clean_transcript(&out);
    let whisper_ms = t0.elapsed().as_millis() as u64;
    if text.is_empty() {
        log(config, &format!("EMPTY no speech detected (audio={shown}s)"));
        return nothing(seconds);
    }
    log(config, &format!("TRANSCRIBE audio={shown}s whisper={whisper_ms}ms"));
    if config.log_text {
        log(config, &format!("  raw:     {text}"));
    }
    Ok(Transcript { text, seconds, whisper_ms })
}

/// `transcribe_server`'s request: the WAV and the same settings as whisper-cli, as multipart form data. Err(true) if
/// the server answered with an error or timed out, Err(false) if nothing listens on the port.
fn inference(port: u16, wav: &Path, language: &str, prompt: &str) -> Result<String, bool> {
    let audio = std::fs::read(wav).map_err(|_| false)?;
    let name = wav.file_name().map(|n| n.to_string_lossy().replace(['"', '\r', '\n'], "_")).unwrap_or_default();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let boundary = format!("------------------------ovt{nanos:x}");
    let mut body = Vec::with_capacity(audio.len() + 1024);
    let mut part = |headers: String, content: &[u8]| {
        body.extend_from_slice(format!("--{boundary}\r\n{headers}\r\n\r\n").as_bytes());
        body.extend_from_slice(content);
        body.extend_from_slice(b"\r\n");
    };
    part(format!("Content-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream"), &audio);
    for (field, value) in
        [("response_format", "text"), ("temperature", "0"), ("language", language), ("prompt", prompt)]
    {
        part(format!("Content-Disposition: form-data; name=\"{field}\""), value.as_bytes());
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let response = agent(SERVER_TIMEOUT, true)
        .post(&format!("http://127.0.0.1:{port}/inference"))
        .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .send(&body[..]);
    let refused = |e: &ureq::Error| match e {
        ureq::Error::ConnectionFailed => true,
        ureq::Error::Io(e) => e.kind() == io::ErrorKind::ConnectionRefused,
        _ => false,
    };
    match response {
        Err(e) if refused(&e) => Err(false),
        response => match crate::openai::answer(response) {
            Ok((status, text)) if status < 400 => Ok(text),
            _ => Err(true),
        },
    }
}

/// whisper-cli is chatty on stderr: kept only when it fails (appended to error.log).
fn whisper_cli_run(
    config: &Config,
    cli: &Path,
    model: &Path,
    wav: &Path,
    language: &str,
    prompt: &str,
) -> io::Result<String> {
    let mut cli_command = command(cli);
    cli_command.arg("-m").arg(model).arg("-f").arg(wav).args(["-nt", "-np", "-sns", "-l", language]);
    if !prompt.is_empty() {
        cli_command.args(["--prompt", prompt]);
    }
    std::fs::create_dir_all(&config.state_dir)?;
    let err_path = config.state_dir.join("whisper.err");
    cli_command.stderr(create_private(&err_path)?);
    let ran = run(&mut cli_command, None, None);
    match ran {
        Ok(ran) if ran.status.is_some_and(|s| s.success()) => Ok(String::from_utf8_lossy(&ran.stdout).into_owned()),
        failed => {
            let mut errors = std::fs::read(&err_path).unwrap_or_default();
            if let Err(e) = &failed {
                errors.extend_from_slice(format!("whisper-cli: {e}\n").as_bytes());
            }
            use std::io::Write;
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(error_log(config))
                .and_then(|mut f| f.write_all(&errors));
            log(config, &format!("ERROR whisper-cli failed (see {})", error_log(config).display()));
            Err(io::Error::other("whisper-cli failed"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::tests::{reply, serve, test_config};

    fn wav(dir: &Path, name: &str, byte_rate: u32, data_length: u32, data: usize, extra: &[u8]) -> std::path::PathBuf {
        let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
        bytes.extend_from_slice(extra);
        bytes.extend_from_slice(b"fmt \x10\0\0\0\x01\0\x01\0\x80\x3e\0\0");
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        bytes.extend_from_slice(b"\x02\0\x10\0data");
        bytes.extend_from_slice(&data_length.to_le_bytes());
        bytes.extend(std::iter::repeat_n(0u8, data));
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn reads_the_length_from_the_header() {
        let dir = crate::claude::tests::test_dir("wav");
        assert_eq!(wav_seconds(&wav(&dir, "a.wav", 32000, 32000, 32000, b"")), Some(1.0));
        // An unfinished header (0 or 0xFFFFFFFF) or a length past the end: to the end of the file.
        assert_eq!(wav_seconds(&wav(&dir, "b.wav", 32000, 0, 16000, b"")), Some(0.5));
        assert_eq!(wav_seconds(&wav(&dir, "c.wav", 32000, 0xFFFF_FFFF, 8000, b"")), Some(0.25));
        assert_eq!(wav_seconds(&wav(&dir, "d.wav", 32000, 64000, 3333, b"")), Some(0.104));
        // Another chunk first (odd length, padded).
        assert_eq!(wav_seconds(&wav(&dir, "e.wav", 32000, 32000, 32000, b"LIST\x03\0\0\0abc\0")), Some(1.0));
        assert_eq!(wav_seconds(&wav(&dir, "f.wav", 0, 32000, 32000, b"")), None);
        std::fs::write(dir.join("g.wav"), b"not a wav").unwrap();
        assert_eq!(wav_seconds(&dir.join("g.wav")), None);
        assert_eq!(wav_seconds(&dir.join("missing.wav")), None);
    }

    #[test]
    fn posts_the_wav_to_whisper_server() {
        let dir = crate::claude::tests::test_dir("inference");
        let path = wav(&dir, "rec.wav", 32000, 32000, 32000, b"");
        let (port, server) = serve(vec![reply(200, " Hello there.\n"), reply(500, "{}")]);
        assert_eq!(inference(port, &path, "en", "Names and terms: Kubernetes."), Ok(" Hello there.\n".into()));
        assert_eq!(inference(port, &path, "en", ""), Err(true));
        let request = &server.join().unwrap()[0];
        assert!(request.starts_with("POST /inference "));
        assert!(request.contains("name=\"file\"; filename=\"rec.wav\""));
        for (field, value) in [
            ("response_format", "text"),
            ("temperature", "0"),
            ("language", "en"),
            ("prompt", "Names and terms: Kubernetes."),
        ] {
            assert!(request.contains(&format!("name=\"{field}\"\r\n\r\n{value}\r\n")), "{field}");
        }
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert_eq!(inference(closed, &path, "en", ""), Err(false));
    }

    #[test]
    fn skips_short_or_missing_audio() {
        let config = test_config("transcribe-short");
        let dir = config.state_dir.parent().unwrap().to_path_buf();
        let none = Path::new("/nonexistent");
        let short = wav(&dir, "short.wav", 32000, 0, 9600, b"");
        let t = transcribe(&config, &short, 1, none, none, "en").unwrap();
        assert_eq!((t.text.as_str(), t.seconds), ("", 0.3));
        let t = transcribe(&config, &dir.join("missing.wav"), 1, none, none, "en").unwrap();
        assert_eq!(t.text, "");
        let error = transcribe(&config, &wav(&dir, "ok.wav", 32000, 0, 32000, b""), 1, none, none, "en");
        assert_eq!(error.err().map(|e| e.kind()), Some(io::ErrorKind::NotFound));
        let log = std::fs::read_to_string(&config.log_file).unwrap();
        let lines: Vec<&str> = log.lines().map(|l| &l[20..]).collect();
        assert_eq!(
            lines,
            ["SKIP audio too short (0.300s)", "EMPTY no audio file", "ERROR Whisper model not found: /nonexistent"]
        );
    }
}
