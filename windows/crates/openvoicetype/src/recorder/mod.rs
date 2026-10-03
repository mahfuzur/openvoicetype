//! The microphone (follows `Recorder.swift`): capture through WASAPI (cpal), the live level, and a 16 kHz mono WAV.

pub mod wav;

#[cfg(windows)]
mod capture;
#[cfg(windows)]
pub use capture::Recorder;
