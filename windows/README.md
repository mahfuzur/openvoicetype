# OpenVoiceType for Windows (in progress)

The native Windows app (plan: [docs/plans/M8-windows.md](../docs/plans/M8-windows.md), milestone W2). A tray app: press
Ctrl+Alt+Space, speak, press it again (or hold it while you speak), and the cleaned-up text is pasted into the window
that was in front. It uses the shared Rust crates at the repo root: `ovt-core` (settings, modes, paste-target rules)
and `ovt-pipeline` (Whisper and the cleanup, the same as `scripts/dictate.sh`).

Not released yet: there's no installer, no Settings window (settings open in Notepad) and no setup window.

## Build

On Windows 11 with [Rust](https://rustup.rs) (the MSVC toolchain), from the repo root:

```powershell
cargo build --release -p openvoicetype-windows
```

The program is `target\release\OpenVoiceType.exe`. It runs without a console window; Quit is in the tray menu.

## What it needs next to it

```
OpenVoiceType.exe
helpers\whisper-server.exe   whisper.cpp's server (and its DLLs), kept loaded while you dictate
helpers\whisper-cli.exe      the fallback when the server can't start
helpers\llama-server.exe     only for S1-mini (offline cleanup)
prompts\                     the repo's prompts\ folder (found next to the exe or up to three folders above it)
```

Until `scripts/build-windows-deps.ps1` exists (W6), take the binaries from whisper.cpp's `whisper-bin-x64.zip` and
llama.cpp's `win-vulkan-x64` (or `win-cpu-x64`) release.

- **Speech model:** `%LOCALAPPDATA%\voice-to-text\whisper\ggml-large-v3-turbo-q5_0.bin` (the file name is
  `whisperModel` in the settings). Download it from Hugging Face (`ggerganov/whisper.cpp`).
- **Cleanup:** Claude Code installed and signed in (`irm https://claude.ai/install.ps1 | iex`, then `claude auth login`).
  `claude.exe` is found in `%USERPROFILE%\.local\bin`, WinGet's links, npm's `claude.cmd`, then `PATH`.
- **Settings:** `%APPDATA%\voice-to-text\settings.json`, the same keys as the other apps (Settings… in the tray menu
  opens it in Notepad; changes apply within 2 seconds). An API key for the OpenAI-compatible engine is read from
  Credential Manager: a generic credential named `<host>.OpenVoiceType cleanup API key`.
- **Log:** `%LOCALAPPDATA%\voice-to-text\logs\dictate.log` (Open Log in the menu).

## Checks

```powershell
OpenVoiceType.exe --logic-selftest report.txt
```

runs the checks that need no microphone, keystrokes or windows (hotkey parsing, terminal detection, clipboard formats,
WAV writing, overlay drawing…) and writes one `OK`/`FAIL` line each, plus `INFO` lines on what's installed. The exit
code is 1 if anything failed. `--console` attaches the app to the terminal it was started from.

The unit tests run on every platform (`cargo test -p openvoicetype-windows`); one more touches the real clipboard and runs
only when asked: `cargo test -p openvoicetype-windows -- --ignored`.
