# Changelog

## 0.1.0 (unreleased)

First public version.

- Menu-bar app with a global hotkey, Esc to cancel, and a floating overlay (live waveform, transcribing and polishing states).
- On-device transcription with whisper.cpp (large-v3-turbo) and a style and vocabulary prompt.
- Cleanup through the user's own Claude Code CLI, with the transcript treated as data and a raw-text fallback.
- Formatting rules: numbers, times and dates, self-corrections, spoken symbols (email addresses, URLs, identifiers), spoken commands, lists, paragraphs.
- App-aware modes: Chat, Email, Code, Notes, Default, Raw.
- Personal dictionary (terms and replacements), and rich-text paste for lists.
- Microphone picker and test, with Bluetooth headsets handled.
- Clipboard restore, stable local code signing, and the formatting-quality eval harness.
