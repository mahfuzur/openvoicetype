# Roadmap: Voice to Text

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
- It works in daily use. Real timings: whisper 2–4 s, claude 4–8 s, **total 6–10 s**.

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
| Too slow compared with other tools | 6–10 s, versus about 1–3 s for Wispr Flow and VoiceInk |

## Roadmap

| # | Milestone | Effort | Status | Why |
|---|---|---|---|---|
| M1 | Floating overlay with animations | 1–2 days | ✅ Done (2026-09-23) | You can see when it's listening and working |
| M2 | Formatting quality | 2 days | ✅ Done (2026-09-23) | Lists, paragraphs, spoken commands, app-aware style |
| M2.5 | Offline cleanup with S1-mini | 1–2 days | Built (2026-09-24), needs a Wi-Fi-off test | Works with no internet; a fully on-device option |
| M3 | Native pipeline and speed | 3–4 days | Not started | Around 2–3 s total, needed before a public release |
| M4 | Settings window and providers | 4–5 days | Not started | Turns it into a platform: pick Claude, Codex, Gemini, Ollama or an API |
| M5 | Open-source release | 2–3 days | Partly done (license, CI, docs) | Name, license, CI, signing, docs |

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

**Runtime:** llama.cpp from Homebrew (`brew install llama.cpp`), the runtime the S1-mini authors document. Ollama was
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

**Offline check:** `dictate.sh` skips Claude when the Mac has no default route (`route -n get default`, instant), so the app and
the CLI both go straight to S1-mini instead of waiting for Claude to time out. `VTT_OFFLINE=on` forces it for testing.
A network without internet access still waits for Claude's timeout, then falls back.

**Work:**
- [x] `dictate.sh`: `refine_s1()`, `s1-server` commands with an idle watchdog, the fallback chain, and logging (`refine=s1` / `refine=s1-fallback`)
- [x] `config.example.sh` and `install.sh`: `CLEANUP`, `S1_*` settings, `brew install llama.cpp`, model download
- [x] App: Cleanup Model menu (Claude Haiku / Claude Sonnet / S1-mini offline, plus the fallback toggle), llama-server kept loaded while S1-mini is selected, "Pasted · cleaned offline"
- [x] Overlay: "Polishing offline" when S1-mini is selected
- [x] `evals/run.py --cleanup s1`: baseline recorded (below)
- [x] README: offline mode, privacy note, "S1-mini by Superwhisper" credit; CLAUDE.md pipeline notes
- [ ] Real test: Wi-Fi off, dictate in the app

**Eval baseline (2026-09-24):** S1-mini **10/20** (median 1.1 s per case in the parallel eval, about 0.1–0.3 s per call when warm)
vs Haiku **20/20** (3.8 s). Two failures are code-mode cases S1-mini skips by design. The rest: no vocabulary ("Cloud Code"),
no context-based fixes of Whisper mishearings ("witness day"), spoken email digits, and one heavy self-correction where it
dropped items. Good enough as a fallback; Claude stays the default for quality.

**Done when:** with Wi-Fi off, a dictation is cleaned up by S1-mini and pasted, and the self-test passes with S1-mini selected.

## M3: Native pipeline and speed

Target: **≤ 3 s** from stopping to pasted text, for 15 s of speech.

| Stage | Now | Change | Expected |
|---|---|---|---|
| Recording | `rec` child process | Swift `AVAudioEngine` (from M1) | No change |
| Whisper | 2–4 s (loads 1.6 GB on every call) | The app launches and supervises `whisper-server`, keeping the model warm; the app posts audio to it over HTTP | < 1 s |
| Claude | 4–8 s (CLI starts every time) | Keep one `claude -p --input-format stream-json --output-format stream-json` running and send each transcript to it; restart it every N dictations | About 1.5–3 s (to be measured) |
| Paste | < 0.3 s | No change | No change |

Other work:
- Swift `Transcriber` and `Refiner` protocols replace the bundled bash script. The script stays as a CLI for power users.
- **Push-to-talk:** Carbon's `kEventHotKeyReleased` lets holding the key record and releasing it stop, with no extra permission.
  This is offered alongside toggle mode.
