//! The dictation thread (follows `Dictation.start`, `Dictation.stop` and `Dictation.prestart`): record, transcribe,
//! clean up, then hand the result to `deliver`. Pipeline calls run guarded, so a bug there ends one dictation with
//! Whisper's text or an error, never the app.

use super::deliver::{self, Delivery, Timing};
use super::servers::Slot;
use super::{Command, Finish, State, UiEvent};
use crate::paste::{apps, target};
use crate::pipeline;
use crate::recorder::Recorder;
use crate::sounds::Sound;
use crate::{applog, overlay::scene::Phase};
use ovt_core::mode::DictationMode;
use ovt_core::settings::Settings;
use ovt_pipeline::config::Config;
use ovt_pipeline::refine::{Report, Session};
use ovt_pipeline::servers::Server;
use ovt_pipeline::whisper;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// Recordings end on their own after this long (the Mac's `maxDuration`).
const MAX_DURATION: Duration = Duration::from_secs(300);
/// No buffer from the mic for this long: it never started, or it dropped out (`Recorder.noAudioTimeout`).
const NO_AUDIO: Duration = Duration::from_millis(2500);
/// "Connecting to …" only for mics slower than this (built-in ones start in ~50 ms).
const CONNECTING_AFTER: Duration = Duration::from_millis(300);
/// `WHISPER_IDLE_MINUTES` / `S1_IDLE_MINUTES`.
const SERVER_IDLE: Duration = Duration::from_secs(10 * 60);

/// Runs `f`, turning a panic (a pipeline bug, or a part not written yet) into None. The panic hook logs it.
pub(super) fn guarded<T>(f: impl FnOnce() -> T) -> Option<T> {
    std::panic::catch_unwind(AssertUnwindSafe(f)).ok()
}

/// How a recording ended.
enum Ended {
    Stop,
    Cancel(Option<String>),
    Failed(String),
}

/// What the start of a dictation fixed: where the text goes and how it's written.
struct Start {
    settings: Settings,
    target: target::Target,
    mode: DictationMode,
    config: Config,
}

pub(super) struct Worker {
    commands: Receiver<Command>,
    settings: Arc<Mutex<Settings>>,
    owner: isize,
    level: Arc<AtomicU32>,
    notify: Box<dyn Fn(UiEvent) + Send>,
    state: Arc<AtomicU8>,
    whisper: Slot,
    s1: Slot,
    last_used: Instant,
    quit: bool,
}

impl Worker {
    pub(super) fn new(
        commands: Receiver<Command>,
        settings: Arc<Mutex<Settings>>,
        owner: HWND,
        level: Arc<AtomicU32>,
        notify: Box<dyn Fn(UiEvent) + Send>,
        state: Arc<AtomicU8>,
    ) -> Worker {
        Worker {
            commands,
            settings,
            owner: owner.0 as isize,
            level,
            notify,
            state,
            whisper: Slot::default(),
            s1: Slot::default(),
            last_used: Instant::now(),
            quit: false,
        }
    }

