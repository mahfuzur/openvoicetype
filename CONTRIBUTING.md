# Contributing to Voice to Text

Thanks for helping. Bug reports, prompt improvements, new eval cases and code are all welcome.

## Development setup

You need macOS 13+ on Apple Silicon, Homebrew, and the Xcode Command Line Tools (the full Xcode isn't required).

```bash
brew install sox whisper-cpp shellcheck
./scripts/install.sh               # Whisper model, config, `dictate` command, self-test
./scripts/setup-signing.sh         # one time: stable local code signing (keeps permissions across rebuilds)
./scripts/build-app.sh --install   # build, install to ~/Applications, relaunch
```

`build-app.sh` bundles `scripts/dictate.sh` and `prompts/` into the app. Rebuild after changing either.

## Project layout

| Path | What it is |
|---|---|
| `app/Sources/VoiceToText/` | The menu-bar app (Swift, AppKit + SwiftUI): hotkey, recorder, overlay, modes, paste |
| `app/Sources/ObjCSupport/` | A small Objective-C helper that turns AVFoundation exceptions into Swift errors |
| `scripts/dictate.sh` | The pipeline: Whisper transcription, Claude cleanup, post-processing. It also works as a standalone CLI |
| `prompts/` | Cleanup rules (`system.md`) and per-mode style (`modes/*.md`) |
| `evals/` | The formatting-quality eval: `cases.json` and `run.py` |
| `docs/` | The roadmap and detailed milestone plans |

## Before opening a pull request

1. `shellcheck scripts/*.sh` passes.
2. `./scripts/build-app.sh` builds with no warnings.
3. `./scripts/dictate.sh selftest` works.
4. **If you changed a prompt or `post_process()`:** run the eval, and include the before and after numbers in the PR:
   ```bash
   evals/run.py              # about 20 cases on Haiku, about 1 minute
   evals/run.py --runs 3     # check for flakiness
   evals/run.py --e2e        # also run synthesized speech through Whisper
   ```
   The target is ≥ 90% on Haiku, and cases in the "must never" category (answering the transcript, inventing values) must always pass.
5. **For UI changes:** check the overlay without a mic by running
   `app/build/VoiceToText.app/Contents/MacOS/VoiceToText --overlay-snapshots /tmp/snaps`, and add images to the PR.

## Adding an eval case

Found a dictation that came out wrong? Copy the `raw:` line for it from `~/Library/Logs/voice-to-text/dictate.log`,
and add a case to `evals/cases.json` with the checks the output should pass (`contains`, `not_contains`, `regex`,
`min_list_items`, `min_paragraphs`, …). A failing case with a clear expectation is a great first contribution.

## Guidelines

- Keep the audio local. Only transcript text may leave the machine, and only to the provider the user chose.
- Never use `claude --bare`: it ignores subscription logins.
- Don't copy code or prompt text from GPL-licensed projects into this MIT-licensed repo.
- Match the style of the surrounding code, and keep dependencies to a minimum (there are currently no Swift packages).
- `CLAUDE.md` holds the architecture notes and known gotchas. Read it before changing the recorder or the Claude call.
