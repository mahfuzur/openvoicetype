//! The text side of `dictate.sh`: the dictionary, the vocabulary, the prompts and user messages, post-processing, the
//! meaning guard and small helpers. No I/O except reading the prompt and dictionary files. Held equal to the script by
//! `golden/text.json` (see `golden/make-text.sh`).

use crate::config::Config;

/// `load_dictionary`: plain lines are terms; `heard => wanted` lines are replacements, and `wanted` is also a term
/// (in file order, as the script builds `DICT_TERMS`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dictionary {
    pub terms: Vec<String>,
    /// (heard, wanted)
    pub replacements: Vec<(String, String)>,
}

impl Dictionary {
    pub fn parse(text: &str) -> Self {
        todo!("load_dictionary")
    }
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path).map(|text| Self::parse(&text)).unwrap_or_default()
    }
}

/// `trim`: bash's [[:space:]] trimming.
pub fn trim(text: &str) -> &str {
    todo!("trim")
}

/// `word_count`: `wc -w`.
pub fn word_count(text: &str) -> usize {
    todo!("word_count")
}

/// `vocabulary`: `VOCAB`, `VTT_VOCAB` and the dictionary's terms, trimmed, de-duplicated, joined with ", ".
pub fn vocabulary(config: &Config, dictionary: &Dictionary) -> String {
    todo!("vocabulary")
}

/// `whisper_prompt`: the style sample plus " Names and terms: <vocabulary>." (or just the vocabulary when the style
/// prompt is off).
pub fn whisper_prompt(config: &Config, dictionary: &Dictionary) -> String {
    todo!("whisper_prompt")
}

/// `HALLUCINATIONS`: Whisper's text on silence ("Thank you.", "[BLANK_AUDIO]"…) counts as no speech.
pub fn is_hallucination(text: &str) -> bool {
    todo!("HALLUCINATIONS")
}

/// `transcribe`'s clean-up of Whisper's output: lines joined (\r too), whitespace squeezed and trimmed, and a
/// hallucination becomes "".
pub fn clean_transcript(out: &str) -> String {
    todo!("transcribe's join and trim")
}

/// `system_prompt`: command.md for Command Mode, else the user's `PROMPT_FILE`, else system.md, else the fallback
/// prompt; then `\n\n` and `modes/<mode>.md` when it exists.
pub fn system_prompt(config: &Config) -> String {
    todo!("system_prompt")
}

/// `user_message` (a dictation): the context line, the vocabulary, the transcript. Command Mode: `command_message`.
pub fn user_message(config: &Config, dictionary: &Dictionary, raw: &str) -> String {
    todo!("user_message")
}

/// `command_message`: Command Mode's user message from `VTT_COMMAND_FILE`, with tags inside the data neutralized.
pub fn command_message(config: &Config, dictionary: &Dictionary, instruction: &str) -> String {
    todo!("command_message")
}

/// `command_source`: the text a command works on (the latest result for a follow-up, else the original).
pub fn command_source(config: &Config) -> String {
    todo!("command_source")
}

/// `post_process`: dictionary replacements, the output filter, paragraph splitting and the chat period rule; for
/// Command Mode only the dictionary, whitespace and the source-aware filters.
pub fn post_process(config: &Config, dictionary: &Dictionary, text: &str) -> String {
    todo!("post_process")
}

/// `meaning_guard`: None if the cleanup kept the meaning, else what went missing ("a number (12)", "a negation").
pub fn meaning_guard(raw: &str, clean: &str) -> Option<String> {
    todo!("meaning_guard")
}

/// `s1_control_line`.
pub fn s1_control_line(mode: &str) -> &'static str {
    todo!("s1_control_line")
}

/// `format_resets`: an epoch (seconds or ms) becomes "3:45 PM", with "Oct 4, " in front if not today (local time);
/// anything else is kept as it is.
pub fn format_resets(value: &str) -> String {
    todo!("format_resets")
}

/// `log_safe`: the guard's reason without the digits, unless text logging is on.
pub fn log_safe(reason: &str, log_text: bool) -> String {
    todo!("log_safe")
}
