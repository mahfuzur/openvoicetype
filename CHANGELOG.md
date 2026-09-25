# Changelog

## 0.2.0 (2026-09-25)

**Voice to Text is now OpenVoiceType.** "Voice to Text" was too generic to find.

- **Coming from 0.1.x:** macOS sees the renamed app as a new app, so it asks for **Microphone** and **Accessibility**
  once more. Your settings are copied over on the first launch, and it offers to move the old Voice to Text app to the
  Trash (quitting it first, so the two don't fight over the hotkey). The dictionary, config and logs stay where they were.
  If the old app opened at login, turn that on again in Settings → General.
- New bundle ID `io.github.mahfuzur.openvoicetype`, `OpenVoiceType.app`, `OpenVoiceType-<version>.dmg`, and the repo moves
  to `mahfuzur/openvoicetype` (the old links redirect).
- [docs/TERMS.md](docs/TERMS.md): how the app uses your own Claude Code, and what Anthropic's terms say about it. Linked from
  the README, Settings → Cleanup and About. The wording is more careful: "works with your own Claude Code", and your
  plan's usage limits apply.
- A security policy, feature-request and pull-request templates, and a script for the README demo GIF.
- `install.sh` no longer links another app's Whisper model; it downloads its own.

## 0.1.1 (2026-09-24)

- The Settings window opens at the right size (the first pane was clipped until you switched tabs).
- **Reset…** for Accessibility, in Settings → General and in setup: when Voice to Text is switched on in the list but
  pasting still doesn't work. The switch belonged to an older copy with a different signature, and turning it off and on
  didn't help; the button removes the old entry and asks again.
- Maintainers: `setup-signing.sh` imports the release certificate, so local builds are signed like releases and keep the
  same permission grants.

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
