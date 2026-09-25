# Roadmap: OpenVoiceType

*Called Voice to Text up to v0.1.1 (renamed in M5).*

**Goal:** a free, open-source macOS dictation app. Press a hotkey, speak, and well-formatted text appears in
the focused app. Whisper runs locally, and cleanup is done by **the AI subscription the user already pays for**,
through its CLI, with no API keys. Local models and API keys are options too.

## Status

**Done (2026-09-23):**
- `scripts/dictate.sh`: the pipeline. Records with sox, transcribes with whisper.cpp large-v3-turbo, cleans up with `claude -p`, then pastes.
  Raw text is used if Claude fails, short utterances skip Claude, dictated requests come back as text rather than answers,
  and every run is logged.
- `app/`: a menu-bar app. Carbon hotkey (⌃⌥Space, changeable), Esc to cancel, and menu settings
  (Claude on/off, Haiku/Sonnet, auto-paste, launch at login). It pastes itself and restores the whole clipboard. It runs `dictate.sh` for the pipeline.
- It works in daily use. Real timings then: whisper 2–4 s, claude 4–8 s, **total 6–10 s** (2.7–5.7 s since M3).

**Done: M1 overlay (2026-09-23):** recording moved into Swift (`AVAudioEngine`, with live levels). The script is split into
`transcribe` and `refine` stages. There's a floating pill with a live waveform and timer, then Transcribing, Polishing, and Pasted,
No speech or an error. Menu options: overlay on/off, overlay position, sounds. `--overlay-snapshots` and `--overlay-demo` are available for UI work.

**Done: mic handling (2026-09-23):** Microphone menu (system default or a specific device, with a Bluetooth tip).
Test Microphone shows a 4 s live level meter and a verdict. Bluetooth-safe recorder: async start with retries, a "Connecting to …"
pill, a no-audio watchdog, and configuration changes handled without looping. Stable local code signing (`setup-signing.sh`)
means permission grants survive rebuilds.

**Done: M2 formatting quality (2026-09-23):** prompt v2 (`prompts/`), app-aware modes, dictionary, Whisper style prompt,
post-processing, rich paste, and the eval harness (`evals/`). Eval: 30% → **100%** on Haiku, with the median cleanup down from 9.3 s to **4.9 s**
(extended thinking is now off). Details: [plans/M2-formatting-quality.md](plans/M2-formatting-quality.md).

**Found in real use (from `dictate.log`):**

| Problem | Example |
|---|---|
| ~~No visible sign that it's recording or processing~~ | Fixed in M1 |
| ~~Lists come out inline~~ (fixed in M2) | "make a grocery list 1 kg banana 1 kg apple…" became "Make a grocery list: 1 kg banana, 1 kg apple, …" |
| ~~Long dictations come out as one paragraph~~ (fixed in M2) | An 81 s dictation produced a single block of text |
| ~~Names are misheard~~ (dictionary, M2) | "Claude" was transcribed as "cloud" |
| Too slow compared with other tools (improved in M3) | Was 6–10 s; now about 3–5 s with Claude and 1.5 s with S1-mini, versus about 1–3 s for Wispr Flow and VoiceInk |

## Roadmap

