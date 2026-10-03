# M8: OpenVoiceType for Windows (native, full parity), Detailed Plan

**Goal:** a native Windows 11 app with the same features as the Mac app, released only once it reaches full parity. It keeps
the same privacy guarantees (audio stays local, no API key, the user's own `claude` CLI) and GPU-accelerated Whisper.

**Status legend:** ☐ to do · ◐ in progress · ☑ done

**Status: ◐ W0 done, W2 built (2026-10-03); waiting for the first run on the PC (W1).** The Git Bash gate failed on the
first CI run (below), so the Windows app runs the cleanup pipeline in Rust (`crates/ovt-pipeline`), checked against
`dictate.sh` by golden tests (287 cases) and the shared contract test, on Linux and Windows CI. The first app
(`windows/crates/openvoicetype`) builds with MSVC in CI; it has only run under Wine so far.

## Context

OpenVoiceType runs on macOS, and the Linux port (M7) is under way. The maintainer now has a Windows 11 PC. The Linux plan set
the pattern this one follows: a native app per platform, one shared pipeline (`scripts/dictate.sh`, the prompts and the
evals), and the Swift app's logic ported once to Rust in `ovt-core`, which both new apps use.

**Decisions made:**
- **Stack:** Rust. Win32 through `windows` (windows-rs) for hotkeys, input, clipboard, UI Automation and the overlay;
  `tray-icon` + `muda` for the notification-area icon and menu; Slint (Fluent style) for the Settings and setup windows.
- **Location:** the same repo. One Cargo workspace at the root: `crates/` holds what Linux and Windows share
  (`ovt-core`, `ovt-pipeline`), `windows/crates/` the Windows app.
- **Pipeline: in Rust (`crates/ovt-pipeline`), not `dictate.sh`.** The first plan ran `dictate.sh` through Git for Windows'
  bash, with a gate (W0). It failed on both counts on `windows-latest` (2026-10-03):
  - **Speed:** `refine` costs 2,228 ms of script time under Git Bash, against 209 ms on Linux. A profile shows no hot spot:
    each process start costs 15–60 ms, and `refine` starts about 100 (subshells, perl, grep, date, tr).
  - **Pre-start:** a native program can't read an MSYS fifo ("The handle is invalid"), so the pre-started `claude.exe`
    can't work the way it does on macOS and Linux.
  
  Everything else passed (17 of 18 contract checks on the first run), and `dictate.sh` keeps working in Git Bash for the
  CLI (one-shot Claude calls there). The app instead ports the cleanup half of the script to Rust: prompt assembly,
  the Claude pre-start and one-shot calls with the same flags and failure classes, the OpenAI-compatible endpoint, S1-mini,
  the online check, the meaning guard, post-processing, and the result details. It reads the same `prompts/` and
  `dictionary.txt`, and gains no Git dependency (Claude Code no longer needs Git on Windows either).
- **Keeping two pipelines equal:**
  - **Golden tests:** `scripts/make-golden.sh` runs `dictate.sh`'s own functions (`post_process`, `meaning_guard`,
    `user_message`, `system_prompt`, `claude_parse`…) over `tests/golden/inputs.json` and writes `expected.json`;
    `ovt-pipeline`'s tests must match it. CI regenerates it and fails on a difference, so a change to the script without
    the Rust side (or the reverse) is caught.
  - **The same contract:** an `ovt` CLI (`refine`, `command`, `transcribe`, `selftest`) with `dictate.sh`'s environment
    variables, exit codes and result file, so `scripts/test-dictate.sh` and `evals/run.py` run against both.
- **Test machine:** the maintainer's Windows 11 PC. CI runs on GitHub's `windows-latest` (Windows Server 2025).
- **GPU:** whisper.cpp and llama.cpp built with Vulkan (NVIDIA, AMD and Intel) plus a CPU build, chosen at startup.

**What research found (2026-10-03):**
- Claude Code installs natively on Windows (`irm https://claude.ai/install.ps1 | iex`) to
  `%USERPROFILE%\.local\bin\claude.exe`, which the installer doesn't add to `PATH`. WinGet and npm installs exist too
  (`claude.cmd`). Git for Windows is no longer required by Claude Code itself. All the flags we use are cross-platform.
  `claude auth status` prints JSON.
- whisper.cpp publishes Windows binaries (`whisper-bin-x64.zip`: `whisper-server.exe`, `whisper-cli.exe` and the ggml DLLs)
  but no Vulkan build; llama.cpp publishes `win-cpu-x64` and `win-vulkan-x64` builds. All are unsigned. We build our own,
  pinned, like `build-deps.sh` does on the Mac.
