//! `APP …` lines in dictate.log (follows `AppLog.swift` and the `RESULT`/`TIMING` lines of `AppDelegate.deliver`), so
//! one log reads the same on every platform. Dictated text is never in these lines.

use std::io::Write;

/// Appends `<date> APP <message>`. Logging never fails a dictation.
pub fn write(message: &str) {
    let path = ovt_core::paths::log_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line(chrono::Local::now().naive_local(), message).as_bytes());
    }
}

pub fn line(now: chrono::NaiveDateTime, message: &str) -> String {
    format!("{} APP {message}\n", now.format("%Y-%m-%d %H:%M:%S"))
}

/// What `RESULT` reports about a delivered dictation.
pub struct ResultFields<'a> {
    /// pasted, copied or withheld-secure.
    pub outcome: &'a str,
    pub mode: &'a str,
    pub app: &'a str,
    pub chars: usize,
    pub rich: bool,
    pub status: &'a str,
    pub engine: &'a str,
    pub error: &'a str,
    /// The meaning guard's reason when it used Whisper's text (its digits only with `log_text`).
    pub guard: Option<&'a str>,
    pub log_text: bool,
    /// same, changed, secure or elevated.
    pub target: &'a str,
    pub cleanup_failed: bool,
    pub offline_fallback: bool,
}

pub fn result_line(f: &ResultFields) -> String {
    let dash = |value: &str| if value.is_empty() { "-".to_string() } else { value.to_string() };
    let mut line = format!(
        "RESULT {} mode={} app=\"{}\" chars={} rich={} status={} engine={}",
        f.outcome,
        f.mode,
        f.app,
        f.chars,
        f.rich,
        dash(f.status),
        dash(f.engine)
    );
    if !f.error.is_empty() {
        line += &format!(" error={}", f.error);
    }
    if let Some(reason) = f.guard {
        let shown = if f.log_text { reason } else { reason.split(" (").next().unwrap_or_default() };
        line += &format!(" guard=\"{shown}\"");
    }
    line += &format!(" target={} cleanupFailed={} offlineFallback={}", f.target, f.cleanup_failed, f.offline_fallback);
    line
}

/// `TIMING` (milliseconds from the moment recording stopped).
pub fn timing_line(transcribe_ms: u64, cleanup_ms: u64, total_ms: u64, prestarted: bool, mode: &str) -> String {
    let pasted = total_ms.saturating_sub(transcribe_ms + cleanup_ms);
    format!(
        "TIMING stop→transcript={transcribe_ms}ms transcript→cleaned={cleanup_ms}ms cleaned→pasted={pasted}ms \
         total={total_ms}ms prestarted={prestarted} mode={mode}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_mac() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 3).unwrap().and_hms_opt(9, 5, 7).unwrap();
        assert_eq!(line(date, "RESULT no-speech"), "2026-10-03 09:05:07 APP RESULT no-speech\n");
        let mut fields = ResultFields {
            outcome: "pasted",
            mode: "chat",
            app: "Slack",
            chars: 42,
            rich: false,
            status: "guard-raw",
            engine: "claude",
            error: "",
            guard: Some("dropped a number (12)"),
            log_text: false,
            target: "same",
            cleanup_failed: false,
            offline_fallback: false,
        };
        assert_eq!(
            result_line(&fields),
            "RESULT pasted mode=chat app=\"Slack\" chars=42 rich=false status=guard-raw engine=claude \
             guard=\"dropped a number\" target=same cleanupFailed=false offlineFallback=false"
        );
        fields.guard = None;
        fields.status = "";
        fields.error = "limit";
        assert!(result_line(&fields).contains("status=- engine=claude error=limit target=same"));
        assert_eq!(
            timing_line(800, 2000, 3000, true, "default"),
            "TIMING stop→transcript=800ms transcript→cleaned=2000ms cleaned→pasted=200ms total=3000ms \
             prestarted=true mode=default"
        );
    }
}
