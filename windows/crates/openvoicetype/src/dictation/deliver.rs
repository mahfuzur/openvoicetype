//! Pastes a finished dictation (follows `AppDelegate.deliver`): unless focus moved (then it's copied), a password box
//! has focus (then it's only kept for Copy Last), or the window runs as administrator (copied: our keys can't reach
//! it). Says what happened, including why cleanup didn't run, and logs the `RESULT` and `TIMING` lines.

use super::worker::guarded;
use super::Finish;
use crate::applog::{self, ResultFields};
use crate::overlay::scene::Phase;
use crate::paste::{self, target};
use crate::sounds::Sound;
use ovt_core::mode::DictationMode;
use ovt_core::script::{self, CleanupDetails};
use ovt_core::settings::Settings;
use ovt_pipeline::refine::Report;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

/// Stage timings for the `TIMING` line.
pub struct Timing {
    pub stopped_at: Instant,
    pub transcribe_ms: u64,
    pub cleanup_ms: u64,
    /// The transcript went to a session started when recording began.
    pub prestarted: bool,
}

impl Default for Timing {
    fn default() -> Self {
        Timing { stopped_at: Instant::now(), transcribe_ms: 0, cleanup_ms: 0, prestarted: false }
    }
}

pub struct Delivery<'a> {
    pub owner: HWND,
    pub settings: &'a Settings,
    pub target: &'a target::Target,
    pub mode: DictationMode,
    /// Whisper's text.
    pub raw: String,
    /// None: the cleanup failed to run at all (or timed out).
    pub report: Option<Report>,
    pub uses_cleanup: bool,
    pub timing: Timing,
}

fn details(report: Option<&Report>) -> CleanupDetails {
    let Some(r) = report else { return CleanupDetails::default() };
    CleanupDetails {
        status: r.status.clone(),
        engine: r.engine.clone(),
        error: r.error.clone(),
        resets: r.resets.clone(),
        guard_reason: r.guard.clone(),
        rejected: r.rejected.clone(),
        raw: r.raw.clone(),
    }
}

pub fn deliver(d: Delivery) -> Finish {
    let exit = d.report.as_ref().and_then(|r| guarded(|| r.exit_code()));
    let output = d.report.as_ref().map_or("", |r| r.result.as_str());
    let outcome = script::interpret_refine(exit, output, &d.raw, d.uses_cleanup);
    let details = details(d.report.as_ref());
    let text = outcome.text.clone();
    let rich = d.settings.rich_paste && d.mode != DictationMode::Code && ovt_core::richtext::contains_list(&text);
    let html = rich.then(|| ovt_core::richtext::html(&text));
    let check = if d.settings.auto_paste { d.target.check() } else { target::Check::Same };
    let problem = script::problem_description(&details, &d.settings.cleanup_engine);
    let guarded_raw = details.status == "guard-raw";
    let mode_suffix = if d.mode == DictationMode::Default { String::new() } else { format!(" · {}", d.mode.title()) };
    let with_problem = |label: &str| problem.as_ref().map_or(label.to_string(), |p| format!("{label} · {p}"));

    let mut pasted = false;
    let label = match &check {
        target::Check::Secure => "Not pasted: a password field has focus (see Copy Last Dictation)".to_string(),
        target::Check::Changed(place) => {
            paste::copy(d.owner, &text, html.as_deref());
            format!("Copied: you switched to {place}. Press Ctrl+V")
        }
        target::Check::Elevated => {
            paste::copy(d.owner, &text, html.as_deref());
            format!("Copied: {} runs as administrator. Press Ctrl+V", d.target.target.app_name)
        }
        target::Check::Same if !d.settings.auto_paste => {
            paste::copy(d.owner, &text, html.as_deref());
            format!("Copied{mode_suffix}")
        }
        target::Check::Same => {
            pasted = paste::paste(d.owner, d.target, &text, html.as_deref());
            if !pasted {
                "Couldn't paste: the clipboard is busy (see Copy Last Dictation)".to_string()
            } else if guarded_raw {
                format!("Pasted Whisper's text: the cleanup dropped {}", details.guard_reason)
            } else if outcome.offline_fallback {
                with_problem("Pasted · cleaned offline")
            } else if outcome.cleanup_failed {
                with_problem("Pasted without cleanup")
            } else {
                format!("Pasted{mode_suffix}")
            }
        }
    };
    log(&d, &details, &check, pasted, rich, &outcome);

    let notice = check != target::Check::Same || guarded_raw || problem.is_some();
    let success = check == target::Check::Same && pasted && !notice;
    Finish {
        phase: if success { Phase::Success(label) } else { Phase::Message { text: label, error: false } },
        hide_after: Duration::from_secs_f32(if pasted && !notice { 0.9 } else { 3.0 }),
        sound: (check == target::Check::Secure).then_some(Sound::Notice),
        text: Some(text),
    }
}

fn log(
    d: &Delivery,
    details: &CleanupDetails,
    check: &target::Check,
    pasted: bool,
    rich: bool,
    outcome: &script::RefineOutcome,
) {
    let target = match check {
        target::Check::Same => "same",
        target::Check::Changed(_) => "changed",
        target::Check::Secure => "secure",
        target::Check::Elevated => "elevated",
    };
    let result = if pasted {
        "pasted"
    } else if *check == target::Check::Secure {
        "withheld-secure"
    } else {
        "copied"
    };
    applog::write(&applog::result_line(&ResultFields {
        outcome: result,
        mode: d.mode.as_str(),
        app: &d.target.target.app_name,
        chars: outcome.text.chars().count(),
        rich,
        status: &details.status,
        engine: &details.engine,
        error: &details.error,
        guard: (details.status == "guard-raw").then_some(details.guard_reason.as_str()),
        log_text: d.settings.log_text,
        target,
        cleanup_failed: outcome.cleanup_failed,
        offline_fallback: outcome.offline_fallback,
    }));
    let t = &d.timing;
    let total = t.stopped_at.elapsed().as_millis() as u64;
    applog::write(&applog::timing_line(t.transcribe_ms, t.cleanup_ms, total, t.prestarted, d.mode.as_str()));
}