- Unsigned or self-signed executables get SmartScreen's "Windows protected your PC", and **Smart App Control blocks them
  outright**, including helpers the app starts. Signing matters more on Windows than on the Mac.

## Windows in 60 seconds (for the owner)

- **Win32:** the native Windows API. **UI Automation (UIA):** Windows accessibility, the equivalent of macOS AX: the focused
  element, whether it's a password box, the selected text.
- **SendInput:** sends key presses to the focused app (our Ctrl+V). **UIPI:** a normal app can't send keys to an app run
  as administrator; it fails silently, so we detect elevated windows and copy instead.
- **Low-level keyboard hook:** a callback that sees every key press and release; how we get hold to talk. Windows removes
  a hook that takes longer than about 1 s to answer, so it runs on its own thread and does nothing slow.
- **Notification area ("tray"):** the icons next to the clock, the menu-bar equivalent.
- **Git Bash:** the bash, perl and curl that come with Git for Windows (an MSYS2 environment). It runs `dictate.sh`.
- **SmartScreen / Smart App Control:** Windows' download reputation check and the stricter "only signed apps" mode.
- **%APPDATA% / %LOCALAPPDATA%:** the per-user folders for settings (roaming) and data (this machine only).

## Architecture

```
OpenVoiceType.exe (Rust, one binary, no console window)
├── ovt-core (shared with Linux)  pure logic ported from Swift, with its unit tests
├── audio      cpal (WASAPI) capture → 16 kHz mono WAV, levels, device list (+Bluetooth flag), device changes
├── hotkey     RegisterHotKey for the press + a low-level keyboard hook for the release (hold to talk), Esc while recording
├── input      SendInput chords (Ctrl+V, Shift+Insert in terminals), wait for modifiers to be released
├── clipboard  Win32 clipboard: snapshot of every format, CF_UNICODETEXT + "HTML Format", excluded from clipboard history
│              and cloud sync, delayed rendering so the restore runs after the target read it (as on the Mac)
├── target     foreground HWND, pid, exe, title; elevation check; UIA focused element (IsPassword, selection, text
│              before caret)
├── overlay    a layered, click-through, no-activate, topmost window (WS_EX_LAYERED|TRANSPARENT|NOACTIVATE|TOOLWINDOW)
├── tray       tray-icon + muda: the icon (still waveform, red dot while working) and the short menu
├── ui         Slint windows: Settings (6 panes), first-run setup
└── ovt-pipeline (shared crate): Whisper (whisper-server kept loaded, whisper-cli fallback) and the cleanup, the same as
    dictate.sh's, behind the same contract (exit codes, result details); the servers are child processes of the app
```

**Repo layout:** `windows/Cargo.toml` (a workspace), `windows/crates/openvoicetype` (the app), `windows/packaging/` (Inno Setup
script, icon), `scripts/build-windows-deps.ps1` (whisper.cpp + llama.cpp, Vulkan and CPU), `scripts/build-windows.ps1`.

**Folders:**

| What | macOS | Linux | Windows |
|---|---|---|---|
| Config, dictionary, settings | `~/.config/voice-to-text` | `$XDG_CONFIG_HOME/voice-to-text` | `%APPDATA%\voice-to-text` |
| Whisper and S1-mini models | `~/.local/share/{whisper,s1-mini}` | `$XDG_DATA_HOME/{whisper,s1-mini}` | `%LOCALAPPDATA%\voice-to-text\{whisper,s1-mini}` |
| Logs | `~/Library/Logs/voice-to-text` | `$XDG_STATE_HOME/voice-to-text` | `%LOCALAPPDATA%\voice-to-text\logs` |
| Runtime state | `$TMPDIR/voice-to-text` | `$XDG_RUNTIME_DIR/voice-to-text` | `%TEMP%\voice-to-text` |

## Mac → Windows parity map

