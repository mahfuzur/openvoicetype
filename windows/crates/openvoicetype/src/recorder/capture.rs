//! WASAPI capture through cpal (follows `Recorder.swift`'s `AVAudioEngine` tap): the default input or the device named
//! in the settings, its native format downmixed to mono as it arrives, a level per buffer for the overlay, and the
//! signals the dictation's no-audio watchdog reads. The WAV is written (resampled to 16 kHz) when recording stops.
//!
//! A cpal stream can't move between threads, so a `Recorder` lives on the dictation thread.
//! TODO: the Mac's ~2 s start retry for Bluetooth headsets switching to their headset profile.

use super::wav;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What the audio callback shares with the dictation thread.
struct Shared {
    mono: Mutex<Vec<f32>>,
    buffers: AtomicU64,
    /// Milliseconds after `started` of the last buffer.
    last_buffer_ms: AtomicU64,
    started: Instant,
    error: Mutex<Option<String>>,
    /// The overlay's level: the loudest buffer since the overlay last took it (f32 bits; for non-negative floats the
    /// bit patterns order like the values, so `fetch_max` works).
    level: Arc<AtomicU32>,
}

pub struct Recorder {
    stream: cpal::Stream,
    shared: Arc<Shared>,
    pub device_name: String,
    rate: u32,
}

impl Recorder {
    /// Starts capturing from `device` (a name or cpal id from the settings; None or not found: the default input).
    pub fn start(device: Option<&str>, level: Arc<AtomicU32>) -> Result<Recorder, String> {
        let host = cpal::default_host();
        let device = pick_device(&host, device).ok_or("No microphone found")?;
        let device_name = device.to_string();
        let supported = device.default_input_config().map_err(|e| format!("{device_name} can't record: {e}"))?;
        let config = supported.config();
        let shared = Arc::new(Shared {
            mono: Mutex::new(Vec::with_capacity(config.sample_rate as usize * 60)),
            buffers: AtomicU64::new(0),
            last_buffer_ms: AtomicU64::new(0),
            started: Instant::now(),
            error: Mutex::new(None),
            level,
        });
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, &config, &shared),
            SampleFormat::I16 => build::<i16>(&device, &config, &shared),
            SampleFormat::I32 => build::<i32>(&device, &config, &shared),
            SampleFormat::U16 => build::<u16>(&device, &config, &shared),
            SampleFormat::U8 => build::<u8>(&device, &config, &shared),
            SampleFormat::I8 => build::<i8>(&device, &config, &shared),
            SampleFormat::F64 => build::<f64>(&device, &config, &shared),
            other => return Err(format!("{device_name}: unsupported sample format {other}")),
        }
        .map_err(|e| format!("{device_name} can't record: {e}"))?;
        stream.play().map_err(|e| format!("{device_name} can't start: {e}"))?;
        crate::applog::write(&format!(
            "MIC start device=\"{device_name}\" rate={} channels={}",
            config.sample_rate, config.channels
        ));
        Ok(Recorder { stream, shared, device_name, rate: config.sample_rate })
    }

    /// The first buffer arrived: the moment to start speaking.
    pub fn has_audio(&self) -> bool {
        self.shared.buffers.load(Ordering::SeqCst) > 0
    }

    /// Time since the last buffer (or since the start, before the first).
    pub fn silence(&self) -> Duration {
        let last = Duration::from_millis(self.shared.last_buffer_ms.load(Ordering::SeqCst));
        self.shared.started.elapsed().saturating_sub(last)
    }

    /// The stream reported an error (the device was unplugged…).
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().ok()?.clone()
    }

    /// Stops and writes the 16 kHz WAV; returns the seconds recorded.
    pub fn finish(self, path: &Path) -> std::io::Result<f64> {
        let _ = self.stream.pause();
        drop(self.stream);
        let mono = std::mem::take(&mut *self.shared.mono.lock().unwrap_or_else(|e| e.into_inner()));
        wav::write_wav(path, &mono, self.rate)?;
        Ok(mono.len() as f64 / self.rate.max(1) as f64)
    }
}

fn pick_device(host: &cpal::Host, wanted: Option<&str>) -> Option<cpal::Device> {
    if let Some(wanted) = wanted.filter(|w| !w.is_empty()) {
        let matches = |d: &cpal::Device| d.to_string() == wanted || d.id().is_ok_and(|id| id.to_string() == wanted);
        if let Some(device) = host.input_devices().ok().and_then(|mut all| all.find(matches)) {
            return Some(device);
        }
        crate::applog::write(&format!("MIC \"{wanted}\" not found, using the default input"));
    }
    host.default_input_device()
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: &Arc<Shared>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let data_shared = Arc::clone(shared);
    let error_shared = Arc::clone(shared);
    let (mut samples, mut frame) = (Vec::new(), Vec::new());
    device.build_input_stream::<T, _, _>(
        *config,
        move |data: &[T], _| {
            samples.clear();
            samples.extend(data.iter().map(|s| s.to_sample::<f32>()));
            frame.clear();
            wav::downmix(&samples, channels, &mut frame);
            let level = wav::level(&frame);
            data_shared.level.fetch_max(level.to_bits(), Ordering::Relaxed);
            if let Ok(mut mono) = data_shared.mono.lock() {
                mono.extend_from_slice(&frame);
            }
            let elapsed = data_shared.started.elapsed().as_millis() as u64;
            data_shared.last_buffer_ms.store(elapsed, Ordering::SeqCst);
            data_shared.buffers.fetch_add(1, Ordering::SeqCst);
        },
        move |error| {
            crate::applog::write(&format!("MIC error {error}"));
            if let Ok(mut slot) = error_shared.error.lock() {
                slot.get_or_insert(error.to_string());
            }
        },
        None,
    )
}
