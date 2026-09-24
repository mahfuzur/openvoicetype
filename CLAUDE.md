# Voice to Text

An open-source, local-first dictation app for macOS (Apple Silicon). Speech goes in, and polished text is pasted into the focused app.
Whisper (whisper.cpp) transcribes on-device. The cleanup step calls the user's own logged-in `claude` CLI in print mode,
so it runs on their existing Claude subscription with **no API key**. That's the project's main difference from
Superwhisper, Wispr Flow, Typeless and VoiceInk.

See [docs/ROADMAP.md](docs/ROADMAP.md) (the phased roadmap, which is the source of truth) and [docs/plans/](docs/plans/)
(detailed milestone plans). The user-facing documentation is [README.md](README.md), and the contributor guide is [CONTRIBUTING.md](CONTRIBUTING.md).

## Pipeline

```
hotkey -> record (AVAudioEngine in the app, or sox/rec from the CLI; 16 kHz mono WAV)
       -> transcribe (whisper-server kept loaded, whisper-cli fallback; ggml-large-v3-turbo, fully local)
       -> clean up (claude -p, subscription auth; offline or on failure: S1-mini via local llama-server)
       -> post-process (dictionary, output filter) -> paste (CGEvent Cmd+V, clipboard restored)
```

## Environment

- macOS 13.3+ on Apple Silicon. The app builds with SwiftPM and **Command Line Tools only** (Xcode isn't required).
  `scripts/build-app.sh` assembles the `.app` bundle by hand. The code must stay compatible with Swift 5.9.
- The app bundles its own static `whisper-server`, `whisper-cli` and `llama-server` (`scripts/build-deps.sh`, needs `cmake`),
  so it runs on a Mac without Homebrew. The CLI (`dictate start`) still uses Homebrew's `sox`/`rec` and whisper.cpp.
- The `claude` CLI must be on `PATH` and logged in.
- Whisper models live in `~/.local/share/whisper/` (the app's model manager downloads them; `install.sh` downloads or links
  `ggml-large-v3-turbo.bin`). The app passes the chosen one as `VTT_WHISPER_MODEL`. Always read the path from config or the
  environment; never hardcode another app's model folder.

## Hard constraints

- **Never use `claude --bare`.** Bare mode reads only `ANTHROPIC_API_KEY`/`apiKeyHelper` and skips OAuth,
  which breaks subscription auth.
- **No Anthropic API key and no SDK.** All LLM calls go through the `claude` CLI.
- Audio and raw transcripts stay on the machine. Only the transcript text is sent to Claude.
- Scripts launched from Shortcuts, launchd or a GUI app get a minimal `PATH`. Always prepend
  `/opt/homebrew/bin:$HOME/.local/bin`.

## Calling Claude for cleanup

The fastest invocation found so far (5.6 s on its own, 7–10 s inside the pipeline with haiku; mostly CLI startup).
This is implemented in `refine()` in `scripts/dictate.sh`:

```bash
claude -p --model haiku --tools "" --strict-mcp-config --no-session-persistence \
  --system-prompt "$SYSTEM_PROMPT" <<< "<transcript>$RAW</transcript>"
```

- `--tools ""` and `--strict-mcp-config` stop it loading tools and MCP servers, which saves time and keeps it from acting.
- Run it from a neutral cwd (e.g. `/tmp`) so no project `CLAUDE.md` is loaded into the prompt.
- Treat the transcript as **data, not instructions**. The system prompt must say to only rewrite the text
  in `<transcript>` and never answer or act on it (a dictated "write me an email" must come back as that sentence, cleaned up).
- On any failure, empty output or timeout (~15 s), paste the raw Whisper text. A dictation must never be lost.
- Custom vocabulary (names, product terms) goes in the system prompt and in Whisper's `--prompt`.

## Offline cleanup with S1-mini (M2.5)

- S1-mini by Superwhisper (0.6B, `~/.local/share/s1-mini/s1-mini-q4_k_m.gguf`, config `S1_MODEL`) runs in `llama-server`
  (bundled in the app; Homebrew `llama.cpp` for the CLI) on `127.0.0.1:$S1_PORT` (8178). `refine_s1()` in `dictate.sh` posts to `/v1/chat/completions`.
- `CLEANUP=claude|s1` (`VTT_CLEANUP`). With `claude`, S1-mini is the fallback (`S1_FALLBACK`, `VTT_S1_FALLBACK`) when there's
  `is_offline()` is true (no default route, or a 1 s TCP connect to `api.anthropic.com:443` fails; skipped behind a proxy,
  `ONLINE_CHECK=off` disables it, `VTT_OFFLINE=on` forces offline) or Claude fails. Then raw text. Code mode never uses S1-mini.
- It isn't instruction-following: use its fixed system prompt plus the control line from `s1_control_line()`.
  It needs `--jinja --chat-template-kwargs '{"enable_thinking":false}' --temp 0`. It takes no vocabulary.
- Server lifecycle: `dictate.sh s1-server start [--keep] | release | stop | status`. The app keeps it loaded (`--keep`) while
  S1-mini is selected; a fallback start is stopped by a detached watchdog after `S1_IDLE_MINUTES`. About 1 GB RSS.
- Credit it as "S1-mini by Superwhisper" (license naming clause). Eval: `evals/run.py --cleanup s1` (baseline 10/20; code cases skip by design).

## Prompts and formatting (M2)

- `prompts/system.md` holds the core cleanup rules and examples. `prompts/modes/<mode>.md` adds per-mode style
  (default, chat, email, code, notes; raw skips Claude). Both are bundled into the app's `Resources/prompts`.
  The script finds them next to itself or one level up, resolving the `~/.local/bin` symlink.
- The user message is `<context app=… mode=…/>`, then `<vocabulary>` (config `VOCAB` + `VTT_VOCAB` + dictionary terms),
  then `<transcript>`.
- `post_process()` in `dictate.sh` runs after Claude, and also when cleanup is skipped. It applies dictionary replacements,
  strips tags, preambles and code fences, lower-cases email addresses, splits paragraphs over 4 sentences (not in chat),
  and removes the trailing period from one-line chat messages.
- The Whisper `--prompt` is a style sample (`WHISPER_STYLE`: punctuation, digits, AWS/GitHub) plus the vocabulary.
  It noticeably improves punctuation and tech-term recognition, and doesn't leak on silence or noise.
- **Gotcha:** Haiku uses extended thinking by default. With the long prompt, a cleanup took 30 s and 2,500 thinking tokens.
  `MAX_THINKING_TOKENS=0` (config `CLAUDE_THINKING_TOKENS`) brings it back to about 5 s with equal eval quality.
  `--effort low` does *not* disable thinking.
- Never copy text from VoiceInk's prompts (it's GPL-3); ours are written from scratch.
- **Eval:** `evals/run.py [--model sonnet] [--case ID] [--runs N] [--e2e]`. The cases are in `evals/cases.json`, and reports
  go to `evals/results/` (gitignored). Run it after every prompt or post-processing change; the target is ≥ 90% on Haiku.
  Add real failing dictations from `dictate.log` as new cases.

