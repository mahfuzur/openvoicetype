//! OpenVoiceType for Windows (docs/plans/M8-windows.md, W2): a tray app that records while the hotkey is down (or
//! between two presses), transcribes with Whisper, cleans up with the user's own Claude Code (or S1-mini, or an
//! OpenAI-compatible endpoint) through `ovt-pipeline`, and pastes into the window that was in front.
//!
//! The macOS app (`app/Sources/VoiceToText/`) is the specification: each module names the Swift file it follows. The
//! logic that doesn't touch Win32 (key tables, WAV, drawing, menu ids, log lines) is plain Rust tested on every
//! platform; the rest is Windows-only, and elsewhere this binary only says so.
//!
//! Flags: `--logic-selftest [report.txt]` runs the checks that need no UI; `--console` attaches to the terminal it was
//! started from (for panics and debugging).

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code))]

mod applog;
mod hotkey;
mod overlay;
mod paste;
mod pipeline;
mod recorder;
mod selftest;
mod tray;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod dictation;
#[cfg(windows)]
mod sounds;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--console" || a == "--logic-selftest") {
        attach_console();
    }
    std::panic::set_hook(Box::new(|info| {
        applog::write(&format!("PANIC {info}"));
        eprintln!("{info}");
    }));
    if let Some(index) = args.iter().position(|a| a == "--logic-selftest") {
        let report = args.get(index + 1).filter(|a| !a.starts_with("--")).map(std::path::Path::new);
        std::process::exit(selftest::run(report));
    }
    #[cfg(windows)]
    std::process::exit(app::run());
    #[cfg(not(windows))]
    {
        eprintln!("OpenVoiceType for Windows runs on Windows only (the Mac app is in app/, the Linux one in linux/).");
        std::process::exit(1);
    }
}

/// A GUI-subsystem program has no console: borrow the one of the terminal that started it, so printing works.
fn attach_console() {
    #[cfg(windows)]
    // SAFETY: fails harmlessly when started from Explorer (no parent console).
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
