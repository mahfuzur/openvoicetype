//! The last 10 dictations, in memory only (follows `DictationHistory.swift`): Whisper's text next to the cleaned text,
//! for Recent Dictations and for swapping the last paste between the two.

use crate::mode::DictationMode;
use crate::target::PasteTarget;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Entry {
    pub date: Instant,
    pub app_name: String,
    pub mode: DictationMode,
    /// Whisper's transcript as it would be pasted (dictionary and output filter applied).
    pub raw: String,
    /// The cleanup, or None when there was none. When the meaning guard turned it down, this is the rejected one.
    pub cleaned: Option<String>,
    /// The meaning guard's reason, when it pasted Whisper's text instead of the cleanup.
    pub guard_reason: Option<String>,
    pub target: PasteTarget,
    /// It went into the app (not just onto the clipboard), so undo can take it back.
    pub was_pasted: bool,
    /// Whisper's text is the one in the app now (the guard used it, or the user swapped to it).
    pub showing_raw: bool,
}

impl Entry {
    /// The version in the app now.
    pub fn shown(&self) -> &str {
        if self.showing_raw {
            &self.raw
        } else {
            self.cleaned.as_deref().unwrap_or(&self.raw)
        }
    }

    /// The other version of the text, if there is one.
    pub fn alternative(&self) -> Option<&str> {
        let cleaned = self.cleaned.as_deref()?;
        if cleaned == self.raw {
            return None;
        }
        Some(if self.showing_raw { cleaned } else { &self.raw })
    }
}

#[derive(Default, Debug)]
pub struct DictationHistory {
    entries: Vec<Entry>,
}

impl DictationHistory {
    pub const LIMIT: usize = 10;

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn last(&self) -> Option<&Entry> {
        self.entries.first()
    }

    pub fn add(&mut self, entry: Entry) {
        self.entries.insert(0, entry);
        self.entries.truncate(Self::LIMIT);
    }

    /// Something else was pasted since (a Command Mode result): undo would no longer take back the dictation, so a swap
    /// only copies.
    pub fn invalidate_last(&mut self) {
        if let Some(entry) = self.entries.first_mut() {
            entry.was_pasted = false;
        }
    }

    /// After a swap: the other version is now in the app.
    pub fn swapped_last(&mut self, pasted: bool) {
        if let Some(entry) = self.entries.first_mut() {
            entry.showing_raw = !entry.showing_raw;
            entry.was_pasted = pasted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(raw: &str, cleaned: Option<&str>, showing_raw: bool) -> Entry {
        Entry {
            date: Instant::now(),
            app_name: "Test".into(),
            mode: DictationMode::Default,
            raw: raw.into(),
            cleaned: cleaned.map(Into::into),
            guard_reason: None,
            target: PasteTarget::default(),
            was_pasted: true,
            showing_raw,
        }
    }

    #[test]
    fn swaps_and_limits() {
        // The swap case from LogicSelfTest.swift.
        let mut history = DictationHistory::default();
        history.add(entry("um we have 12 users", Some("We have 12 users."), false));
        let first = history.last().unwrap().alternative().map(str::to_string);
        history.swapped_last(true);
        let second = history.last().unwrap().alternative().map(str::to_string);
        history.add(entry("hello", None, true));
        assert_eq!(first.as_deref(), Some("um we have 12 users"));
        assert_eq!(second.as_deref(), Some("We have 12 users."));
        assert_eq!(history.last().unwrap().alternative(), None);
        for _ in 0..12 {
            history.add(entry("x", None, true));
        }
        assert_eq!(history.entries().len(), 10);
        history.invalidate_last();
        assert!(!history.last().unwrap().was_pasted);
    }
}
