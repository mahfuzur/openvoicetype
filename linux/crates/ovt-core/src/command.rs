//! Command Mode planning (follows `CommandMode.swift`): what a press acts on, decided as soon as the key goes down so the
//! overlay can say it before you speak, and the session that follow-ups continue.
//!
//! Reading the selection and re-selecting text are the desktop's job (AT-SPI, a Ctrl+C with a marker, PRIMARY in
//! terminals); they come in as a `Selection` and the `check`/`reselect` callbacks.

use crate::history::Entry;
use crate::target::{PasteTarget, TargetCheck};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandTarget {
    /// Replace the selected text.
    Selection,
    /// Replace what was just dictated (selected again through accessibility).
    LastDictation,
    /// New text at the cursor.
    Write,
    /// The answer goes to the clipboard: text you can't edit, a terminal, or no text field.
    Copy,
}

impl CommandTarget {
    /// The value in `VTT_COMMAND_FILE`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::LastDictation => "last_dictation",
            Self::Write => "write",
            Self::Copy => "copy",
        }
    }
}

/// How the selection was read. Linux has no "press Edit ▸ Copy through accessibility" (the macOS `menu` source).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionSource {
    /// AT-SPI Text selection.
    Accessibility,
    /// A Ctrl+C with a marker on the clipboard.
    Copy,
    /// The PRIMARY selection (terminals, where Ctrl+C would interrupt the program).
    Primary,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    /// The selected text, or None when nothing is selected (or it couldn't be read).
    pub text: Option<String>,
    pub source: SelectionSource,
    /// The focused element takes text. True when unknown.
    pub editable: bool,
    /// Accessibility could say whether a text field has focus.
    pub knows_focus: bool,
    /// The selection couldn't be read: there may be one, so the answer must not be pasted over it.
    pub inconclusive: bool,
}

impl Selection {
    pub fn none() -> Self {
        Selection { text: None, source: SelectionSource::None, editable: true, knows_focus: false, inconclusive: false }
    }
}

/// One edit and its follow-ups ("shorter still"). In memory only; a follow-up has to come within a minute, and Restore
/// Original works for 5 minutes.
#[derive(Debug)]
pub struct CommandSession {
    pub target: PasteTarget,
    pub original: String,
    pub started_as: CommandTarget,
    pub instructions: Vec<String>,
    pub current: Option<String>,
    /// The latest result went into the app (not just onto the clipboard).
    pub pasted_last: bool,
    pub last_used: Instant,
}

pub type SharedSession = Rc<RefCell<CommandSession>>;

const ALIVE: Duration = Duration::from_secs(300);
const FOLLOW_UP: Duration = Duration::from_secs(60);

impl CommandSession {
    pub fn new(target: PasteTarget, original: String, started_as: CommandTarget, now: Instant) -> Self {
        CommandSession {
            target,
            original,
            started_as,
            instructions: Vec::new(),
            current: None,
            pasted_last: false,
            last_used: now,
        }
    }

    pub fn is_alive(&self, now: Instant) -> bool {
        now.duration_since(self.last_used) < ALIVE
    }

    pub fn can_follow_up(&self, now: Instant) -> bool {
        now.duration_since(self.last_used) < FOLLOW_UP && self.current.is_some()
    }

    /// There's an original to put back (not for Write, where there was none).
    pub fn can_restore(&self, now: Instant) -> bool {
        self.is_alive(now)
            && !self.original.is_empty()
            && self.current.is_some()
            && self.current.as_ref() != Some(&self.original)
    }
}

#[derive(Debug)]
pub struct CommandPlan {
    pub target: CommandTarget,
    /// The text the instruction applies to ("" for Write).
    pub text: String,
    pub session: SharedSession,
    /// For a replace: the text that must still be selected when the result arrives.
    pub expected_selection: Option<String>,
    pub source: SelectionSource,
    /// When the command selected our last paste itself: where the cursor was, to put it back if nothing is pasted.
    pub restore_cursor: Option<i32>,
}

impl CommandPlan {
    pub fn is_follow_up(&self) -> bool {
        !self.session.borrow().instructions.is_empty()
    }