    pub(super) fn run(mut self) {
        // SAFETY: UI Automation (the password check) needs COM on this thread.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        while !self.quit {
            match self.commands.recv_timeout(Duration::from_secs(60)) {
                Ok(Command::Start) => self.dictate(),
                Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => self.stop_idle_servers(),
            }
        }
        self.whisper.stop();
        self.s1.stop();
    }

    fn set_state(&self, state: State) {
        self.state.store(state as u8, Ordering::SeqCst);
        (self.notify)(UiEvent::State(state));
    }

    fn finish(&self, finish: Finish) {
        self.set_state(State::Idle);
        (self.notify)(UiEvent::Finished(finish));
    }

    fn dictate(&mut self) {
        let Some(start) = self.begin() else { return };
        self.set_state(State::Starting);
        let helpers = pipeline::helpers_dir();
        self.whisper.prepare(pipeline::whisper_spec(&start.settings, &helpers));
        if start.settings.uses_s1() {
            self.s1.prepare(pipeline::s1_spec(&start.config, &helpers));
        }
        let s1_started: Arc<Mutex<Option<Server>>> = Arc::default();
        let session = {
            let config = start.config.clone();
            let starter = s1_starter(&start.config, &helpers, Arc::clone(&s1_started));
            std::thread::spawn(move || {
                guarded(|| {
                    let mut session = Session::start(config);
                    session.set_s1_starter(starter);
                    session
                })
            })
        };
        let finish = match Recorder::start(start.settings.input_device.as_deref(), Arc::clone(&self.level)) {
            Ok(recorder) => self.record(recorder, &start, session),
            Err(message) => {
                applog::write(&format!("RESULT failed: {message}"));
                drop_session(session);
                Finish::message(message, true, 2.5, Some(Sound::Error))
            }
        };
        if let Some(server) = s1_started.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.s1.adopt(server);
        }
        self.last_used = Instant::now();
        self.finish(finish);
    }

    /// Captures the target and decides the mode; None (and the dictation is refused) in a password box.
    fn begin(&mut self) -> Option<Start> {
        let settings = self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let target = target::capture();
        if target.target.is_secure {
            applog::write("RESULT refused: Not in password fields");
            self.finish(Finish::message("Not in password fields", false, 1.8, Some(Sound::Notice)));
            return None;
        }
        let mode = settings.mode_override.unwrap_or_else(|| apps::mode_for(&target.app_id, &settings.app_modes));
        let api_key = settings.uses_api().then(|| pipeline::stored_api_key(&settings.openai_base_url)).flatten();
        let config = pipeline::config(Config::from_env(), &settings, mode, &target.target.app_name, api_key);
        Some(Start { settings, target, mode, config })
    }

    /// Records until stopped, then transcribes, cleans up and delivers. Returns how the dictation ends.
    fn record(&mut self, recorder: Recorder, start: &Start, session: JoinHandle<Option<Session>>) -> Finish {
        let ended = self.wait_for_stop(&recorder);
        let stopped_at = Instant::now();
        let wav = ovt_core::paths::state_dir().join("app-recording.wav");
        let written = recorder.finish(&wav);
        let message = match ended {
            Ended::Stop if written.is_ok() => None,
            Ended::Stop => Some(Finish::message("Recording failed", true, 2.5, Some(Sound::Error))),
            Ended::Cancel(hint) => Some(cancelled(hint)),
            Ended::Failed(message) => Some(Finish::message(message, true, 2.5, Some(Sound::Error))),
        };
        if let Some(message) = message {
            let _ = std::fs::remove_file(&wav);
            drop_session(session);
            if let Finish { phase: Phase::Message { text, error: true }, .. } = &message {
                applog::write(&format!("RESULT failed: {text}"));
            }
            return message;
        }
        self.set_state(State::Transcribing);
        self.whisper.ready();
        let transcript = self.transcribe(start, &wav);
        let _ = std::fs::remove_file(&wav);
        let raw = match transcript {
            Some(text) if !text.trim().is_empty() => text.trim().to_string(),
            Some(_) => {
                drop_session(session);
                applog::write("RESULT no-speech");
                return Finish::message("No speech detected", false, 1.6, Some(Sound::Notice));
            }
            None => {
                drop_session(session);
                applog::write("RESULT failed: Transcription failed");
                return Finish::message("Transcription failed", true, 2.5, Some(Sound::Error));
            }
        };
        let mut timing =
            Timing { stopped_at, transcribe_ms: stopped_at.elapsed().as_millis() as u64, ..Timing::default() };
        let uses_cleanup = ovt_core::script::uses_cleanup(&start.settings, start.mode);
        if uses_cleanup {
            self.set_state(State::Polishing);
        }
        let cleanup_started = Instant::now();
        let (report, prestarted) = finish_session(session, raw.clone());
        timing.cleanup_ms = cleanup_started.elapsed().as_millis() as u64;
        timing.prestarted = prestarted;
        deliver::deliver(Delivery {
            owner: HWND(self.owner as *mut _),
            settings: &start.settings,
            target: &start.target,
            mode: start.mode,
            raw,
            report,
            uses_cleanup,
            timing,
        })
    }

    /// Waits for Stop or Cancel while watching the mic: the first buffer (now it's recording), no audio, an error,
    /// the time limit.
    fn wait_for_stop(&mut self, recorder: &Recorder) -> Ended {
        let started = Instant::now();
        let mut recording = false;
        let mut connecting_shown = false;
        loop {
            match self.commands.recv_timeout(Duration::from_millis(50)) {
                Ok(Command::Stop) if recording => return Ended::Stop,
                // Released before the mic was ready: nothing useful was recorded.
                Ok(Command::Stop) => return Ended::Cancel(None),
                Ok(Command::Cancel(hint)) => return Ended::Cancel(hint),
                Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => {
                    self.quit = true;
                    return Ended::Cancel(None);
                }
                Ok(Command::Start) | Err(RecvTimeoutError::Timeout) => {}
            }
            if let Some(error) = recorder.error() {
                // The mic dropped out mid-dictation: transcribe what was captured so far.
                return if recording {
                    Ended::Stop
                } else {
                    Ended::Failed(format!("{}: {error}", recorder.device_name))
                };
            }
            if !recording {
                if recorder.has_audio() {
                    recording = true;
                    self.set_state(State::Recording);
                } else if started.elapsed() > NO_AUDIO {
                    return Ended::Failed(format!("No audio from {}", recorder.device_name));
                } else if !connecting_shown && started.elapsed() > CONNECTING_AFTER {
                    connecting_shown = true;
                    (self.notify)(UiEvent::Connecting(recorder.device_name.clone()));
                }
            } else if recorder.silence() > NO_AUDIO {
                applog::write(&format!("MIC no audio from \"{}\" for 2.5 s, stopped", recorder.device_name));
                return Ended::Stop;
            } else if started.elapsed() > MAX_DURATION {
                return Ended::Stop;
            }
        }
    }

    /// Whisper's text (empty: no speech), or None if it failed.
    fn transcribe(&self, start: &Start, wav: &std::path::Path) -> Option<String> {
        let helpers = pipeline::helpers_dir();
        let model = pipeline::whisper_model(&start.settings);
        let cli = pipeline::helper(&helpers, "whisper-cli.exe");
        let result = guarded(|| whisper::transcribe(&start.config, wav, pipeline::WHISPER_PORT, &cli, &model))
            .and_then(|r| r.map_err(|e| applog::write(&format!("TRANSCRIBE failed: {e}"))).ok());
        result.map(|transcript| transcript.text)
    }

    /// Unloads the servers after `SERVER_IDLE` without a dictation (S1-mini stays while it's the chosen engine).
    fn stop_idle_servers(&mut self) {
        if self.last_used.elapsed() < SERVER_IDLE {
            return;
        }
        self.whisper.stop_if_ready();
        if !self.settings.lock().is_ok_and(|s| s.uses_s1()) {
            self.s1.stop_if_ready();
        }
    }
}

