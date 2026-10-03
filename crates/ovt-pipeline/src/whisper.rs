//! `transcribe` / `transcribe_wav`: Whisper through the warm whisper-server (`/inference`), whisper-cli as the
//! fallback; `wav_seconds` from the header. Starting and stopping the server is the app's job.

use crate::config::Config;

/// `wav_seconds`: the length from a 16-bit PCM WAV header, or None.
pub fn wav_seconds(path: &std::path::Path) -> Option<f64> {
    todo!("wav_seconds")
}

pub struct Transcript {
    /// Empty: no speech (too short, silence, a hallucination).
    pub text: String,
    pub seconds: f64,
    pub whisper_ms: u64,
}

/// `transcribe_wav`: the warm whisper-server on `port` (the caller starts it), else `whisper_cli` with `model`.
/// Logged like `cmd_transcribe`.
pub fn transcribe(
    config: &Config,
    wav: &std::path::Path,
    port: u16,
    whisper_cli: &std::path::Path,
    model: &std::path::Path,
) -> std::io::Result<Transcript> {
    todo!("transcribe_wav")
}