    /// The overlay's chip while recording.
    pub fn chip(&self) -> String {
        let words = self.text.split_whitespace().count();
        let count = format!("{words} word{}", if words == 1 { "" } else { "s" });
        match self.target {
            CommandTarget::Selection if self.is_follow_up() => format!("Follow-up · {count}"),
            CommandTarget::Selection => format!("{count} selected"),
            CommandTarget::LastDictation => format!("Last dictation · {count}"),
            CommandTarget::Write => "Write at cursor".into(),
            CommandTarget::Copy if self.source == SelectionSource::Copy && self.text.is_empty() => {
                "Copy only · couldn't read the selection".into()
            }
            CommandTarget::Copy if self.text.is_empty() => "Copy only · no text field".into(),
            CommandTarget::Copy => format!("Copy only · {count}"),
        }
    }

    /// While the model works.
    pub fn working_label(&self) -> &'static str {
        if self.target == CommandTarget::Write || (self.target == CommandTarget::Copy && self.text.is_empty()) {
            "Writing"
        } else {
            "Editing"
        }
    }

    /// `VTT_COMMAND_FILE` for `dictate.sh command`.
    pub fn payload(&self) -> Value {
        let session = self.session.borrow();
        let mut payload = json!({ "target": self.target.as_str(), "original": session.original });
        if !session.instructions.is_empty() {
            payload["current"] = json!(session.current.clone().unwrap_or_default());
            payload["turns"] = session.instructions.iter().map(|i| json!({ "instruction": i })).collect();
        }
        payload
    }
}

#[derive(Debug)]
pub enum Decision {
    Plan(CommandPlan),
    /// Command Mode can't run here; the message says why.
    Refuse(String),
}

pub const MAX_CHARACTERS: usize = 6_000;

/// What the planner needs from the desktop.
pub struct Context<'a> {
    pub selection: &'a Selection,
    pub target: &'a PasteTarget,
    pub session: Option<SharedSession>,
    pub last_dictation: Option<&'a Entry>,
    /// Compares a captured target with what has focus now.
    pub check: &'a dyn Fn(&PasteTarget) -> TargetCheck,
    /// Selects a text that ends at the cursor (our last paste) and returns where the cursor was, or None if it couldn't.
    /// Only called when nothing is selected.
    pub reselect: &'a dyn Fn(&str) -> Option<i32>,
    pub now: Instant,
}

fn new_session(target: &PasteTarget, original: &str, started_as: CommandTarget, now: Instant) -> SharedSession {
    Rc::new(RefCell::new(CommandSession::new(target.clone(), original.to_string(), started_as, now)))
}

