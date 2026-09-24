# Changelog

## 0.1.0 (2026-09-24)

First public version.

- **Install from a DMG:** the app bundles its own whisper.cpp and llama.cpp, so it runs on a Mac without Homebrew.
  A first-run setup handles the permissions, downloads the speech model (compressed large-v3-turbo by default, 574 MB) and
  S1-mini with progress and a checksum check, and finds Claude Code or installs it with Anthropic's installer.
- **Settings window:** General (hotkey recorder for any key combination, hold to talk, microphone, overlay, launch at login,
  permissions), Speech (model manager), Cleanup (Claude status, S1-mini, a Test button), Dictionary, Modes (your own
  app → mode choices) and About (update check, credits, licenses). The menu is shorter.
- A new menu-bar icon: waveform bars like the app icon, with a red dot while recording and processing. It follows the
  light or dark menu bar.
- An app icon, and a DMG window like other Mac apps: drag Voice to Text onto Applications, with an arrow between them.
  The artwork is drawn in code and documented in docs/ARTWORK.md.
- Releases: `scripts/release.sh` and a GitHub Actions workflow build a signed DMG from a version tag; a daily update check
  shows "Update Available" in the menu.
- Fixes: the recording length no longer needs `sox` (every dictation was skipped as too short without it), and a model
  server restarts when the chosen model or its binary changes.
- Release review fixes: the bundled helpers run from a copy in Application Support (in a downloaded DMG they were blocked
  by Gatekeeper even after Open Anyway), resumed downloads work, the hotkey recorder can't get stuck, and Claude Code is
  found in more install locations.

- Menu-bar app with a global hotkey, Esc to cancel, and a floating overlay (live waveform, transcribing and polishing states).
- On-device transcription with whisper.cpp (large-v3-turbo) and a style and vocabulary prompt.
- Cleanup through the user's own Claude Code CLI, with the transcript treated as data and a raw-text fallback.
- Offline cleanup with S1-mini by Superwhisper (llama.cpp): used automatically when Claude is offline or fails, or selected to stay fully on-device.
- About twice as fast: text is pasted about 3 s after a short dictation and 3.5–5.5 s after 15–20 s of speech (was 6–9 s).
  Whisper stays loaded in a local whisper-server, and the claude process starts while you speak.
- Hold to Talk hotkey mode, and a per-dictation timing line in the log.
- Formatting rules: numbers, times and dates, self-corrections, spoken symbols (email addresses, URLs, identifiers), spoken commands, lists, paragraphs.
- App-aware modes: Chat, Email, Code, Notes, Default, Raw.
- Personal dictionary (terms and replacements), and rich-text paste for lists.
- Microphone picker and test, with Bluetooth headsets handled.
- Clipboard restore, stable local code signing, and the formatting-quality eval harness.
