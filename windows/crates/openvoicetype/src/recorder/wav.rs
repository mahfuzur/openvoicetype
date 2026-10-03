//! The audio arithmetic of `Recorder.swift` (which lets `AVAudioConverter` do it): the device's interleaved samples →
//! mono, 16 kHz, 16-bit PCM in a WAV file, and the 0–1 level the overlay's waveform shows. Pure, so it's tested on
//! every platform.

use std::io;
use std::path::Path;

/// What Whisper wants.
pub const TARGET_RATE: u32 = 16_000;

/// Averages each frame's channels into `out`.
pub fn downmix(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    let channels = channels.max(1);
    out.extend(interleaved.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32));
}

/// The level of one buffer, like `Recorder.swift`: RMS in decibels, −55 dB → 0 and −10 dB → 1.
pub fn level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let decibels = 20.0 * rms.max(1e-7).log10();
    ((decibels + 55.0) / 45.0).clamp(0.0, 1.0)
}

/// Converts the sample rate. Downsampling averages the input samples each output sample covers (a box filter, enough
/// against aliasing for speech at 44.1/48 kHz → 16 kHz); upsampling interpolates linearly.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() || from == 0 || to == 0 {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let count = ((input.len() as f64) / ratio).floor() as usize;
    if ratio > 1.0 {
        (0..count)
            .map(|i| {
                let start = (i as f64 * ratio) as usize;
                let end = (((i + 1) as f64 * ratio) as usize).clamp(start + 1, input.len());
                input[start..end].iter().sum::<f32>() / (end - start) as f32
            })
            .collect()
    } else {
        (0..count)
            .map(|i| {
                let position = i as f64 * ratio;
                let index = position as usize;
                let fraction = (position - index as f64) as f32;
                let next = input.get(index + 1).copied().unwrap_or(input[index]);
                input[index] + (next - input[index]) * fraction
            })
            .collect()
    }
}

pub fn to_i16(samples: &[f32]) -> Vec<i16> {
    samples.iter().map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16).collect()
}

/// A canonical 44-byte-header PCM WAV: mono, 16-bit.
pub fn wav_bytes(samples: &[i16], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // channels
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Mono samples at `rate` → a 16 kHz WAV at `path` (the recording dictate.sh's `transcribe` used to get).
pub fn write_wav(path: &Path, mono: &[f32], rate: u32) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, wav_bytes(&to_i16(&resample(mono, rate, TARGET_RATE)), TARGET_RATE))
}

/// The format of a WAV this module wrote: (sample rate, channels, bits per sample, data bytes). For the self-test.
pub fn read_header(bytes: &[u8]) -> Option<(u32, u16, u16, u32)> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..16] != b"WAVEfmt " || &bytes[36..40] != b"data" {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    Some((u32_at(24), u16_at(22), u16_at(34), u32_at(40)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmixes_frames() {
        let mut out = Vec::new();
        downmix(&[0.5, -0.5, 1.0, 0.0, 0.25], 2, &mut out); // the incomplete last frame is dropped
        assert_eq!(out, vec![0.0, 0.5]);
    }

    #[test]
    fn resamples() {
        let input: Vec<f32> = (0..4800).map(|i| (i % 3) as f32).collect();
        let down = resample(&input, 48_000, 16_000);
        assert_eq!(down.len(), 1600);
        assert!(down.iter().all(|s| (s - 1.0).abs() < 1e-6)); // averaging 0, 1, 2
        let up = resample(&[0.0, 1.0], 8_000, 16_000);
        assert_eq!(up, vec![0.0, 0.5, 1.0, 1.0]);
        assert_eq!(resample(&[0.3], 16_000, 16_000), vec![0.3]);
        assert_eq!(resample(&input, 44_100, 16_000).len(), 1741);
    }

    #[test]
    fn levels() {
        assert_eq!(level(&[]), 0.0);
        assert_eq!(level(&[0.0; 64]), 0.0);
        assert!((level(&[1.0; 64]) - 1.0).abs() < 1e-6);
        let quiet = level(&[0.01; 64]); // −40 dB
        assert!((quiet - 15.0 / 45.0).abs() < 1e-3);
    }

    #[test]
    fn wav_round_trip() {
        let dir = std::env::temp_dir().join(format!("ovt-wav-{}", std::process::id()));
        let path = dir.join("test.wav");
        let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 / 10.0).sin() * 0.5).collect();
        write_wav(&path, &tone, 48_000).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(read_header(&bytes), Some((16_000, 1, 16, 32_000)));
        assert_eq!(bytes.len(), 44 + 32_000);
        assert_eq!(to_i16(&[1.5, -1.0, 0.0]), vec![i16::MAX, -i16::MAX, 0]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