## Latency (M3, see docs/plans/M3-speed.md)

- Stop → pasted, real dictations: about 2.7 s (8 s of speech) to 3.5–5.7 s (17–22 s) with Claude, about 1.5 s with S1-mini
  (was 6–9 s). What's left is Claude generating the text. Measure with `evals/run.py --e2e --timing --jobs 1` (short
  sentences, so it reads lower, about 2.5 s); the app logs `APP TIMING …` for every dictation.
- **whisper-server** (`dictate.sh whisper-server start|release|stop|status`, port `WHISPER_PORT` 8179) shares the server helper
  (`srv_*` in `dictate.sh`) with S1-mini: pid/keep/used/lock files in `$STATE_DIR`, a detached idle watchdog
  (`WHISPER_IDLE_MINUTES`). `transcribe_server()` posts to `/inference` with the same prompt; `whisper-cli` is the fallback.
  The app and `dictate.sh start` load it when recording starts (0.6 s). About 750 MB RSS with the compressed model
  (the default for new installs), 1.7 GB with the full one.
- **Pre-started Claude:** `claude_prestart()` launches `claude -p --input-format stream-json --output-format stream-json --verbose`
  (same flags as the one-shot call) with its stdin on a fifo held open as fd 3, *before* the transcript exists. The app runs
  `dictate.sh refine` when recording starts and writes the transcript to its stdin on stop; `claude_send()` writes one user message
  and polls the output file for the `result` event. One process per dictation: a reused session was just as fast but kept every
  earlier transcript in its history. Cancel or no speech = close stdin with nothing; the script and its Claude exit.
