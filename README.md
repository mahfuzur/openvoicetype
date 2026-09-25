<p align="center">
  <img src="docs/images/app-icon.png" width="128" alt="OpenVoiceType app icon: a waveform above two lines of text">
</p>

<h1 align="center">OpenVoiceType</h1>

<p align="center">
  <strong>Open-source dictation for macOS: on-device Whisper, polished by your own Claude Code.</strong>
</p>

<p align="center">
  <a href="https://github.com/mahfuzur/openvoicetype/releases/latest">Download for Mac</a> ·
  <a href="#install">Install</a> ·
  <a href="#how-well-it-works">How well it works</a> ·
  <a href="#privacy">Privacy</a> ·
  <a href="docs/GUIDE.md">Guide</a>
</p>

Press a hotkey, speak, and press it again. Clean, well-formatted text appears in the app you're typing into.
Speech is transcribed **on your Mac** with [whisper.cpp](https://github.com/ggml-org/whisper.cpp). The cleanup
(filler words, punctuation, numbers, lists, email addresses) runs through the
[Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI you're already signed in to. That uses your Claude plan's
normal usage limits, with no API key. You can also clean up with any OpenAI-compatible endpoint (Ollama, LM Studio,
OpenAI, Groq…) or fully offline with S1-mini.

> **Early software.** This is a young project with one maintainer, tested mainly on one Mac (M3 Pro). It isn't notarized
> by Apple yet, so the first launch needs **Open Anyway**. Bug reports are very welcome.

<!-- Demo GIF: record it with scripts/make-demo-gif.sh (see docs/ARTWORK.md), then uncomment:
<p align="center"><img src="docs/images/demo.gif" width="720" alt="Dictating into Notes: speak, then the polished text is pasted"></p>
-->

<p align="center">
  <img src="docs/images/overlay-recording.png" width="420" alt="Recording: live waveform and timer">
  <img src="docs/images/overlay-polishing.png" width="420" alt="Polishing with Claude">
</p>

## What it does

- **Any app:** Slack, Mail, VS Code, the terminal, the browser. The text goes where you started dictating: if you switch
  away meanwhile, it's copied instead, and it's never pasted into a password field. Your clipboard comes back afterwards.
- **Cleanup that keeps your meaning:**
  - Removes "um" and false starts, and applies self-corrections ("Friday, no wait, Thursday").
  - Writes $15,400 · 15% · 2:30 PM · john.doe@gmail.com properly, and formats lists and paragraphs.
  - If a cleanup ever drops a number or a "not", Whisper's own text is pasted instead.
  - **⌃⌥Z** swaps the last paste between the cleaned text and Whisper's text.
- **Command Mode (⌃⌥⇧Space):** select text, press it, and say how to change it: "make this shorter and more polite",
  "turn this into bullet points", "translate to Spanish", "it's T-O-N-I". The selection is replaced, and ⌘Z brings it back.
  - Follow up with "shorter still" or "go back to the original".
  - With nothing selected, it edits what you just dictated, or writes new text at the cursor ("write a two-line thank-you
    to the team").
  - For text you can't edit (a web page, a PDF, a terminal), the answer goes to the clipboard.
- **App-aware modes:** casual for chat, paragraphs for email, exact identifiers for code, lists for notes.
- **Dictionary** for names and terms, a **floating indicator** (listening, transcribing, polishing, pasted), a microphone
  picker, and clear messages when something goes wrong ("Claude limit reached · resets 3:45 PM").
- **Nothing lost:** if cleanup fails, S1-mini cleans up on your Mac instead, and failing that you get Whisper's text.

## Which install is for me?

| You want | Do this | You need |
|---|---|---|
| **The app** (almost everyone) | [Download the DMG](#install) | Nothing else: no Homebrew, no Terminal |
| To work on the code | [Build from source](#build-from-source) | Xcode Command Line Tools, `cmake` |
| Only the `dictate` terminal command | `./scripts/install.sh` ([guide](docs/GUIDE.md#command-line)) | `brew install sox whisper-cpp` |

## Install

Requires macOS 13.3 or later on **Apple Silicon**.

1. Download `OpenVoiceType-<version>.dmg` from the [latest release](https://github.com/mahfuzur/openvoicetype/releases/latest),
   open it, and drag **OpenVoiceType** to **Applications**.
2. Open it. The first time, macOS says it can't check the app, because it isn't notarized by Apple. Click **Done**, open
   **System Settings → Privacy & Security**, and click **Open Anyway** next to "OpenVoiceType was blocked". Once only.
3. The setup window walks you through the rest:
   - the **Microphone** and **Accessibility** permissions (Accessibility is for pasting);
   - a speech model (the compressed one, 574 MB, is recommended);
   - cleanup: it finds Claude Code, or installs it with Anthropic's installer and signs you in, in Terminal, where you can
     see what runs. It also offers S1-mini (484 MB), so dictation works right away, even offline;
   - **Try it:** a box to dictate into.

The models come from Hugging Face ([whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp),
[S1-mini](https://huggingface.co/superwhisper/s1-mini-GGUF)), pinned and checked against their SHA-256.

### Build from source

```bash
git clone https://github.com/mahfuzur/openvoicetype.git && cd openvoicetype
xcode-select --install             # if you don't have the Command Line Tools (full Xcode isn't needed)
brew install cmake
./scripts/setup-signing.sh         # once: a local signing identity, so macOS keeps permissions across rebuilds
./scripts/build-app.sh --install   # builds whisper.cpp and llama.cpp into the app (about 3 minutes the first time)
```

## Using it

**⌃⌥Space** starts and stops (or hold it while you speak), **Esc** cancels, **⌃⌥⇧Space** is Command Mode, and **⌃⌥Z**
swaps the last paste. Everything else is in **Settings** (⌘, from the menu-bar icon). Modes, the dictionary, cleanup engines, the command line and
troubleshooting are in the **[Guide](docs/GUIDE.md)**.

## How well it works

Measured on 2026-09-25 on an M3 Pro (18 GB) with [`evals/run.py`](evals/run.py), on 25 fixed test cases. Seventeen are
formatting cases (lists, numbers, corrections, email addresses, code). Eight are safety cases: a number or a "not" must
survive, and a dictated instruction ("ignore previous instructions…", "you are now a pirate…") must come back as text, not
be carried out.

| Cleanup engine | Formatting | Safety | Median time per cleanup |
|---|---|---|---|
| Claude Haiku (default) | 17/17 (16/17 in some runs) | 8/8 | 3.4 s with the CLI's startup; about 1.2 s pre-started, as in the app |
| S1-mini (on this Mac, offline) | 8/17 | 7/8 | 0.7 s |
| Ollama llama3.2, 3B (on this Mac) | 9/17 | 3/8 | 1.8 s |

**Command Mode** has its own 20 cases (`evals/run.py --command`): edits, targeted changes, tone, lists, translation, a
spelled-out name, Write, a copy-only summary, follow-ups, and instructions hidden in the selection. Claude Haiku passes
19–20 of them (the flaky one: "shorter and more polite" sometimes comes out longer). A command takes about 3–4 s one-shot.

**Compared with a plain `claude -p` call** (the way other apps use Claude Code: one call per dictation, no isolation,
extended thinking on; `evals/run.py --cold`): a median of **10.9 s** per cleanup, and 22/25. OpenVoiceType's isolated call
takes 3.7 s one-shot (25/25), and about 1.2 s when it's pre-started while you speak, as in the app.

**From stop to text:** a median of **2.3 s** end to end on short sentences (Whisper about 1.1 s, Claude about 1.2 s;
`evals/run.py --e2e --timing --jobs 1`). In real use, the app logs 2.7 s for 8 s of speech and 3.5–5.7 s for 17–22 s.

**Caveats:**
- The test speech is synthesized with macOS `say`, not recorded from real voices.
- 25 cases is a small set, and they come from one maintainer's dictations.
- One formatting case is flaky on Haiku: it doesn't always fix a misheard "leads to" into "needs to".
- The safety rules are a prompt plus a check for dropped numbers and negations. They are **mitigations, not guarantees**.
- Small local models are much weaker. For an API engine, pick a capable model.

## Privacy

- **Audio** is transcribed on your Mac and deleted straight away. It never leaves the Mac.
- **Transcript text** goes only to the cleanup engine you pick:
  - your own Claude Code (Anthropic's terms and your plan's limits apply: [docs/TERMS.md](docs/TERMS.md)). Command Mode
    sends the selected text and your instruction, only when you press its key;
  - an API endpoint you configure (a local Ollama or LM Studio keeps it on your Mac);
  - or nowhere, with S1-mini or cleanup off.
- **No servers, telemetry or analytics.** The app never reads or stores your Claude login, keeps API keys in your Keychain,
  and ignores an exported `ANTHROPIC_API_KEY`, so nothing quietly bills the API.
- **Logs** keep timings and outcomes, not your words (unless you turn that on for debugging). They're private to you and
  capped at about 2 MB. **Recent Dictations** lives in memory only.
- **Update check:** once a day the app asks GitHub for the latest release. It can be turned off in Settings → About.

## Contributing

Contributions are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md) for the setup and the eval workflow, and the
[roadmap](docs/ROADMAP.md) for what's next (on-screen context, snippets, Apple's speech engine).

## Acknowledgements

- [whisper.cpp](https://github.com/ggml-org/whisper.cpp) and OpenAI's [Whisper](https://github.com/openai/whisper) models; S1-mini
  by Superwhisper ([Apache 2.0](https://huggingface.co/superwhisper/s1-mini-GGUF)) with [llama.cpp](https://github.com/ggml-org/llama.cpp);
  [SoX](https://sourceforge.net/projects/sox/) for the command-line tool.
- Original artwork drawn in code ([docs/ARTWORK.md](docs/ARTWORK.md)).
- Ideas from [Wispr Flow](https://wisprflow.ai), [Superwhisper](https://superwhisper.com), [Typeless](https://www.typeless.com)
  and [VoiceInk](https://github.com/Beingpax/VoiceInk).

## License

[MIT](LICENSE). Not affiliated with or endorsed by Anthropic or OpenAI. "Claude" and "Claude Code" are trademarks of
Anthropic. OpenVoiceType runs the Claude Code CLI you installed yourself; your use of it is subject to
[Anthropic's terms](https://www.anthropic.com/legal/consumer-terms), including your plan's usage limits
([docs/TERMS.md](docs/TERMS.md)). It was called **Voice to Text** up to version 0.1.1.