/// The transcript to the pre-started session; its report, or None if it failed or took longer than `refine`'s
/// watchdog (then Whisper's text is pasted). Also says whether the session had been pre-started.
fn finish_session(session: JoinHandle<Option<Session>>, raw: String) -> (Option<Report>, bool) {
    let session = session.join().ok().flatten();
    let prestarted = session.is_some();
    let Some(session) = session else { return (None, false) };
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(guarded(|| session.finish(&raw)));
    });
    match receiver.recv_timeout(ovt_core::script::REFINE_TIMEOUT) {
        Ok(report) => (report, prestarted),
        Err(_) => {
            applog::write("SCRIPT refine still running after 45 s, abandoned");
            (None, prestarted)
        }
    }
}

/// Ends a session without work (its pre-started Claude exits), off this thread.
/// How the cleanup gets S1-mini ready when it falls back to it (`refine_s1`'s `srv_start s1-server`): a server that
/// already answers is used as is; otherwise one is started here, and the worker adopts it afterwards so the idle timer
/// stops it.
fn s1_starter(
    config: &Config,
    helpers: &std::path::Path,
    started: Arc<Mutex<Option<Server>>>,
) -> impl FnMut() -> bool + Send + 'static {
    let spec = pipeline::s1_spec(config, helpers);
    move || {
        if ovt_pipeline::servers::healthy(spec.port) {
            return true;
        }
        let Some(server) = super::servers::start(spec.clone()) else { return false };
        let ready = server.healthy();
        *started.lock().unwrap_or_else(|e| e.into_inner()) = Some(server);
        ready
    }
}

fn drop_session(session: JoinHandle<Option<Session>>) {
    std::thread::spawn(move || {
        if let Ok(Some(session)) = session.join() {
            guarded(|| session.finish(""));
        }
    });
}

fn cancelled(hint: Option<String>) -> Finish {
    match hint {
        Some(hint) => {
            applog::write("RESULT cancelled (hold-to-talk tap)");
            Finish::message(hint, false, 1.4, None)
        }
        None => {
            applog::write("RESULT cancelled");
            Finish::message("Cancelled", false, 0.8, Some(Sound::Notice))
        }
    }
}