/// Decides what a press acts on (follows `CommandPlanner.decide`).
pub fn decide(ctx: Context) -> Decision {
    let terminal = ctx.target.is_terminal();
    let selection = ctx.selection;
    if let Some(text) = selection.text.as_deref().filter(|t| !t.trim().is_empty()) {
        // UTF-16 units, as the macOS app counts.
        if text.encode_utf16().count() > MAX_CHARACTERS {
            return Decision::Refuse("Selection too long (6,000 characters max)".into());
        }
        let replaceable = !terminal && selection.editable;
        // Our last result, selected again: a follow-up in the same session.
        if let Some(session) = &ctx.session {
            let follow_up = {
                let s = session.borrow();
                s.is_alive(ctx.now)
                    && s.current.as_deref().is_some_and(|current| squeezed(text) == squeezed(current))
                    && (ctx.check)(&s.target) == TargetCheck::Same
            };
            if follow_up {
                let current = session.borrow().current.clone().unwrap_or_default();
                return Decision::Plan(CommandPlan {
                    target: if replaceable { CommandTarget::Selection } else { CommandTarget::Copy },
                    text: current,
                    session: session.clone(),
                    expected_selection: replaceable.then(|| text.to_string()),
                    source: selection.source,
                    restore_cursor: None,
                });
            }
        }
        let kind = if replaceable { CommandTarget::Selection } else { CommandTarget::Copy };
        return Decision::Plan(CommandPlan {
            target: kind,
            text: text.to_string(),
            session: new_session(ctx.target, text, kind, ctx.now),
            expected_selection: (kind == CommandTarget::Selection).then(|| text.to_string()),
            source: selection.source,
            restore_cursor: None,
        });
    }

    // The selection couldn't be read at all: there may be one, so nothing is pasted (the answer is copied).
    if selection.inconclusive {
        return Decision::Plan(CommandPlan {
            target: CommandTarget::Copy,
            text: String::new(),
            session: new_session(ctx.target, "", CommandTarget::Copy, ctx.now),
            expected_selection: None,
            source: selection.source,
            restore_cursor: None,
        });
    }

    // Nothing selected: continue the latest of our last result and the last dictation, if it's still right before the
    // cursor and can be selected again.
    if !terminal {
        enum Candidate<'b> {
            Session(SharedSession, String),
            Dictation(&'b Entry),
        }
        let mut candidates: Vec<(Instant, Candidate)> = Vec::new();
        if let Some(session) = &ctx.session {
            let s = session.borrow();
            if s.can_follow_up(ctx.now) && s.pasted_last {
                if let Some(current) = &s.current {
                    candidates.push((s.last_used, Candidate::Session(session.clone(), current.clone())));
                }
            }
        }
        if let Some(entry) = ctx.last_dictation {
            if entry.was_pasted && ctx.now.duration_since(entry.date) < FOLLOW_UP {
                candidates.push((entry.date, Candidate::Dictation(entry)));
            }
        }
        candidates.sort_by_key(|c| std::cmp::Reverse(c.0));
        for (_, candidate) in candidates {
            match candidate {
                Candidate::Session(session, current) => {
                    let same = (ctx.check)(&session.borrow().target) == TargetCheck::Same;
                    if let Some(cursor) = same.then(|| (ctx.reselect)(&current)).flatten() {
                        return Decision::Plan(CommandPlan {
                            target: CommandTarget::Selection,
                            text: current.clone(),
                            session,
                            expected_selection: Some(current),
                            source: SelectionSource::Accessibility,
                            restore_cursor: Some(cursor),
                        });
                    }
                }
                Candidate::Dictation(entry) => {
                    let same = (ctx.check)(&entry.target) == TargetCheck::Same;
                    if let Some(cursor) = same.then(|| (ctx.reselect)(entry.shown())).flatten() {
                        return Decision::Plan(CommandPlan {
                            target: CommandTarget::LastDictation,
                            text: entry.shown().to_string(),
                            session: new_session(ctx.target, entry.shown(), CommandTarget::LastDictation, ctx.now),
                            expected_selection: Some(entry.shown().to_string()),
                            source: SelectionSource::Accessibility,
                            restore_cursor: Some(cursor),
                        });
                    }
                }
            }
        }
    }

    // Write: at the cursor in a text field, or onto the clipboard where there's none (or in a terminal).
    let no_field = terminal || (selection.knows_focus && !selection.editable);
    let kind = if no_field { CommandTarget::Copy } else { CommandTarget::Write };
    Decision::Plan(CommandPlan {
        target: kind,
        text: String::new(),
        session: new_session(ctx.target, "", kind, ctx.now),
        expected_selection: None,
        source: selection.source,
        restore_cursor: None,
    })
}

