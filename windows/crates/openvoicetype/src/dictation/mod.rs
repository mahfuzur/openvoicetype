//! The dictation state machine (follows `Dictation.swift`): idle → starting → recording → transcribing → polishing →
//! idle, on its own thread so the window's message loop never waits. The app sends it commands (start, stop, cancel)
//! and gets `UiEvent`s back for the tray, the overlay and the sounds.
//!
//! Where the text goes is decided when recording starts: the target window and the mode for its app. The slow parts
//! start while you speak: whisper-server loads, and the cleanup `Session` pre-starts Claude.

mod deliver;
mod servers;
mod worker;

use crate::overlay::scene::Phase;
use crate::sounds::Sound;
use ovt_core::settings::Settings;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::Foundation::HWND;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    Idle,
    Starting,
    Recording,
    Transcribing,
    Polishing,
}

impl State {
    fn from_u8(value: u8) -> State {
        [State::Idle, State::Starting, State::Recording, State::Transcribing, State::Polishing]
            .get(value as usize)
            .copied()
            .unwrap_or(State::Idle)
    }
}

enum Command {
    Start,
    /// Stop recording and process it (before the mic is ready: cancel).
    Stop,
    /// Drop the recording; the message replaces "Cancelled" (the Hold to Talk hint).
    Cancel(Option<String>),
    Quit,
}

/// What the app shows and plays.
pub enum UiEvent {
    State(State),
    /// A slow (Bluetooth) mic is still starting.
    Connecting(String),
    Finished(Finish),
}

/// The end of a dictation: the overlay's last state, how long it stays, a sound, and the text for Copy Last.
pub struct Finish {
    pub phase: Phase,
    pub hide_after: Duration,
    pub sound: Option<Sound>,
    pub text: Option<String>,
}

impl Finish {
    fn message(text: impl Into<String>, error: bool, seconds: f32, sound: Option<Sound>) -> Finish {
        Finish {
            phase: Phase::Message { text: text.into(), error },
            hide_after: Duration::from_secs_f32(seconds),
            sound,
            text: None,
        }
    }
}

/// The app's end of the dictation thread.
pub struct Handle {
    commands: Sender<Command>,
    state: Arc<AtomicU8>,
    thread: Option<JoinHandle<()>>,
}

impl Handle {
    /// `owner` is the app's window (the clipboard's owner); `level` is where the mic's level goes for the overlay;
    /// `notify` is called on the dictation thread.
    pub fn spawn(
        settings: Arc<Mutex<Settings>>,
        owner: HWND,
        level: Arc<AtomicU32>,
        notify: impl Fn(UiEvent) + Send + 'static,
    ) -> Handle {
        let (commands, receiver) = mpsc::channel();
        let state = Arc::new(AtomicU8::new(State::Idle as u8));
        let worker = worker::Worker::new(receiver, settings, owner, level, Box::new(notify), Arc::clone(&state));
        let thread = std::thread::Builder::new().name("dictation".into()).spawn(move || worker.run()).ok();
        Handle { commands, state, thread }
    }

    pub fn state(&self) -> State {
        State::from_u8(self.state.load(Ordering::SeqCst))
    }

    /// Starts a dictation if none is running. The state turns `Starting` at once, so a second press can't race it.
    pub fn start(&self) -> bool {
        let idle = State::Idle as u8;
        let started =
            self.state.compare_exchange(idle, State::Starting as u8, Ordering::SeqCst, Ordering::SeqCst).is_ok();
        started && self.commands.send(Command::Start).is_ok()
    }

    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    pub fn cancel(&self, hint: Option<String>) {
        let _ = self.commands.send(Command::Cancel(hint));
    }

    /// Ends a recording in progress and stops the model servers. Waits up to 5 s: a dictation that is transcribing
    /// or polishing finishes first, and one that takes longer is abandoned.
    pub fn shutdown(mut self) {
        let _ = self.commands.send(Command::Quit);
        let Some(thread) = self.thread.take() else { return };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !thread.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
    }
}