| # | Milestone | Effort | Status | Why |
|---|---|---|---|---|
| M1 | Floating overlay with animations | 1–2 days | ✅ Done (2026-09-23) | You can see when it's listening and working |
| M2 | Formatting quality | 2 days | ✅ Done (2026-09-23) | Lists, paragraphs, spoken commands, app-aware style |
| M2.5 | Offline cleanup with S1-mini | 1–2 days | ✅ Done (2026-09-24); a real internet-off test in the app is still to do (see [Open items](#open-items)) | Works with no internet; a fully on-device option |
| M3 | Native pipeline and speed | 3–4 days | ✅ Done (2026-09-24): about 2× faster; ≤ 3 s for short dictations | Around 2–3 s total, needed before a public release |
| M4 | Settings window, first-run setup and DMG | 7–9 days | ✅ Released as v0.1.0 (2026-09-24); a test on a second Mac is still to do ([plan](plans/M4-app-and-install.md)) | Anyone can install it from a DMG with no Homebrew or Terminal, and set it up in a real window |
| M5 | Open-source release | 2–3 days | ✅ Released as v0.2.0 (2026-09-25): renamed to **OpenVoiceType**, terms check, community files, repo renamed. The demo GIF is deferred ([plan](plans/M5-open-source-release.md)) | Name, terms check, demo, the first tagged DMG |
| M5.4 | Trust release (v0.3.0) | 7–9 days | ◐ Built (2026-09-25): to release after the manual paste checks ([plan](plans/M5.4-trust-release.md)) | From an outside review: private logs, safe pasting, a guard for changed numbers and "not", an isolated Claude call with clear errors, and an OpenAI-compatible provider so cleanup doesn't depend on Claude alone |
| M5.5 | Command Mode, on-screen context and snippets | 11–14 days | ☐ Planned (2026-09-25) ([plan](plans/M5.5-command-mode-context-snippets.md), [research](research/2026-09-25-command-mode-and-cli.md)) | Closes the biggest gap: editing the selection by voice. v0.4.0 = Command Mode; v0.5.0 = context, snippets and Apple's on-device speech engine (macOS 26) |
| M6 | More providers | 3–4 days | Not started | Turns it into a platform: pick Codex, Gemini, Ollama or an API as well as Claude and S1-mini |

M3 matters most for adoption: people don't keep using a slow dictation tool.

---

## M1: Floating overlay (recording and processing indicator) ✅ Done

A small pill-shaped overlay at the bottom-centre of the screen, like Wispr Flow's, shown above every app and never taking focus.

| State | Visual |
|---|---|
| Recording | Red dot, **live waveform bars that move with your voice**, and an elapsed timer (0:07) |
| Transcribing | Bars turn into a shimmer, with "Transcribing…" |
| Polishing | A shimmer with a sparkle icon and "Polishing…" (skipped when cleanup is off) |
| Done | A green check, then the pill fades out after 0.6 s |
| Nothing heard / error | The pill shakes, shows "No speech detected" or the error, then fades |

Technical:
- An `NSPanel` with `.nonactivatingPanel` and `.borderless`, `level = .statusBar`, `ignoresMouseEvents`, and
  `collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]`. It shows on the screen that has the mouse.
  Drawn with SwiftUI hosted in `NSHostingView`, with spring animations for appear, disappear and state changes.
- **Live audio level:** move recording from `rec` into Swift (`AVAudioEngine` input tap, converted to 16 kHz mono
  Int16 and written with `AVAudioFile`). The tap gives the RMS level about 30 times a second for the waveform. This is also the first step of M3.
- **Real stage changes:** split the script into `dictate.sh transcribe <wav>` and `dictate.sh refine` (which reads stdin),
  so the app knows when transcription ends and polishing starts.
- Menu options: show overlay on/off, and position (bottom or top).
- Keep the start and stop sounds, with a toggle.

**Done when:** a glance at the screen always tells you whether it's listening, working, or finished.

## M2: Formatting quality ✅ Done

> **Detailed plan and task tracking:** [plans/M2-formatting-quality.md](plans/M2-formatting-quality.md)

### 2.1 Prompt v2

Rules to add to the system prompt:
- **Lists:** when the speaker lists items ("make a list…", "first… second…", "a, b and c" after "list"),
  output a bullet or numbered list, one item per line.
- **Paragraphs:** start a new paragraph when the topic changes. Never output more than about 4 sentences without a break.
- **Spoken commands:** "new line", "new paragraph", "bullet point", "numbered list", "comma" and "period" are turned into formatting.
  **Backtrack:** "scratch that", "actually, no" and "I mean" drop or replace what came just before.
- **Numbers and units:** 1 kg, 5 pm, $20, 3.5%, dates.
- **Few-shot examples** taken from real log entries (the grocery list, the long feedback paragraph).

### 2.2 App-aware modes

The app sends `<context app="Slack" window="…">` along with the transcript. The mode is chosen from the frontmost app's bundle ID:

| Mode | Apps | Style |
|---|---|---|
| Chat | Slack, Teams, WhatsApp, Discord, Messages | Casual, short, no trailing period on one-liners |
| Email | Mail, Outlook, Gmail in a browser | Full sentences and paragraphs |
| Code | VS Code, Xcode, JetBrains, Terminal, iTerm | Keeps identifiers and file names exact, no Markdown |
| Notes/Docs | Notes, Notion, Obsidian, Google Docs | Markdown-style lists and headings |
| Default | Everything else | As in 2.1 |

Users can edit the prompt for each mode and add their own mappings (M4 settings).

### 2.3 Rich paste

If the output has lists, put both **HTML** and plain text on the clipboard. Notes, Mail, Google Docs and Slack then show
real bullets, and plain-text apps get `- item` / `1. item`.

### 2.4 Personal dictionary

- **Replacements** applied after Whisper, e.g. `cloud code → Claude Code` and `cloud → Claude` in AI contexts.
- **Vocabulary** passed to Whisper as its prompt (it already accepts `VOCAB`) and listed in the Claude prompt.

### 2.5 Eval harness

`evals/cases/*.json` holds raw transcripts, the expected properties (a list, paragraphs, a spelling) and the mode.
`dictate.sh eval` runs the refine step on every case and prints pass/fail and a diff. Use it to tune the prompt,
compare Haiku with Sonnet, and catch regressions. The first cases come from the log.

**Done when:** the grocery-list, long-feedback and "Claude" cases all pass the eval.

## M2.5: Offline cleanup with S1-mini

[S1-mini](https://huggingface.co/superwhisper/s1-mini-GGUF) by Superwhisper is a 0.6B model trained only to clean up ASR
transcripts: fillers, self-corrections, punctuation, numbers, dates and emails. It's English only, `s1-mini-q4_k_m.gguf` is 484 MB,
and the license is Apache 2.0 plus a naming clause (credit it as "S1-mini by Superwhisper" and ship its LICENSE and NOTICE).
Whisper is already local, so with S1-mini the whole pipeline runs on the Mac.

**Behaviour:**

| Cleanup setting | What happens |
|---|---|
| Claude (default) | Claude cleans up. If the Mac is offline, or Claude fails (not logged in, rate limit, error, empty output, timeout), S1-mini cleans up instead. If S1-mini also fails, the raw Whisper text is pasted. |
| S1-mini | Always S1-mini, fully offline. Raw text if it fails. |
| Off / raw mode | No cleanup, as today. |

**Runtime:** llama.cpp, the runtime the S1-mini authors document. The app bundles its own static `llama-server` (M4); the
CLI uses Homebrew's (`brew install llama.cpp`). Ollama was
considered: same speed (it's built on llama.cpp), but S1-mini isn't in its library, it needs a custom Modelfile to get the
prompt format right, and every user would have to install a separate app.

- `llama-server -m <model> --jinja --chat-template-kwargs '{"enable_thinking":false}' --temp 0` on `127.0.0.1`.
  Requests go to its OpenAI-style `/v1/chat/completions` endpoint with `curl`.
- **Memory (about 1 GB measured: the 484 MB model plus a 4K context):** kept loaded only while S1-mini is the selected cleanup. For a fallback it's started on demand
  (about 1–2 s extra that time) and stopped after a few idle minutes.
- The model path comes from config (`S1_MODEL`, default `~/.local/share/s1-mini/s1-mini-q4_k_m.gguf`). `install.sh`
  installs llama.cpp and downloads the model.

**Prompt:** S1-mini doesn't follow instructions. It takes its own fixed system prompt, then a control line and the transcript:
`[Styling: casual|semi-casual|semi-formal|formal] [Structure: prose|lists] [Context: general|email]`. Mode mapping, to be tuned
with the eval:

| Mode | Control line |
|---|---|
| default | semi-formal, lists, general (it only makes a list for 3+ items) |
| chat | semi-formal, prose, general (semi-casual lower-cased sentence starts) |
| email | semi-formal, prose, email |
| notes | semi-formal, lists, general |
| code | Skip S1-mini: raw text + `post_process` (S1-mini has no code style) |

It takes no vocabulary, so names rely on the Whisper `--prompt` and the dictionary replacements in `post_process`, which still
runs on S1-mini output.

**Offline check:** `dictate.sh` skips Claude when the Mac has no default route (`route -n get default`, instant) or a TCP
connect to `api.anthropic.com:443` fails (`nc -z -G 1`, capped at 2 s with DNS; about 20 ms when online). The app and the CLI
both go straight to S1-mini instead of waiting for Claude to time out. The connect test is skipped behind a proxy, and
`ONLINE_CHECK=off` turns it off. `VTT_OFFLINE=on` forces offline for testing.

**Work:**
- [x] `dictate.sh`: `refine_s1()`, `s1-server` commands with an idle watchdog, the fallback chain, and logging (`refine=s1` / `refine=s1-fallback`)
- [x] `config.example.sh` and `install.sh`: `CLEANUP`, `S1_*` settings, `brew install llama.cpp`, model download
- [x] App: Cleanup Model menu (Claude Haiku / Claude Sonnet / S1-mini offline, plus the fallback toggle), llama-server kept loaded while S1-mini is selected, "Pasted · cleaned offline"
- [x] Overlay: "Polishing offline" when S1-mini is selected
- [x] `evals/run.py --cleanup s1`: baseline recorded (below)
- [x] README: offline mode, privacy note, "S1-mini by Superwhisper" credit; CLAUDE.md pipeline notes
- [ ] Real test: internet off, dictate in the app (first try on 2026-09-24 didn't exercise S1-mini, see the note below)
- [x] Detect "connected but no internet" quickly (see the note below)

**Eval baseline (2026-09-24):** S1-mini **10/20** (median 1.1 s per case in the parallel eval, about 0.1–0.3 s per call when warm)
vs Haiku **20/20** (3.8 s). Two failures are code-mode cases S1-mini skips by design. The rest: no vocabulary ("Cloud Code"),
no context-based fixes of Whisper mishearings ("witness day"), spoken email digits, and one heavy self-correction where it
dropped items. Good enough as a fallback; Claude stays the default for quality.

> **Note: first real offline test (2026-09-24).** The internet was cut while the Mac stayed on the router's Wi-Fi, and both
> dictations went into VS Code. Every dictation still pasted, but S1-mini never did the cleanup:
>
> - **The offline check missed it.** `route -n get default` only sees whether there's a network, and the router was still
>   there, so Claude waited the full 15 s timeout, then the raw text was pasted (code mode doesn't fall back to S1-mini).
>   This "connected but no internet" case is the common one in practice (ISP down, captive portal, dead hotspot).
>   **Fixed:** before cleanup, a TCP connect to `api.anthropic.com:443` with a 1 s limit (`nc -z -G 1`). In tests, an
>   unreachable internet now falls back to S1-mini in 0.5–1.3 s end to end, and the check costs about 20 ms when online.
> - **Code mode never uses S1-mini, by design** (it has no code style), so dictating into VS Code, Xcode or a terminal
>   always gives the raw text offline. Test in Notes, Slack or Mail. The Cleanup Model menu now says so.

**Done when:** with Wi-Fi off, a dictation is cleaned up by S1-mini and pasted, and the self-test passes with S1-mini selected.

## M3: Native pipeline and speed

> **Detailed plan and task tracking:** [plans/M3-speed.md](plans/M3-speed.md)

Target: **≤ 3 s** from stopping to pasted text, for 15 s of speech (**≤ 1.5 s** with S1-mini). Measured on 2026-09-24:

| Stage | Now | Change | Measured in the spike |
|---|---|---|---|
| Recording | Swift `AVAudioEngine` (M1) | No change | |
| Whisper | 1.6–2.0 s (loads 1.6 GB on every call) | `whisper-server` kept loaded, started on hotkey press, stopped when idle | 0.8–0.9 s |
| Claude | 3.4–4.1 s (CLI starts every time) | A `claude -p --input-format stream-json` process **started when recording starts** and used for one dictation only | about 1.0 s |
| Paste | < 0.3 s | No change | |

- A persistent multi-dictation Claude session was just as fast, but kept every earlier transcript in its history, so it was rejected.
- **Push-to-talk** (Carbon key release) alongside toggle mode, plus an `APP TIMING` log line per dictation.
- The bash pipeline stays: both wins come from keeping processes warm. The Swift `Transcriber`/`Refiner` protocols weren't needed in M3 or M4;
  a `Refiner` protocol comes with M6.

## M4: Settings window, first-run setup and DMG

> **Detailed plan and task tracking:** [plans/M4-app-and-install.md](plans/M4-app-and-install.md)

Rescoped on 2026-09-24: a self-contained app (Whisper and llama.cpp built into the bundle, models downloaded during setup),
a first-run setup that installs or finds the Claude CLI and uses S1-mini meanwhile, a Settings window, and a DMG release.
Providers other than Claude and S1-mini move to M6. Modes prompt editing and History are later.

The original idea for the settings window (the plan has the final list):
- **General:** hotkey recorder (any combination), toggle or push-to-talk, overlay, sounds, launch at login.
- **Transcription:** model manager that downloads tiny, base, small or large-v3-turbo from Hugging Face with progress,
  so there's no dependency on Superwhisper's folder. Also language and vocabulary.
- **AI Cleanup:** provider, model, timeout, and a **Test** button that runs a sample and shows the result and latency.
- **Modes:** edit each mode's prompt and its app mapping.
- **Dictionary:** replacements and vocabulary.
- **History:** the last N dictations (raw and cleaned). Copy or re-run one with another mode.
- **Onboarding:** a first-run wizard for the Microphone and Accessibility permissions, model download, provider choice and a test dictation.

## M6: More providers ("platform")

**Providers** (a `Refiner` protocol with one implementation each):

| Provider | How it's called | Auth | Notes |
|---|---|---|---|
| Claude Code CLI | `claude -p` | The user's Claude subscription | Current implementation |
| S1-mini | Local `llama-server` | None | M2.5; the offline fallback |
| OpenAI Codex CLI | `codex app-server` (pre-started; `codex exec` one-shot) | The user's ChatGPT plan | **The first M6 provider**: OpenAI documents using it in other apps. Flags in the [research](research/2026-09-25-command-mode-and-cli.md) §4 |
| GitHub Copilot CLI | `copilot -p` | The user's Copilot plan | Documented for third-party tools; inputs are used for training unless the user opts out |
| Gemini CLI | `gemini -p` | **API key only** | Google ended personal-account login on 2026-06-18, and Antigravity's terms forbid third-party tools |
| Ollama / LM Studio | Local HTTP | None | Fully offline; fast with small models |
| OpenAI-compatible API | HTTPS | API key stored in Keychain | OpenAI, Anthropic API, OpenRouter, Groq |
| None | Raw Whisper text | None | Fastest option |

Installed CLIs are detected automatically by resolving the user's login-shell `PATH` (`zsh -lc 'command -v claude'`).

## M5: Open-source release ✅ Done

> **Detailed plan and task tracking:** [plans/M5-open-source-release.md](plans/M5-open-source-release.md).
> Decided on 2026-09-24: the new name is **OpenVoiceType**; Claude's terms are disclosed in `docs/TERMS.md` and the wording is softer.
> **Released as v0.2.0 on 2026-09-25.** The demo GIF is deferred (see [Open items](#open-items)).

- **Name and IDs:** choose a unique name ("Voice to Text" is too generic to find) and check GitHub, the App Store and trademarks.
  Use the bundle ID `io.github.<user>.<name>`, and remove personal paths, e-mail addresses and the Superwhisper model path.
- **License:** MIT (my recommendation, the most permissive) or GPL-3 (like VoiceInk). Don't copy code from GPL projects into an MIT repo.
- **Toolchain:** ~~install Xcode locally~~. Settled in M4: Command Line Tools are enough, locally and in CI. GitHub Actions on
  a macOS runner builds, tests and attaches the DMG to each release.
- **Distribution:** the DMG and release workflow are built in M4. A **signed and notarized** DMG needs the Apple Developer
  Program ($99 a year); without it, users click Open Anyway once, and a self-signed release certificate keeps permission grants
  across updates (see the M4 plan).
- **Docs:** a README with a demo GIF, a privacy section (what leaves the machine: only the transcript text, and only to the
  provider the user picks), CONTRIBUTING, issue templates and a CHANGELOG.
- **Terms check:** before advertising "use your Claude/ChatGPT subscription", check each provider's terms for scripted
  use of its CLI. Only ever call the user's own installed CLI, and never read or reuse its login tokens.

## Risks

| Risk | Mitigation |
|---|---|
| A crowded field (VoiceInk, OpenWhispr, Handy, local-whisper) | Lead with the difference: using the AI subscription you already have through its CLI, formatting quality, and app-aware modes |
| Subscription CLI use falls in a grey area of provider terms | Check the terms (M5), and always offer Ollama and API-key options |
| CLI flags change between versions | One `Refiner` per provider, a Test button, and a version check with a clear error |
| Latency stays high with CLI providers | Persistent sessions, a pass-through for short utterances, and Ollama as a fast offline option |
| Permission grants lost on rebuild (ad-hoc signing) | Developer ID signing before release; a re-grant helper in onboarding |
| The transcript is read as an instruction | Transcript tags, a strict system prompt, tools disabled (already done), plus eval cases |

## M5.5: Command Mode, on-screen context and snippets

> **Detailed plan and task tracking:** [plans/M5.5-command-mode-context-snippets.md](plans/M5.5-command-mode-context-snippets.md).
> Deep research (2026-09-25): [research/2026-09-25-command-mode-and-cli.md](research/2026-09-25-command-mode-and-cli.md).

- **Our selling point, re-checked:** at least 10 dictation apps now use the user's own CLI (VoiceInk since April 2026), but all of
  them start it cold for each dictation. Our lead is the pre-started Claude and its isolation, so v0.3.0 hardens it first
  (`--safe-mode`, clear "limit reached" messages, a guard against an exported API key, safer pasting) and publishes the numbers.
- **Command Mode:** its own hotkey (⌃⌥⇧Space). With text selected, speak an instruction ("make this more polite") and Claude
  rewrites the selection in place, with ⌘Z, follow-ups ("shorter still") and Restore Original. With nothing selected, it edits
  the last dictation or writes new text at the cursor. Read-only text and terminals get the answer on the clipboard. Every
  failure says what happened. Claude only.
- **On-screen context:** opt-in. The window title and the text around the cursor, read through Accessibility, as spelling hints.
  No screenshots.
- **Snippets:** a spoken trigger inserts saved text, alone (no Claude call) or inside a sentence.
- **Small additions:** website modes from the browser's window title, smart spacing, and "send it" in chat apps.
- **Apple speech engine (macOS 26):** Apple's on-device SpeechAnalyzer as an alternative to Whisper, for Macs with little memory.
  Audio still stays on the Mac. Built behind a compiler check, so older SDKs keep building. Claude can't do speech
  recognition (it takes no audio).
- **Not planned:** a custom speech recognizer or fine-tuned LLM (these need GPU servers), learning a personal writing style,
  and a dictionary that learns from your edits (reading text fields back after pasting is fragile).

## Decisions

1. ~~Overlay style~~: a bottom-centre pill (done in M1).
2. ~~Name and license~~: **MIT**. The name was **Voice to Text** (`io.github.mahfuzur.voicetotext`) until M5 renamed it to
   **OpenVoiceType** (`io.github.mahfuzur.openvoicetype`) on 2026-09-24: the old name was too generic to find.
3. **Apple Developer account** ($99 a year) for signed and notarized releases: **not for now** (decided 2026-09-25). Releases are
   signed with the self-signed release certificate, and users click Open Anyway once. `release.sh` notarizes as soon as the
   credentials exist.
4. ~~Offline cleanup~~: **S1-mini** through llama.cpp. Claude stays the default; S1-mini is the automatic fallback and a selectable option (2026-09-24).

## Open items

What's left from finished milestones, and what was moved to later (checked against the code and `dictate.log` on 2026-09-25):

| # | Item | From | Notes |
|---|---|---|---|
| 1 | Real internet-off test in the app | M2.5 | Its "Done when" hasn't been met yet: none of the 61 app dictations in the log used the offline fallback (`APP RESULT … offlineFallback=true`). Test in Notes, Slack or Mail, not in code mode |
| 2 | Install the published DMG on a second Mac without Homebrew or Claude | M4.11, M5 §7.7 | Also finishes spikes S2 and S3 (Open Anyway on a real download, an update keeping the permissions), and runs the Claude Install and Sign In buttons (M4.7) and first-run setup (M4.8) on a clean Mac |
| 3 | Demo GIF for the README | M5 §7.5 | Deferred. `screencapture` needs Screen Recording for the terminal's app; or record with ⌘⇧5 and run `scripts/make-demo-gif.sh --from` |
| 4 | Optional: ask Anthropic to confirm the terms | M5 §7.8 | |
| 5 | ≤ 3 s for 15–20 s dictations | M3 | Now 3.5–5.7 s; the time left is Claude generating the text. Ideas in [plans/M3-speed.md](plans/M3-speed.md) §8 |
| 6 | Model download resume on a real dropped connection | M4.6 | Tested only without a dropped connection |
| 7 | Apple Developer account for notarized releases | Decision 3 | `release.sh` already notarizes when the credentials are set |
| 8 | Later: History pane, editing mode prompts, a cleanup timeout setting, Sparkle updates, a Homebrew cask | M4 | Moved out of M4's scope |

## Next step

M1–M5 are done: **v0.2.0 is released** under the new name (2026-09-25). Real dictations take 2.7 s (8 s of speech) to
3.5–5.7 s (17–22 s of speech) with Claude, about 1.5 s with S1-mini. Next: the open items above (the offline test and the
second-Mac install first), then M5.4 (the trust release, v0.3.0), M5.5 (Command Mode, context and snippets), then M6 (more
providers).
