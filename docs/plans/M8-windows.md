# M8: OpenVoiceType for Windows (native, full parity), Detailed Plan

**Goal:** a native Windows 11 app with the same features as the Mac app, released only once it reaches full parity. It keeps
the same privacy guarantees (audio stays local, no API key, the user's own `claude` CLI) and GPU-accelerated Whisper.

**Status legend:** ☐ to do · ◐ in progress · ☑ done

**Status: ◐ W0 started (2026-10-03).**

## Context

OpenVoiceType runs on macOS, and the Linux port (M7) is under way. The maintainer now has a Windows 11 PC. The Linux plan set
the pattern this one follows: a native app per platform, one shared pipeline (`scripts/dictate.sh`, the prompts and the
evals), and the Swift app's logic ported once to Rust in `ovt-core`, which both new apps use.

**Decisions made:**
- **Stack:** Rust. Win32 through `windows` (windows-rs) for hotkeys, input, clipboard, UI Automation and the overlay;
  `tray-icon` + `muda` for the notification-area icon and menu; Slint (Fluent style) for the Settings and setup windows.
- **Location:** the same repo, in `windows/` (a Cargo workspace). It uses `linux/crates/ovt-core` by path, so the logic and
  its tests stay in one place.
- **Pipeline:** the app runs the same `dictate.sh` with the same contract, through **Git for Windows' bash**. Git Bash
  ships bash, perl 5.42 (with `JSON::PP`, `Time::HiRes`, `IO::Socket::IP`, `POSIX`), curl, `mkfifo`, `/proc`, `timeout`
  and `sha256sum`: everything the script uses. The setup window installs Git for Windows when it's missing
  (`winget install Git.Git`).
- **Gate (W0/W1):** if the script costs more than ~300 ms extra per dictation under Git Bash (process creation is slow
  on Windows), or the fifo pre-start can't feed the native `claude.exe`, the app takes over only that part: it starts
  `claude.exe` itself and holds its stdin, and the script keeps everything else.
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
└── runs scripts/dictate.sh through Git Bash with the SAME contract as Dictation.swift (unchanged pipeline)
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
2. **Git for Windows** (the pipeline's bash and perl): found, or installed with `winget install --id Git.Git -e`.
3. Speech model download.
4. Cleanup: Claude Code installed (`irm https://claude.ai/install.ps1 | iex` in a PowerShell window) and signed in
   (`claude auth login`), or S1-mini.
5. Pin the tray icon, then try it.

## Phases (one milestone, one release; checkpoints are internal)

### ◐ W0: Shared groundwork, from the Mac and CI (no Windows PC needed)

- `dictate.sh` on Git Bash:
  - `OS` is `Windows` for `MINGW*|MSYS*|UCRT*|CLANG*`; folders as in the table (from `cygpath -u "$APPDATA"` etc.).
  - `MSYS_NO_PATHCONV=1` / `MSYS2_ARG_CONV_EXCL='*'` so arguments to native programs aren't rewritten; Windows paths
    passed to native programs through `cygpath -w` where needed.
  - `\r` stripped from native programs' output.
  - `srv_running`: `/proc/<pid>/exe` exists in MSYS; compare without `.exe`.
  - the ownership check on the state folder is skipped (`%TEMP%` is per-user; NTFS owners can be the Administrators group).
  - sounds (`SystemSounds` through PowerShell is too slow: the app plays them; the CLI stays quiet) and notifications
    (none from the CLI on Windows).
- `.gitattributes`: `*.sh`, `scripts/testdata/*`, `prompts/**` with `eol=lf` (a CRLF checkout breaks bash).
- `scripts/test-dictate.sh` passes in Git Bash on `windows-latest`, and measures the script's own overhead (`refine`
  with the fake Claude) to compare with macOS and Linux.
- A fifo test on CI: a native program (`sort.exe`) reading its stdin from an MSYS fifo, as the pre-started Claude does.
- `ovt-core` builds and tests on Windows: Unix-only parts behind `cfg(unix)`, a Windows `paths` module, the watchdog with
  `TerminateProcess`-free termination (close stdin, then kill the bash process tree).
- CI: a `windows` job (Git Bash contract test, `cargo fmt/clippy/test` for `ovt-core` and `windows/`).
- **Checkpoint:** CI green on all three; the Mac `selftest` unchanged.

### ☐ W1: Spikes on the Windows PC (half a day; they set the final scope)

`windows/spike/spike.ps1` walks through them and writes a report:
1. **Pipeline:** `dictate.sh selftest` with the real `claude.exe` and `whisper-server.exe` (Windows SAPI voice instead of
   `say`); the pre-started Claude through the fifo; the 3 s stdin rule of `claude -p`; Unicode in and out; timings.
2. **Hotkey:** hold Ctrl+Alt+Space: one press, one release; Esc only while recording; AltGr layouts.
3. **Paste:** Notepad, Word, Chrome, Edge, VS Code, Windows Terminal (Shift+Insert), an elevated Notepad (must be detected).
4. **UIA:** a password box in Edge and Chrome, selection and text before the caret in Notepad, Word, Chrome, VS Code.
5. **Clipboard:** the history (Win+V) and cloud sync don't keep our pasted text; the restore runs after the target read it.
6. **GPU:** the Vulkan whisper-server on the PC's GPU, and the CPU fallback.

### ☐ W2: End-to-end dictation (first usable build)

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

- `scripts/dictate.sh`, `prompts/`, `evals/` (the eval harness gains Windows speech synthesis for `--e2e`).
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

1. **Git Bash speed.** Every subshell costs a process on Windows. W0 measures it; the gate above limits the damage.
2. **Signing.** Smart App Control blocks unsigned apps outright. SignPath Foundation's approval isn't guaranteed.
3. **UIA gaps:** Electron apps and some terminals expose little; swap and Command Mode fall back to copy there, as on a Mac
   without Accessibility.
4. **Elevated windows** can't receive our paste without signing + uiAccess; we copy instead.
5. **Size:** about 25–35 working days after W0, less than Linux: one desktop, no extension, and `ovt-core` is done.