/// Whitespace runs to single spaces, for comparing a selection with our last result.
pub fn squeezed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::DictationMode;

    fn target(app: &str) -> PasteTarget {
        PasteTarget { pid: 1, app_id: Some(app.into()), app_name: app.into(), ..Default::default() }
    }

    fn selection(text: Option<&str>, editable: bool, knows_focus: bool) -> Selection {
        Selection {
            text: text.map(Into::into),
            source: SelectionSource::Accessibility,
            editable,
            knows_focus,
            inconclusive: false,
        }
    }

    fn plan(decision: Decision) -> CommandPlan {
        match decision {
            Decision::Plan(plan) => plan,
            Decision::Refuse(message) => panic!("refused: {message}"),
        }
    }

    fn run(
        selection: &Selection,
        target: &PasteTarget,
        session: Option<SharedSession>,
        last: Option<&Entry>,
    ) -> Decision {
        decide(Context {
            selection,
            target,
            session,
            last_dictation: last,
            check: &|_| TargetCheck::Same,
            reselect: &|_| Some(17),
            now: Instant::now(),
        })
    }

    // The planner cases from LogicSelfTest.swift, plus terminals and re-selecting the last dictation.
    #[test]
    fn edits_a_selection() {
        let editor = target("org.gnome.TextEditor");
        let p = plan(run(&selection(Some("hello world"), true, true), &editor, None, None));
        assert_eq!(p.target, CommandTarget::Selection);
        assert_eq!(p.chip(), "2 words selected");
        assert_eq!(p.expected_selection.as_deref(), Some("hello world"));
        assert_eq!(p.payload(), json!({"target": "selection", "original": "hello world"}));
    }

    #[test]
    fn read_only_and_terminals_copy() {
        let editor = target("org.gnome.TextEditor");
        assert_eq!(
            plan(run(&selection(Some("hello world"), false, true), &editor, None, None)).target,
            CommandTarget::Copy
        );
        let terminal = target("org.gnome.Ptyxis");
        assert_eq!(
            plan(run(&selection(Some("ls -la"), true, true), &terminal, None, None)).target,
            CommandTarget::Copy
        );
        assert_eq!(plan(run(&selection(None, true, false), &terminal, None, None)).target, CommandTarget::Copy);
    }

    #[test]
    fn refuses_long_selections() {
        let long = "x".repeat(MAX_CHARACTERS + 1);
        assert!(matches!(run(&selection(Some(&long), true, true), &target("a"), None, None), Decision::Refuse(_)));
    }

    #[test]
    fn unreadable_selection_is_copy_only() {
        let mut unreadable = selection(None, true, false);
        unreadable.source = SelectionSource::Copy;
        unreadable.inconclusive = true;
        let p = plan(run(&unreadable, &target("a"), None, None));
        assert_eq!(p.target, CommandTarget::Copy);
        assert_eq!(p.chip(), "Copy only · couldn't read the selection");
    }

    #[test]
    fn nothing_selected_writes() {
        assert_eq!(plan(run(&selection(None, true, false), &target("a"), None, None)).target, CommandTarget::Write);
        assert_eq!(plan(run(&selection(None, false, true), &target("a"), None, None)).target, CommandTarget::Copy);
    }

    #[test]
    fn follow_up_continues_the_session() {
        let editor = target("org.gnome.TextEditor");
        let session = new_session(&editor, "hey can u send it", CommandTarget::Selection, Instant::now());
        session.borrow_mut().instructions = vec!["make it formal".into()];
        session.borrow_mut().current = Some("Could you please send it?".into());
        let p =
            plan(run(&selection(Some("Could you  please send it?"), true, true), &editor, Some(session.clone()), None));
        assert!(p.is_follow_up() && Rc::ptr_eq(&p.session, &session));
        assert_eq!(p.chip(), "Follow-up · 5 words");
        assert_eq!(
            p.payload(),
            json!({"target": "selection", "original": "hey can u send it", "current": "Could you please send it?",
                   "turns": [{"instruction": "make it formal"}]})
        );
    }

    #[test]
    fn reselects_the_last_dictation() {
        let editor = target("org.gnome.TextEditor");
        let entry = Entry {
            date: Instant::now(),
            app_name: "Text Editor".into(),
            mode: DictationMode::Default,
            raw: "we have 12 users".into(),
            cleaned: Some("We have 12 users.".into()),
            guard_reason: None,
            target: editor.clone(),
            was_pasted: true,
            showing_raw: false,
        };
        let p = plan(run(&selection(None, true, true), &editor, None, Some(&entry)));
        assert_eq!(p.target, CommandTarget::LastDictation);
        assert_eq!(p.text, "We have 12 users.");
        assert_eq!(p.restore_cursor, Some(17));
        assert_eq!(p.chip(), "Last dictation · 4 words");
    }
}
