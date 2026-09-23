# Voice to Text

**Free, open-source dictation for macOS that uses the AI subscription you already have.**

Press a hotkey, speak, and press it again. Clean, well-formatted text appears in whatever app you're typing into.
Speech is transcribed **on your Mac** with [whisper.cpp](https://github.com/ggml-org/whisper.cpp). The polishing step
(removing filler words, fixing punctuation, formatting numbers, lists and email addresses) runs through your own
[Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI, so it uses your existing Claude subscription.
**No API keys, no word limits, and no extra subscription.**

<p align="center">
  <img src="docs/images/overlay-recording.png" width="420" alt="Recording: live waveform and timer">
  <img src="docs/images/overlay-polishing.png" width="420" alt="Polishing with Claude">
</p>

## Features

- **Works in any app:** Slack, Mail, VS Code, the terminal, the browser. Text is pasted at the cursor, and your clipboard is restored afterwards.
- **Local transcription:** Whisper large-v3-turbo runs on-device. Audio never leaves your Mac.
- **Smart cleanup:**
  - Removes "um", "uh" and false starts.
  - Applies self-corrections: "Friday, no wait, Thursday" becomes "Thursday".
  - Writes numbers properly: $15,400 · 15% · 2:30 PM · March 3, 2026.
  - Turns spoken addresses into real ones: "john dot doe at gmail dot com" becomes john.doe@gmail.com, and `process_order` stays `process_order`.
  - Formats lists and paragraphs.
  - Carries out spoken commands such as "new paragraph" and "comma".
- **App-aware modes:** casual for chat apps, proper paragraphs for email, exact identifiers for code and terminals, lists for notes.
- **Dictionary:** teach it names and terms, and fix words it keeps mishearing.
- **Floating indicator:** a live waveform while listening, then *Transcribing…* and *Polishing…*, and *✓ Pasted* when done.
- **Microphone picker and test**, with Bluetooth headsets handled. The built-in mic still transcribes more accurately.
- **Fallback:** if Claude is slow or unavailable, you still get the raw transcript. A dictation is never lost.

## Requirements

- macOS 13 or later on **Apple Silicon**.
- [Homebrew](https://brew.sh).
- The [Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI, installed and logged in. Check with `claude --version`, and log in by running `claude` once.
  You can also use Voice to Text **without** Claude: turn off **Clean Up with Claude** to get the raw Whisper transcript.
- Xcode Command Line Tools (`xcode-select --install`). The full Xcode isn't needed.

## Install

```bash
git clone https://github.com/mahfuzur/voice-to-text.git
cd voice-to-text

brew install sox whisper-cpp
./scripts/install.sh               # downloads the Whisper model (1.6 GB), sets up the config and the `dictate` command, runs a self-test
./scripts/setup-signing.sh         # one time: a local signing identity, so macOS keeps permissions across updates
./scripts/build-app.sh --install   # builds the menu-bar app into ~/Applications and launches it
```

On first launch, allow **Microphone** access, and turn on **Voice to Text** under
**System Settings → Privacy & Security → Accessibility** (needed to paste). The menu shows both permissions and has a
**Click to Fix** item for each.

## Usage

| Action | How |
|---|---|
| Start dictating | **⌃⌥Space** (you can change it in the menu under **Hotkey**) |
| Stop and paste | **⌃⌥Space** again |
| Cancel | **Esc** while recording |
| Copy the last result again | Menu → **Copy Last** |

If you use Bluetooth earbuds, wait for the start sound before speaking. They take about 1.5 s to switch into headset mode.

### Modes

The mode is chosen from the app you're typing into (**Auto**). You can also fix a mode in the menu under **Mode**.

| Mode | Apps | Style |
|---|---|---|
| Chat | Slack, Teams, Discord, WhatsApp, Messages | Casual; no period at the end of one-liners |
| Email | Mail, Outlook, Spark, Superhuman | Paragraphs; dictated greetings and sign-offs on their own lines |
| Code | VS Code, Cursor, Xcode, JetBrains, Terminal, iTerm2, Warp, Ghostty | Exact file names, identifiers and commands |
| Notes | Notes, Notion, Obsidian, Bear, Pages, Word | Lists and short paragraphs |
| Default | Everything else, including browsers | Balanced |
| Raw | Chosen manually | No AI cleanup |

Lists are pasted as rich text, so apps like Notes, Mail and Slack show real bullets. Code mode always pastes plain text.

### Dictionary

Open the menu and choose **Edit Dictionary…** (it edits `~/.config/voice-to-text/dictionary.txt`):

```
# One term per line: Whisper and Claude will spell it exactly like this
Kubernetes
PostgreSQL

# Replacements, applied after cleanup: heard => wanted
cloud code => Claude Code
```

### Command line

The app is built around the script `scripts/dictate.sh`, which also works on its own (installed as `dictate`):

```bash
dictate            # toggle: start recording / stop and paste
dictate cancel     # stop and discard
dictate selftest   # synthetic speech through the whole pipeline (no mic, no paste)
dictate file x.wav # transcribe and clean up an existing recording
```

To use a hotkey without the app, bind `~/.local/bin/dictate` to a keyboard shortcut in the Shortcuts app or with
[skhd](https://github.com/koekeishiya/skhd). Give Microphone and Accessibility permission to whichever app runs it.

## How it works

```
hotkey ─► record (AVAudioEngine, 16 kHz WAV)
       ─► transcribe on-device (whisper.cpp + a short style prompt and your vocabulary)
       ─► clean up (claude -p with prompts/system.md + a mode prompt; tools disabled, extended thinking off)
       ─► post-process (dictionary replacements, output filter, paragraph and email fixes)
       ─► paste at the cursor (Cmd+V, then your clipboard is restored)
```

- **Prompts** live in [`prompts/`](prompts/): the core rules are in `system.md`, and each mode has a file in `modes/`.
  The transcript is always treated as text to clean up, never as instructions: "write me a poem" comes back as that sentence, not as a poem.
- **Timing today:** about 2–3 s for Whisper and 4–6 s for Claude after you stop speaking.
  Making this faster is the next milestone ([roadmap](docs/ROADMAP.md)).

## Privacy

- **Audio** is recorded to a temporary file, transcribed on your Mac, and deleted straight away. It is never uploaded.
- **Transcript text** is sent to Claude through *your* Claude Code CLI, only when cleanup is on. This project has no servers,
  telemetry or analytics.
- **Logs:** `~/Library/Logs/voice-to-text/dictate.log` records timings and, by default, the raw and cleaned text, for
  troubleshooting. Set `LOG_TEXT=off` in `~/.config/voice-to-text/config.sh` to keep text out of the log.

## Configuration

`~/.config/voice-to-text/config.sh` holds the Whisper model, language, vocabulary, Claude model and timeout, and on/off switches.
All options are documented in [scripts/config.example.sh](scripts/config.example.sh). Most everyday settings are in the menu.

## Troubleshooting

| Problem | Fix |
|---|---|
| The pill says "Copied. Press ⌘V" | Accessibility isn't allowed. Menu → **Accessibility: Click to Fix**. If it's already switched on, remove Voice to Text from the list and add it again. |
| "No speech detected" every time | Microphone access is missing, or the wrong mic is selected. Try **Microphone ▸ Test Microphone…**. |
| Bluetooth mic says "No audio from …" | Pick the built-in mic under **Microphone**, or reconnect the earbuds. |
| "Pasted without cleanup" | Claude timed out or isn't logged in. Run `claude` in a terminal to check. |
| Something else | Check `~/Library/Logs/voice-to-text/dictate.log` and open an issue. |

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for the development setup and the quality-eval workflow,
and [docs/ROADMAP.md](docs/ROADMAP.md) for what's planned.

## Acknowledgements

- [whisper.cpp](https://github.com/ggml-org/whisper.cpp) and OpenAI's [Whisper](https://github.com/openai/whisper) models, for local speech recognition.
- [SoX](https://sourceforge.net/projects/sox/), for command-line recording.
- Ideas from [Wispr Flow](https://wisprflow.ai), [Superwhisper](https://superwhisper.com), [Typeless](https://www.typeless.com) and [VoiceInk](https://github.com/Beingpax/VoiceInk).

## License

[MIT](LICENSE). This project is not affiliated with or endorsed by Anthropic or OpenAI. "Claude" and "Claude Code" are
trademarks of Anthropic. Voice to Text calls the Claude Code CLI that you installed yourself; your use of it is subject to
[Anthropic's terms](https://www.anthropic.com/legal/consumer-terms).