- **Gotcha:** Claude Code exports `CLAUDE_PID` to the commands it runs. Never use `CLAUDE_*` names for the script's own state;
  the pre-start uses `PRESTART_PID`/`PRESTART_DIR`, reset at startup. An inherited pid once made the cleanup kill the
  developer's Claude Code session.
- Fds passed to detached servers: launch them with `3>&-` so they don't hold a pre-started Claude's stdin open.
- `claude_send()` doesn't just wait for the timeout. A non-JSON line or a process that died without output returns 2, and
  `refine()` retries with the one-shot call (a Claude Code update changed the stream). No event within 5 s, or an error
  `result`, returns 1, and cleanup falls back to S1-mini or raw text. An `assistant` answer with no `result` within 1.5 s is
  used as-is. Deadlines use `now_ms`, not loop counts (each check costs about 30 ms). Test with a fake `CLAUDE_BIN`.
- `srv_running()` also checks the pid's command name (`llama-server` / `whisper-server`), so a stale pid file can never
  make `stop` kill an unrelated process that reused the pid.

## Self-contained app, setup and releases (M4, see docs/plans/M4-app-and-install.md)

- `build-deps.sh` builds whisper.cpp and llama.cpp at pinned tags: static (`BUILD_SHARED_LIBS=OFF`), Metal shaders embedded
  (`GGML_METAL_EMBED_LIBRARY`, so the Metal compiler from Xcode isn't needed), no OpenSSL/curl, `GGML_CCACHE=OFF` (a broken
  Homebrew ccache aborts the build), deployment target 13.3 (Accelerate's new BLAS). It fails if a helper links anything
  outside `/usr/lib` and `/System`. `build-app.sh` copies them to `Contents/Helpers` and signs them before the app.
- The app sets `VTT_BIN_DIR` (the helpers, first on `PATH` in `dictate.sh`), `VTT_WHISPER_MODEL` and `VTT_CLAUDE_BIN` (found
  through the login shell by `ClaudeCLI`, so npm/nvm installs work; its directory is added to `PATH` for `node`).
- **Gotcha:** the bundled servers compile their Metal shaders on first launch: 10 s (Whisper) to 19 s (llama) once, then
  0.3–0.6 s. macOS caches it per binary and location, so every re-signed or moved build pays it again (e.g. launched
  from the DMG, then moved to Applications). The app warms both servers after a model
  download and whenever the helpers change (`warmedHelpers`), and `srv_start` waits up to 30 s.
- `srv_start` restarts a running server whose binary or model differs from the wanted one (`<name>.signature` state
  file): a model change, or an updated or moved app whose old servers would keep running from the old copy.
- `transcribe_wav` reads the length from the WAV header (`wav_seconds`, Perl). Without it `soxi` was required, and a Mac without
  `sox` skipped every dictation as too short.
- First-run setup (`SetupWindow.swift`) opens when no Whisper model is installed or the app runs from the DMG/Downloads.
  Claude is installed and signed in through `.command` files opened in Terminal (official installer, `claude auth login`);
  the app only reads `claude auth status --json`. With no Claude, S1-mini is selected so dictation works.
- Signing levels (`release.sh`): ad-hoc, the self-signed "Voice to Text Release" certificate from CI secrets
  (`make-release-cert.sh`; designated requirement = identifier + certificate, so permission grants survive updates), or
  Developer ID + notarization. **Gotcha:** codesign only finds an identity in a keychain on the user search list, even with
  `--keychain`; `release.sh` adds its temporary keychain and restores the list on exit. Don't restore a search list in zsh with
  an unquoted variable: zsh doesn't word-split it, and the list becomes one bogus entry.

## Commands

- `./scripts/install.sh`: links `~/.local/bin/dictate`, creates the config, links the model, runs the self-test.
- `./scripts/dictate.sh selftest`: runs speech synthesized with `say` through Whisper and Claude, with no mic or paste. Run it after any pipeline change.
- `./scripts/dictate.sh file <wav>`: processes an existing recording and prints the raw text, cleaned text and timings.
- `./scripts/build-app.sh [--install]`: builds `app/` with SwiftPM into `app/build/VoiceToText.app`,
  bundling `scripts/dictate.sh`, `prompts/` and the helpers into it. `--install` copies it to `/Applications/Voice to Text.app` (the same name and place as the DMG, so there is one copy) and relaunches it.
  Rebuild after changing `dictate.sh`, because the app runs its bundled copy. `VERSION=`, `BUNDLE_DEPS=off`, `SIGN_IDENTITY=`.
- `./scripts/release.sh v0.2.0`: builds, signs and packages `dist/VoiceToText-0.2.0.dmg` (+ `.sha256`), with the window
  layout from `scripts/dmg-settings.py` (dmgbuild; the app is named "Voice to Text.app" in the DMG). A pushed `v*` tag runs
  it in `.github/workflows/release.yml` and publishes a GitHub Release.
- Artwork (see docs/ARTWORK.md): `swiftc -o /tmp/make-artwork scripts/make-artwork.swift && /tmp/make-artwork` writes
  `app/Resources/AppIcon.icns`, `app/Resources/dmg-background.tiff` and `docs/images/app-icon.png`. Never use SF Symbols in
  the app icon (license); the menu-bar icon stays still and monochrome, with a red dot only while working.
- `VoiceToText --settings-snapshots <dir>`: renders every Settings pane and the setup window to PNGs.
- `shellcheck scripts/*.sh`: must pass.

## App architecture (current)

The menu-bar app (`app/Sources/VoiceToText/`) records in-process and runs `dictate.sh` for transcribing and cleanup:
- `HotKey.swift`: Carbon global hotkey (press and release events), any key combination (`Combo(event:)`, stored as keyCode,
  Carbon modifiers and a label; the old `hotKeyIndex` preset is migrated). Needs no permission. Esc cancels, and is registered only
  while recording. Hold to Talk (menu) starts on press and stops on release; a press under 0.3 s is treated as a tap.
- `Recorder.swift`: `AVAudioEngine` tap, converted to a 16 kHz mono 16-bit WAV in `$TMPDIR/voice-to-text/`,
  plus a 0–1 input level for each buffer (drives the waveform). Starting is asynchronous: `onReady` fires on the
  first buffer, and starting retries for about 2 s because Bluetooth mics report no format while switching to headset (HFP) mode.
  There's a 2.5 s no-audio watchdog.
  **Gotcha:** `inputNode.auAudioUnit.setDeviceID` posts an `AVAudioEngineConfigurationChange`. The handler re-arms the
  *same* engine and ignores the change if the engine is running with an unchanged format. Rebuilding the engine in the handler
  re-selects the device and loops forever, which shows up as "No audio from …".
  The system default is never pinned; only a device the user picked is.
  **Gotcha 2:** `installTap` and `engine.start()` can raise **NSExceptions** (e.g. while a Bluetooth headset is switching
  profile), and Swift can't catch those, so the app crashes. Every such call goes through `VTTObjC.catchException`
  (the `ObjCSupport` target), and `run()` refuses to tap until `inputFormat` and `outputFormat` report the same non-zero
  sample rate. If the mic fails after audio was captured, the recording is kept and transcribed.
  Mic events are written to `dictate.log` as `APP MIC …` lines.
- `AudioDevices.swift`: Core Audio input-device list (UID, name, Bluetooth flag) and the default input.
- `Dictation.swift`: the state machine (idle, recording, transcribing, polishing). On start it runs `dictate.sh whisper-server start`
  and launches `dictate.sh refine` (a `ScriptRun`, stdin kept open) for the frontmost app's mode; if the mode changed by the
  time you stop, that run is dropped and a fresh one is used. It runs `dictate.sh transcribe <wav>`, then
  `dictate.sh refine` (transcript on stdin; exit 3 means the raw text was used, exit 4 means S1-mini replaced an unavailable Claude),
  with `VTT_QUIET=on`, `VTT_REFINE`, `VTT_CLEANUP`, `VTT_S1_FALLBACK`
  and `VTT_CLAUDE_MODEL`. `VTT_*` variables override `config.sh`.
- `MenuBarIcon.swift`: the status-item icon: still waveform bars (a template image, so macOS makes it white or black for
  the menu bar), plus a red dot while working (recording, transcribing, polishing). No animation: the overlay shows the
  details. The busy image isn't a template (it holds red), so it draws the bars in the menu bar's appearance itself.
  `--overlay-snapshots` includes `7-menubar-icons.png`.
- `Overlay.swift`: a floating, click-through, non-activating `NSPanel` hosting a SwiftUI pill. It shows a live waveform and timer,
  then Transcribing, then Polishing, then Pasted, No speech, or an error with a shake.
- `OverlaySnapshots.swift`: `VoiceToText --overlay-snapshots <dir>` renders every overlay state to PNGs.
  `--overlay-demo` animates the live panel through all states. Use these to check UI changes without a mic.
  `open -n app/build/VoiceToText.app --args --recorder-selftest <report.txt> [--pin-default]` records 2 s and writes
  `OK device=… ready=… peak=… wavBytes=…`. Launch it with `open` so the app's own mic permission applies.
- `Paster.swift`: saves the whole clipboard, pastes with a CGEvent Cmd+V (needs Accessibility), then restores the clipboard.
- `AppDelegate.swift`: the status item and short menu (mode, cleanup engine, microphone, Settings…, Set Up…), sounds, and
  reacting to setting changes (hotkey, overlay, S1-mini server, model reload).
- `AppSettings.swift`: every setting (`ObservableObject`, UserDefaults, the old keys), shared by the menu, the windows and `Dictation`.
- `SettingsWindow.swift`: `NSTabViewController` (toolbar style) with SwiftUI panes: General, Speech, Cleanup, Dictionary,
  Modes, About. `SettingsComponents.swift`: model rows, Claude status, the hotkey recorder. `SetupWindow.swift`: first-run setup.
- `ModelManager.swift`: the model catalog (pinned Hugging Face URLs, sizes, SHA-256) and downloads with progress, resume and a
  checksum check. `ClaudeCLI.swift`: find, version, `auth status`, install/sign in. `Updater.swift`: daily GitHub release check.
  `DictionaryFile.swift`: reads and writes `dictionary.txt`.
- `Modes.swift`: `DictationMode` and the bundle-ID → mode mapping. `DictationContext.current()` reads the frontmost app
  when recording stops. `RichText.swift` converts list lines to HTML for rich paste (not in code mode).
  `AppLog` writes `APP RESULT …` lines to `dictate.log`.

## Conventions

- Phase 1 (POC): Bash in `scripts/`, POSIX-friendly, `set -euo pipefail`, checked with `shellcheck`.
- Phase 2 (app): Swift + SwiftUI menu-bar app in `app/` (SwiftPM package), `LSUIElement` agent app.
- Keep the pipeline stages behind small interfaces (Recorder, Transcriber, Refiner, Output) so each
  can be swapped (e.g. whisper-cli -> whisper-server, clipboard paste -> Accessibility insert).
- Temp audio goes in `$TMPDIR`, never in the repo, and is deleted after each run.
- Logs: `~/Library/Logs/voice-to-text/` (timings, raw vs cleaned text for debugging; off-switch in config).

## macOS permissions

- Microphone: whichever process records (Terminal/iTerm for the POC, the app bundle later).
- Accessibility: whichever process sends Cmd+V via System Events / CGEvent.
- **Signing:** ad-hoc signatures (`codesign -s -`) make macOS identify the app by its cdhash, so every rebuild loses its grants.
  The microphone permission is requested again, but Accessibility silently stops working.
  `scripts/setup-signing.sh` creates the self-signed identity "Voice to Text Local Signing" in a dedicated keychain
  (`~/Library/Keychains/voice-to-text-signing.keychain-db`, password in `~/.config/voice-to-text/signing-keychain-password`).
  `build-app.sh` uses it automatically, so the designated requirement is `identifier + certificate leaf`, which stays the same across rebuilds.
  Check it with `codesign -d -r- "/Applications/Voice to Text.app"`.