- Check memory use: the turbo model takes about 1.6 GB of RAM. Offer to unload it after N minutes idle.

## M4: Settings window and providers ("platform")

A SwiftUI settings window with these tabs:
- **General:** hotkey recorder (any combination), toggle or push-to-talk, overlay, sounds, launch at login.
- **Transcription:** model manager that downloads tiny, base, small or large-v3-turbo from Hugging Face with progress,
  so there's no dependency on Superwhisper's folder. Also language and vocabulary.
- **AI Cleanup:** provider, model, timeout, and a **Test** button that runs a sample and shows the result and latency.
- **Modes:** edit each mode's prompt and its app mapping.
- **Dictionary:** replacements and vocabulary.
- **History:** the last N dictations (raw and cleaned). Copy or re-run one with another mode.
- **Onboarding:** a first-run wizard for the Microphone and Accessibility permissions, model download, provider choice and a test dictation.

**Providers** (a `Refiner` protocol with one implementation each):

| Provider | How it's called | Auth | Notes |
|---|---|---|---|
| Claude Code CLI | `claude -p` | The user's Claude subscription | Current implementation |
| S1-mini | Local `llama-server` | None | M2.5; the offline fallback |
| OpenAI Codex CLI | `codex exec` | The user's ChatGPT subscription | Flags need checking |
| Gemini CLI | `gemini -p` | Google account | Flags need checking |
| Ollama / LM Studio | Local HTTP | None | Fully offline; fast with small models |
| OpenAI-compatible API | HTTPS | API key stored in Keychain | OpenAI, Anthropic API, OpenRouter, Groq |
| None | Raw Whisper text | None | Fastest option |

Installed CLIs are detected automatically by resolving the user's login-shell `PATH` (`zsh -lc 'command -v claude'`).

## M5: Open-source release

- **Name and IDs:** choose a unique name ("Voice to Text" is too generic to find) and check GitHub, the App Store and trademarks.
  Use the bundle ID `io.github.<user>.<name>`, and remove personal paths, e-mail addresses and the Superwhisper model path.
- **License:** MIT (my recommendation, the most permissive) or GPL-3 (like VoiceInk). Don't copy code from GPL projects into an MIT repo.
- **Toolchain:** install Xcode locally, because contributors and CI expect it. GitHub Actions on a macOS runner builds, tests
  and attaches the `.app` to each release.
- **Distribution:** a **signed and notarized** DMG needs the Apple Developer Program ($99 a year). Without it, users see
  Gatekeeper warnings and lose permission grants on every update. Until then, publish a Homebrew cask or build-from-source steps.
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

## Proposed: Command Mode and on-screen context

These are the two features, from comparing with Wispr Flow and Typeless, that are worth copying (researched 2026-09-23):

- **Command Mode:** select text, press a second hotkey, and speak an instruction ("make this more polite",
  "turn this into bullet points", "translate to Bengali"). Claude rewrites the selection in place. This uses a separate
  prompt in which instructions *are* followed. It reads the selection through Accessibility (`AXSelectedText`), falling back to a copy.
- **On-screen context:** send the window title and the text around the cursor (recipients, channel names) as spelling hints.
  It's opt-in per app, because this extra text also goes to Claude.
- **Small additions:** snippets (a spoken trigger inserts a saved text) and a Translate mode.
- **Not planned:** a custom speech recognizer or fine-tuned LLM (these need GPU servers), learning a personal writing style,
  and a dictionary that learns from your edits (reading text fields back after pasting is fragile).

## Decisions

1. ~~Overlay style~~: a bottom-centre pill (done in M1).
2. ~~Name and license~~: **Voice to Text**, **MIT**, bundle ID `io.github.mahfuzur.voicetotext`.
3. **Apple Developer account** ($99 a year) for signed and notarized releases: still open. Until then, releases are built from source.
4. ~~Offline cleanup~~: **S1-mini** through llama.cpp. Claude stays the default; S1-mini is the automatic fallback and a selectable option (2026-09-24).

## Next step

M2.5: S1-mini offline cleanup. Then M3: speed (whisper-server with the model kept loaded, a persistent Claude session),
aiming for ≤ 3 s end to end.