| Mac feature (file) | Windows | Known limit |
|---|---|---|
| Global hotkey, press/release, Esc while recording (`HotKey.swift`) | RegisterHotKey (MOD_NOREPEAT) + LL hook for the release; Esc registered only while recording | A hook can't see keys sent to an elevated window while ours isn't |
| Hold to Talk | the hook's key-up for the hotkey's main key or a modifier | none |
| ⌘V paste, clipboard snapshot and restore, transient, HTML (`Paster.swift`) | SendInput Ctrl+V; snapshot of all formats; `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory`=0, `CanUploadToCloudClipboard`=0; "HTML Format"; delayed rendering for the "was read" signal | Terminals: Shift+Insert (Windows Terminal, conhost and mintty all accept it) |
| Wait for ⌃⌥⇧⌘ release | GetAsyncKeyState until released (0.6 s) | none |
| Layout-aware key code (`KeyboardLayout`) | VkKeyScanEx for "v"/"c"/"z" in the foreground thread's layout | none |
| PasteTarget: pid, window, title, `normalized()` | HWND (IsWindow + equality), pid, exe name, title through `ovt-core::target` | none |
| Password field refused | UIA `IsPassword` on the focused element | Some apps expose no UIA: refuse only when known, as on the Mac |
| Paste into an admin window | detect elevation (OpenProcessToken → TokenElevation; access denied = elevated) → copy, with a notice | UIPI; no fix without signing + uiAccess |
| Swap ⌃⌥Z | UIA TextPattern: the text before the caret; otherwise the 30 s rule | never in terminals |
| Command Mode selection read (`SelectionReader.swift`) | UIA TextPattern `GetSelection` → Ctrl+C with marker and restore (same line-copy guard) | none in terminals (copy only) |
| Re-select last dictation, Restore Original | UIA TextRange `Select` | apps without TextPattern fall back to copy |
| Overlay pill (`Overlay.swift`) | layered window, Direct2D drawing, same phases, colours and timings | none |
| Menu-bar icon and menu | tray-icon + muda | Windows hides new tray icons in the overflow until the user pins them: setup says so |
| Settings (6 panes), Setup, Model manager, Updater, Claude detection | Slint windows; same logic from `ovt-core` | setup steps change (below) |
| Keychain API keys (`APIKeychain.swift`) | Credential Manager (`keyring`), same service and account names | none |
| Launch at login (SMAppService) | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | none |
| Mic via AVAudioEngine, Bluetooth retry, watchdog | WASAPI through cpal, the same retry and no-audio watchdog | Bluetooth mics drop to call quality |
| `BundledHelpers` de-quarantine | not needed with an installer (files it installs carry no download mark) | a portable zip would keep it |
| Metal warm-up | Vulkan pipeline cache warm-up after install and model download | first run slower |

**Windows setup window steps:**
1. Microphone: Windows asks once per app (Settings ▸ Privacy ▸ Microphone); setup checks sound arrives.
2. Speech model download.
3. Cleanup: Claude Code installed (`irm https://claude.ai/install.ps1 | iex` in a PowerShell window) and signed in
   (`claude auth login`), or S1-mini. `claude.exe` is looked for in `%USERPROFILE%\.local\bin`, the WinGet links folder,
   npm's `claude.cmd`, then `PATH` (the native installer doesn't add its folder to `PATH`).
4. Pin the tray icon, then try it.

## Phases (one milestone, one release; checkpoints are internal)

### ◐ W0: Shared groundwork, from the Mac and CI (no Windows PC needed)

- ☑ `dictate.sh` in Git Bash (for the CLI and the measurement): `OS=Windows`, Windows folders, no argument rewriting for
  native programs, `/dev/clipboard`, `\r`-tolerant Whisper output, `.exe`-aware server check, the Windows voice for
  `selftest`, pre-start off. `.gitattributes` keeps LF for scripts and prompts. `test-dictate.sh` passes in Git Bash.
- ☑ CI: a `windows` job (the contract test in Git Bash, `cargo fmt/clippy/test`).
- ☑ The Rust code is one workspace at the root; `ovt-core` moved to `crates/` and builds on Windows (Unix-only parts
  behind `cfg(unix)`, Windows folders in `paths`, a portable `run_id`).
- ☑ `crates/ovt-pipeline`: the cleanup pipeline in Rust, with the golden tests (`golden/make-text.sh`: 239 cases;
  `golden/make-claude.sh`: 48), regenerated in CI. The pre-start uses an ordinary pipe; S1-mini is started by the app
  through `Session::set_s1_starter`.
- ☑ The `ovt` CLI with `dictate.sh`'s contract; `test-dictate.sh` passes against it on Linux and Windows (10/10), and
  `evals/run.py --bin` runs it. A pre-started `refine` (fake Claude) on `windows-latest`: 413 ms against `dictate.sh`'s
  1,929 ms.
- **Left for the PC:** the evals through `ovt.exe` with the real Claude (W1).
- **Checkpoint:** CI green on all three; `test-dictate.sh` passes against both `dictate.sh` and `ovt`; evals on Haiku
  ≥ 90% through `ovt` (on the PC, or on Linux in CI with a fake Claude for the contract part).

### ☐ W1: Spikes on the Windows PC (half a day; they set the final scope)

