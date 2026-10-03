//! `transcribe` / `transcribe_wav`: Whisper through the warm whisper-server (`/inference`), whisper-cli as the
//! fallback; `wav_seconds` from the header. Starting and stopping the server is the app's job.

use crate::config::Config;

/// `wav_seconds`: the length from a 16-bit PCM WAV header, or None.
pub fn wav_seconds(path: &std::path::Path) -> Option<f64> {
    todo!("wav_seconds")
}
