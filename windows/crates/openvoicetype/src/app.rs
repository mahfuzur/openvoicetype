//! The app (follows `AppDelegate.swift`): one hidden window on the main thread receives the hotkeys, the dictation
//! thread's events, menu clicks and timers, and drives the tray icon, the overlay and the sounds. Everything that waits
//! (recording, Whisper, the cleanup, the paste) happens on the dictation thread.

use crate::dictation::{self, State, UiEvent};
use crate::hotkey::keys::{self, WinCombo};
use crate::hotkey::{self, ID_DICTATION, ID_ESCAPE, WM_HOTKEY_RELEASED};
use crate::overlay::scene::Phase;
use crate::overlay::Overlay;
use crate::sounds::{self, Sound};
use crate::tray::{Action, Tray};
use crate::{applog, paste, pipeline};
use ovt_core::paths;
use ovt_core::settings::{OverlayPosition, Settings};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tray_icon::menu::MenuEvent;
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, KillTimer, PostMessageW, PostQuitMessage,
    RegisterClassW, SetTimer, TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_HOTKEY,
    WM_TIMER, WNDCLASSW,
};

/// "The dictation thread has events" and "the menu was clicked": drain the channel.
const WM_APP_UI: u32 = WM_APP + 2;
const WM_APP_MENU: u32 = WM_APP + 3;
const TIMER_OVERLAY: usize = 1;
const TIMER_SETTINGS: usize = 2;
/// About 30 frames a second while the overlay is up.
const FRAME_MS: u32 = 33;
/// Hold to Talk: a press shorter than this is a tap, and gets the hint instead of a recording.
const HOLD_MINIMUM: Duration = Duration::from_millis(300);

struct App {
    hwnd: HWND,
    settings: Arc<Mutex<Settings>>,
    /// The settings file's modification time when last read or written (edits in Notepad are picked up).
    settings_stamp: Option<SystemTime>,
    tray: Tray,
    overlay: Overlay,
    dictation: dictation::Handle,
    ui: Receiver<UiEvent>,
    menu: Receiver<MenuEvent>,
    level: Arc<AtomicU32>,
    combo: Option<WinCombo>,
    escape_registered: bool,
    overlay_timer: bool,
    /// Hold to Talk: when the press that started this dictation happened.
    hold_started: Option<Instant>,
    /// For Copy Last Dictation (memory only).
    last_text: Option<String>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs the app until Quit; the process's exit code.
pub fn run() -> i32 {
    let Some(instance) = single_instance() else { return 0 };
    // SAFETY: process-wide setting, made before any window exists.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let app = match create_window().map_err(|e| e.to_string()).and_then(App::new) {
        Ok(app) => app,
        Err(error) => {
            applog::write(&format!("START failed: {error}"));
            return 1;
        }
    };
    applog::write(&format!("START OpenVoiceType {} (Windows)", env!("CARGO_PKG_VERSION")));
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    // SAFETY: the standard message loop of this thread.
    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    if let Some(app) = APP.with(|cell| cell.borrow_mut().take()) {
        app.dictation.shutdown();
    }
    applog::write("QUIT");
    // SAFETY: our mutex, released at exit anyway.
    let _ = unsafe { CloseHandle(instance) };
    0
}

/// A named mutex: a second copy exits at once (both would want the same hotkey).
fn single_instance() -> Option<windows::Win32::Foundation::HANDLE> {
    // SAFETY: plain Win32 calls.
    unsafe {
        let handle = CreateMutexW(None, true, w!("Local\\OpenVoiceType.SingleInstance")).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            return None;
        }
        Some(handle)
    }
}

/// A message-only window: hotkeys, timers, our own messages, and the clipboard's owner.
fn create_window() -> windows::core::Result<HWND> {
    // SAFETY: registers our class and creates the window with it.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: w!("OpenVoiceType.Main"),
            ..Default::default()
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("OpenVoiceType.Main"),
            w!("OpenVoiceType"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
    }
}

extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ours = matches!(message, WM_HOTKEY | WM_TIMER | WM_APP_UI | WM_APP_MENU | WM_HOTKEY_RELEASED);
    if !ours {
        // SAFETY: default handling for everything else.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let handled = APP.with(|cell| match cell.try_borrow_mut() {
        Ok(mut slot) => {
            if let Some(app) = slot.as_mut() {
                app.handle(message, wparam);
            }
            true
        }
        Err(_) => false,
    });
    // Reentered (a nested message loop while the app was busy): try the channels again later.
    if !handled && (message == WM_APP_UI || message == WM_APP_MENU) {
        // SAFETY: posting to our own window.
        let _ = unsafe { PostMessageW(Some(hwnd), message, wparam, lparam) };
    }
    LRESULT(0)
}

/// Posts `message` to the window from any thread.
fn poster(hwnd: HWND, message: u32) -> impl Fn() + Send + Sync + 'static {
    let raw = hwnd.0 as isize;
    move || {
        // SAFETY: posting to our window; after it's gone this only fails.
        let _ = unsafe { PostMessageW(Some(HWND(raw as *mut _)), message, WPARAM(0), LPARAM(0)) };
    }
}

impl App {
    fn new(hwnd: HWND) -> Result<App, String> {
        let path = paths::settings_file();
        let settings = Settings::load(&path);
        let tray = Tray::new(&settings, "Starting…")?;
        let overlay = Overlay::new().map_err(|e| e.to_string())?;
        let settings = Arc::new(Mutex::new(settings));
        let level = Arc::new(AtomicU32::new(0));

        let (ui_sender, ui) = mpsc::channel();
        let wake_ui = poster(hwnd, WM_APP_UI);
        let notify = move |event| {
            let _ = ui_sender.send(event);
            wake_ui();
        };
        let dictation = dictation::Handle::spawn(Arc::clone(&settings), hwnd, Arc::clone(&level), notify);

        let (menu_sender, menu) = mpsc::channel();
        let wake_menu = poster(hwnd, WM_APP_MENU);
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = menu_sender.send(event);
            wake_menu();
        }));

        let mut app = App {
            hwnd,
            settings,
            settings_stamp: modified(&path),
            tray,
            overlay,
            dictation,
            ui,
            menu,
            level,
            combo: None,
            escape_registered: false,
            overlay_timer: false,
            hold_started: None,
            last_text: None,
        };
        app.apply_settings();
        // SAFETY: a timer on our own window.
        unsafe { SetTimer(Some(hwnd), TIMER_SETTINGS, 2000, None) };
        Ok(app)
    }

    fn handle(&mut self, message: u32, wparam: WPARAM) {
        match message {
            WM_HOTKEY if wparam.0 as i32 == ID_DICTATION => self.hotkey_pressed(),
            WM_HOTKEY if wparam.0 as i32 == ID_ESCAPE => self.dictation.cancel(None),
            WM_HOTKEY_RELEASED => self.hotkey_released(),
            WM_APP_UI => {
                while let Ok(event) = self.ui.try_recv() {
                    self.ui_event(event);
                }
            }
            WM_APP_MENU => {
                while let Ok(event) = self.menu.try_recv() {
                    if let Some(action) = Action::from_id(&event.id.0) {
                        self.action(action);
                    }
                }
            }
            WM_TIMER if wparam.0 == TIMER_OVERLAY => self.tick_overlay(),
            WM_TIMER if wparam.0 == TIMER_SETTINGS => self.reload_if_changed(),
            _ => {}
        }
    }

    fn settings(&self) -> Settings {
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    // MARK: hotkey

    fn hotkey_pressed(&mut self) {
        let settings = self.settings();
        let hold = settings.hold_to_talk;
        let state = self.dictation.state();
        if !hold && state != State::Idle {
            // The second press stops (or cancels a mic that isn't ready); busy stages ignore it.
            match state {
                State::Starting => self.dictation.cancel(None),
                State::Recording => self.dictation.stop(),
                _ => {}
            }
            return;
        }
        if state != State::Idle || self.speech_model_missing(&settings) {
            return;
        }
        if self.dictation.start() {
            self.update_escape(State::Starting);
            if let (true, Some(combo)) = (hold, self.combo) {
                self.hold_started = Some(Instant::now());
                hotkey::watch_release(self.hwnd, combo);
            }
        }
    }

    fn hotkey_released(&mut self) {
        let Some(started) = self.hold_started.take() else { return };
        if started.elapsed() < HOLD_MINIMUM {
            let label = self.settings().hot_key.label();
            self.dictation.cancel(Some(format!("Hold {label} while you speak")));
        } else if self.dictation.state() == State::Recording {
            self.dictation.stop();
        } else {
            self.dictation.cancel(None);
        }
    }

    /// No Whisper model yet: say where it goes instead of failing to transcribe. (The setup window comes in W5.)
    fn speech_model_missing(&mut self, settings: &Settings) -> bool {
        if pipeline::whisper_model(settings).is_file() {
            return false;
        }
        let message = format!("No speech model: put {} in {}", settings.whisper_model, paths::whisper_dir().display());
        self.finish_overlay(Phase::Message { text: message, error: false }, Duration::from_secs(4));
        true
    }

    /// Esc cancels, but only while recording: registered globally, it would take Esc from every app.
    fn update_escape(&mut self, state: State) {
        let wanted = matches!(state, State::Starting | State::Recording);
        if wanted == self.escape_registered {
            return;
        }
        if wanted {
            let escape = WinCombo { modifiers: 0, vk: keys::VK_ESCAPE };
            self.escape_registered = hotkey::register(self.hwnd, ID_ESCAPE, escape);
        } else {
            hotkey::unregister(self.hwnd, ID_ESCAPE);
            self.escape_registered = false;
        }
    }

    // MARK: dictation events

    fn ui_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::State(state) => self.state_changed(state),
            UiEvent::Connecting(device) => self.show_overlay(Phase::Connecting(device)),
            UiEvent::Finished(finish) => {
                if finish.text.is_some() {
                    self.last_text = finish.text;
                }
                if let Some(sound) = finish.sound {
                    self.play(sound);
                }
                self.finish_overlay(finish.phase, finish.hide_after);
            }
        }
    }

    fn state_changed(&mut self, state: State) {
        self.tray.set_busy(state != State::Idle);
        self.tray.set_status(&self.status_text(state));
        self.update_escape(state);
        match state {
            State::Idle if self.hold_started.take().is_some() => hotkey::stop_watching(),
            State::Recording => {
                self.play(Sound::Start);
                self.show_overlay(Phase::Recording);
            }
            State::Transcribing => {
                self.play(Sound::Stop);
                self.show_overlay(Phase::Transcribing);
            }
            State::Polishing => {
                let offline = self.settings().uses_s1();
                self.show_overlay(Phase::Polishing { offline });
            }
            _ => {}
        }
    }

    fn status_text(&self, state: State) -> String {
        let settings = self.settings();
        let label = settings.hot_key.label();
        match state {
            State::Idle if self.combo.is_none() => format!("{label} isn't available: pick another hotkey"),
            State::Idle => {
                format!("Ready. {} {label} to dictate", if settings.hold_to_talk { "Hold" } else { "Press" })
            }
            State::Starting => "Starting the microphone…".into(),
            State::Recording if settings.hold_to_talk => format!("Recording… release {label} to finish"),
            State::Recording => format!("Recording… press {label} to finish, Esc to cancel"),
            State::Transcribing => "Transcribing…".into(),
            State::Polishing => "Polishing…".into(),
        }
    }

    fn play(&self, sound: Sound) {
        sounds::play(sound, self.settings().play_sounds);
    }

    // MARK: overlay

    fn show_overlay(&mut self, phase: Phase) {
        self.overlay.show(phase);
        self.start_frames();
    }

    fn finish_overlay(&mut self, phase: Phase, after: Duration) {
        self.overlay.finish(phase, after);
        self.start_frames();
    }

    fn start_frames(&mut self) {
        if self.overlay.is_active() && !self.overlay_timer {
            // SAFETY: a timer on our own window.
            unsafe { SetTimer(Some(self.hwnd), TIMER_OVERLAY, FRAME_MS, None) };
            self.overlay_timer = true;
        }
    }

    fn tick_overlay(&mut self) {
        let level = f32::from_bits(self.level.swap(0, Ordering::Relaxed));
        self.overlay.tick(level);
        if !self.overlay.is_active() && self.overlay_timer {
            // SAFETY: our own timer.
            let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_OVERLAY) };
            self.overlay_timer = false;
        }
    }

    // MARK: menu and settings

    fn action(&mut self, action: Action) {
        match action {
            Action::Mode(mode) => self.change_settings(|s| s.mode_override = mode),
            Action::Engine(engine) => self.change_settings(|s| s.cleanup_engine = engine),
            Action::CopyLast => self.copy_last(),
            Action::Settings => {
                let path = paths::settings_file();
                if !path.exists() {
                    let _ = self.settings().save(&path);
                    self.settings_stamp = modified(&path);
                }
                open_in_notepad(&path);
            }
            Action::OpenLog => {
                let path = paths::log_file();
                if !path.exists() {
                    applog::write("LOG opened");
                }
                open_in_notepad(&path);
            }
            // SAFETY: ends this thread's message loop.
            Action::Quit => unsafe { PostQuitMessage(0) },
        }
    }

    fn copy_last(&mut self) {
        let (phase, after) = match &self.last_text {
            Some(text) if paste::copy(self.hwnd, text, None) => {
                (Phase::Success("Copied the last dictation. Press Ctrl+V".into()), 1.6)
            }
            Some(_) => (Phase::Message { text: "The clipboard is busy. Try again".into(), error: true }, 2.0),
            None => (Phase::Message { text: "No dictation yet".into(), error: false }, 1.6),
        };
        self.finish_overlay(phase, Duration::from_secs_f32(after));
    }

    fn change_settings(&mut self, change: impl FnOnce(&mut Settings)) {
        let path = paths::settings_file();
        let settings = {
            let mut settings = self.settings.lock().unwrap_or_else(|e| e.into_inner());
            change(&mut settings);
            settings.clone()
        };
        if let Err(error) = settings.save(&path) {
            applog::write(&format!("SETTINGS not saved: {error}"));
        }
        self.settings_stamp = modified(&path);
        self.apply_settings();
    }

    /// The file changed on disk (edited in Notepad): load it and apply it.
    fn reload_if_changed(&mut self) {
        let path = paths::settings_file();
        let stamp = modified(&path);
        if stamp == self.settings_stamp {
            return;
        }
        self.settings_stamp = stamp;
        *self.settings.lock().unwrap_or_else(|e| e.into_inner()) = Settings::load(&path);
        applog::write("SETTINGS reloaded");
        self.apply_settings();
    }

    /// Hotkey, menu ticks, overlay placement.
    fn apply_settings(&mut self) {
        let settings = self.settings();
        self.tray.update(&settings);
        self.overlay.enabled = settings.show_overlay;
        self.overlay.at_top = settings.overlay_position == OverlayPosition::Top;
        let combo = keys::win_combo(&settings.hot_key);
        if combo != self.combo || combo.is_none() {
            hotkey::unregister(self.hwnd, ID_DICTATION);
            self.combo = combo.filter(|c| hotkey::register(self.hwnd, ID_DICTATION, *c));
            if self.combo.is_none() {
                let label = settings.hot_key.label();
                applog::write(&format!("HOTKEY {label} unavailable"));
                let text = format!("{label} is used by another app. Pick another hotkey in Settings");
                self.finish_overlay(Phase::Message { text, error: true }, Duration::from_secs(4));
            }
        }
        self.tray.set_status(&self.status_text(self.dictation.state()));
    }
}

fn modified(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Settings and the log open in Notepad until the Settings window exists (W5).
fn open_in_notepad(path: &std::path::Path) {
    if let Err(error) = std::process::Command::new("notepad.exe").arg(path).spawn() {
        applog::write(&format!("NOTEPAD failed: {error}"));
    }
}