`windows/pc-test/run.ps1` (in the CI artifact `OpenVoiceType-windows`) walks through them and writes a report:
1. **Pipeline:** `ovt selftest` with the real `claude.exe` and `whisper-server.exe` (the Windows voice instead of `say`);
   the pre-started Claude through a pipe (and the 3 s stdin rule of `claude -p`); Unicode in and out; timings;
   `evals/run.py --bin ovt.exe`.
2. **Hotkey:** hold Ctrl+Alt+Space: one press, one release; Esc only while recording; AltGr layouts.
3. **Paste:** Notepad, Word, Chrome, Edge, VS Code, Windows Terminal (Shift+Insert), an elevated Notepad (must be detected).
4. **UIA:** a password box in Edge and Chrome, selection and text before the caret in Notepad, Word, Chrome, VS Code.
5. **Clipboard:** the history (Win+V) and cloud sync don't keep our pasted text; the restore runs after the target read it.
6. **GPU:** the Vulkan whisper-server on the PC's GPU, and the CPU fallback.

### ◐ W2: End-to-end dictation (first usable build): built, not yet run on a PC

Tray icon and menu, hotkey (toggle and hold), recording, whisper-server start, the pre-started `refine`, paste with restore,
the overlay, sounds, logs (`APP …` lines), settings file shared with `ovt-core`. Built in CI as an artifact.

### ☐ W3: Safety and history

Paste target check, password refusal, elevated windows, swap (Ctrl+Alt+Z), the result file and meaning guard surfaced in the
overlay, `--logic-selftest`.

### ☐ W4: Command Mode

UIA selection read, Ctrl+C fallback with marker, planner from `ovt-core`, re-select, Restore Original, the overlay chip.

### ☐ W5: Settings, setup and the rest

Settings window (General, Speech, Cleanup, Dictionary, Modes with exe names, About), first-run setup, model manager with
SHA-256 checks, S1-mini, OpenAI-compatible endpoint with the key in Credential Manager, launch at login, updater.

### ☐ W6: Packaging, CI, docs, release

- `build-windows-deps.ps1`: whisper.cpp and llama.cpp at the Mac's pinned tags, Vulkan + CPU, static runtime, cached in CI.
- Inno Setup per-user installer (no admin): the app, the helpers, `dictate.sh` and `prompts/`, Start menu entry, uninstaller.
- Signing: apply to SignPath Foundation (free for open source); until then, releases are unsigned and the README explains
  SmartScreen's "Run anyway" (and that Smart App Control must be off).
- `release.yml`: a Windows job that attaches `OpenVoiceType-<version>-setup.exe` (+ `.sha256`).
- README, website, `docs/TERMS.md` wording unchanged ("works with your own Claude Code").

## Reuse (don't rewrite)

- `prompts/`, `evals/` (the eval harness gains `--bin` and Windows speech synthesis for `--e2e`), and `dictate.sh` as the
  specification of `ovt-pipeline` (golden tests).
- `ovt-core`: command planning, paste-target rules, history, dictionary, rich text, settings, modes, models, updater,
  the script contract and watchdog.
- The Mac app's behaviour as the spec, and `LogicSelfTest.swift`'s cases as tests.

## Verification

- **Every phase:** CI green (macOS, Linux, Windows); `shellcheck scripts/*.sh`; `cargo fmt --check`, `clippy -D warnings`,
  `cargo test` in `linux/` and `windows/`.
- **Mac regression after any `dictate.sh` change:** `./scripts/dictate.sh selftest` and `scripts/test-dictate.sh`.
- **On the PC:** `spike.ps1`; then `OpenVoiceType.exe --logic-selftest`; a dictation into Notepad, Word, Chrome, Windows
  Terminal; an elevated window → copied; a password box → refused; swap; Command Mode; offline → S1-mini.
- **Install tests:** a fresh Windows 11 VM (Hyper-V "Quick Create") without Git, Claude or a GPU driver.

## Risks

1. **Two pipelines.** `ovt-pipeline` can drift from `dictate.sh`. The golden tests, the shared contract test and the evals
   are the guard; a pipeline change isn't done until both pass.
2. **Signing.** Smart App Control blocks unsigned apps outright. SignPath Foundation's approval isn't guaranteed.
3. **UIA gaps:** Electron apps and some terminals expose little; swap and Command Mode fall back to copy there, as on a Mac
   without Accessibility.
4. **Elevated windows** can't receive our paste without signing + uiAccess; we copy instead.
5. **Size:** about 25–35 working days after W0, less than Linux: one desktop, no extension, and `ovt-core` is done.
