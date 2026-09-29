# M7: OpenVoiceType for Linux (native, full parity), Detailed Plan

**Goal:** a native Linux app with the same features as the Mac app, released only once it reaches full parity. It keeps the
same privacy guarantees (audio stays local, no API key, the user's own `claude` CLI) and GPU-accelerated Whisper.
Requested in [issue #14](https://github.com/mahfuzur/openvoicetype/issues/14).

**Status legend:** ☐ to do · ◐ in progress · ☑ done

**Status: ◐ P0 done on the Mac side (2026-09-30), except a real dictation in the rebuilt Mac app.** `dictate.sh`,
`install.sh` and the eval harness run on Linux; `scripts/test-dictate.sh` checks the contract on both (19/19 on macOS and in
an Ubuntu 24.04 container); `linux/crates/ovt-core` has 33 passing tests; CI has a Linux job. Next: P1 on the Ubuntu PC.

## Context

Issue [#14 "Support for linux"](https://github.com/mahfuzur/openvoicetype/issues/14) asks for a Linux version. OpenVoiceType is
macOS-only today. The Swift menu-bar app records audio, handles hotkeys, pastes and draws the overlay. All the product logic
is in `scripts/dictate.sh`: Whisper, Claude/OpenAI/S1-mini cleanup, the meaning guard, post-processing and the model servers.

**Goal:** a native Linux app with the same features as the Mac app, released only once it reaches full parity. It keeps
the same privacy guarantees (audio stays local, no API key, the user's own `claude` CLI) and GPU-accelerated Whisper.

**Decisions made:**
- **Stack:** Rust + GTK4/libadwaita.
- **Location:** the same repo, in `linux/`.
- **Test machine:** the owner's Ubuntu (GNOME) PC.
- **GPU:** NVIDIA, AMD and Intel are all supported through one Vulkan build, with an automatic CPU fallback.

**What research found:**
- ~95% of `dictate.sh` already runs on Linux. The rest is five BSD-only commands that fail *silently* and a desktop layer
  of about 60 lines.
- The hard parts are desktop integration on Wayland (hotkeys, pasting, focused window, overlay). No single
  API does these on every desktop, so each desktop gets its own small backend behind common traits.

## Linux in 60 seconds (for the owner)

- **Distro:** a Linux flavour, such as Ubuntu or Fedora. **Desktop:** the shell you see, such as GNOME (Ubuntu's default)
  or KDE Plasma. **wlroots:** a library that tiling desktops like sway and Hyprland are built on.
- **Wayland vs X11:** the display systems. **Wayland** is the modern, secure one and the default everywhere; GNOME 50 removed
  X11 sessions. It deliberately stops apps from grabbing keys, typing into other apps or reading other windows. That's
  why each desktop needs its own backend.
- **Portal:** a desktop-provided D-Bus API that grants those powers with the user's consent (GlobalShortcuts, RemoteDesktop).
- **D-Bus:** the message bus that Linux apps talk over (the equivalent of macOS XPC/Apple Events).
- **AT-SPI:** Linux accessibility (the equivalent of macOS AX). It gives us password-field detection, the selected text and the caret.
- **GNOME Shell extension:** JavaScript that runs inside GNOME's own shell. It's the only way to get macOS-level integration on GNOME.
- **PipeWire / PulseAudio:** the audio system. **.deb / .rpm:** installer packages for Ubuntu/Debian and Fedora.

## Architecture

```
openvoicetype (Rust, GTK4/libadwaita, one binary)
├── ovt-core        pure logic, ported from Swift with its unit tests (no desktop deps)
├── ovt-audio       libpulse capture → 16 kHz mono WAV, levels, device list (+Bluetooth flag), hotplug
├── ovt-desktop     traits + backends chosen at runtime (XDG_CURRENT_DESKTOP / WAYLAND_DISPLAY):
│     Hotkeys · Injector (key chords) · Clipboard · FocusProvider · Overlay · Tray · Accessibility
│     ├── gnome   → our GNOME Shell extension over D-Bus (hotkeys with release, focus, clipboard,
│     │             virtual keyboard, modifier state, overlay pill, panel indicator)
│     ├── kde     → GlobalShortcuts + RemoteDesktop portals (ashpd), ext-data-control, KWin script,
│     │             gtk4-layer-shell overlay, StatusNotifierItem tray (ksni)
│     ├── wlroots → virtual-keyboard + data-control protocols, sway/Hyprland IPC, layer-shell,
│     │             hotkeys bound by the user to `openvoicetype press/release/toggle`
│     └── x11     → XGrabKey, XTest, EWMH, X selections (x11rb), best effort
│     └── atspi   → shared by all: password field, selection, caret/text before cursor, set selection
└── runs scripts/dictate.sh with the SAME contract as Dictation.swift (unchanged pipeline)

linux/gnome-extension/openvoicetype@mahfuzur.github.io/   extension.js + metadata.json (GNOME 46–50)
```

The Rust app plays the role of `AppDelegate`/`Dictation`/`Paster`/`PasteTarget`/`SelectionReader`, and `dictate.sh` stays the
single pipeline, so every prompt fix and eval reaches both platforms.

**Repo layout:** `linux/Cargo.toml` (a workspace), `linux/crates/{ovt-core,ovt-audio,ovt-desktop,openvoicetype}`,
`linux/gnome-extension/`, `linux/kwin-script/`, `linux/packaging/{deb,rpm,desktop,udev?}`, and `scripts/build-linux.sh`.

**Key crates:**
- UI: `gtk4` (feature `v4_14`), `libadwaita` (`v1_5`), `gtk4-layer-shell`.
- D-Bus and desktop: `zbus`, `ashpd`, `atspi`, `ksni`, `wayland-client` + `wayland-protocols(-misc)`, `x11rb`.
- Audio and secrets: `libpulse-binding`, `hound`, `oo7` (Secret Service).
- Downloads and data: `ureq` + `sha2`, `serde` / `serde_json`.

## Mac → Linux parity map

| Mac feature (file) | Linux, GNOME (main target) | KDE / wlroots / X11 | Known limit |
|---|---|---|---|
| Global hotkey with press/release, Esc while recording (`HotKey.swift`) | Extension `grab_accelerator` + `accelerator-deactivated` | Portal (KDE, needs Plasma 6.5+) / CLI verbs bound in compositor / XGrabKey | none |
| Hold to Talk | Extension release signal | Portal `Deactivated` / `bindsym --release` → `openvoicetype release` / X11 KeyRelease | none |
| ⌘V paste + clipboard snapshot/restore, transient, HTML (`Paster.swift`) | Extension `St.Clipboard` (all MIME types) + virtual keyboard Ctrl+V; **Ctrl+Shift+V in terminals** | ext-data-control (restore on the `send` event, as on macOS) + portal keysym / virtual keyboard / XTest | GNOME: restore after a fixed delay (no read signal) |
| Wait for ⌃⌥⇧⌘ release before pasting | Extension modifier mask | Portal release signal / compositor / XQueryKeymap | none |
| Layout-aware key code (`KeyboardLayout`) | Keysym-based chords (layout independent) | same | none |
| PasteTarget: pid, window, title, `normalized()` | Extension focus info (pid, app id, stable window id, title) | KWin script / sway-Hyprland IPC / EWMH | none |
| Password field refused | AT-SPI `ROLE_PASSWORD_TEXT` (+ spike: text-input content purpose via extension) | AT-SPI | Chrome/Electron report "unknown" unless run with `--force-renderer-accessibility`. Same policy as macOS: refuse only when known |
| Swap ⌃⌥Z (undo + paste other version, "still there" check) | AT-SPI text before caret; otherwise the 30 s rule | same | Never ⌃Z in terminals (the Mac already skips code mode) |
| Command Mode selection read (`SelectionReader.swift`) | AT-SPI selection → Ctrl+C with marker + restore (same algorithm, same line-copy guard) → **PRIMARY for terminals** | same | No "press Edit ▸ Copy via AX"; Ctrl+C is the fallback, **never in terminals** |
| Re-select last dictation, Restore Original | AT-SPI `setSelection` / caret | same | Apps without AT-SPI text: falls back to copy, as macOS does when AX is missing |
| Overlay pill (`Overlay.swift`) | Drawn by the extension (St widgets), same phases, colours and timings | gtk4-layer-shell / layer-shell / X11 always-on-top window | none |
| Menu-bar icon + menu (`MenuBarIcon.swift`, `AppDelegate.swift`) | Extension panel indicator + menu (same items) | ksni tray / ksni (waybar) / ksni | none |
| Settings (6 panes), Setup, Model manager, Updater, Claude detection | libadwaita PreferencesWindow + setup window; same logic | same | The setup steps change (below) |
| Keychain API keys (`APIKeychain.swift`) | Secret Service (GNOME Keyring), same service name | KWallet | none |
| Launch at login (SMAppService) | `~/.config/autostart/*.desktop` | same | none |
| Mic via AVAudioEngine, Bluetooth retry, watchdog | libpulse (works on PipeWire), `device.bus=bluetooth`, same retry/watchdog | same | Warn that Bluetooth mics drop to call quality |
| Not needed on Linux | Migration, Move to Applications, BundledHelpers de-quarantine, TCC Reset, Metal warm-up | – | – |

**Linux setup window steps:**
1. Microphone check: plain Linux apps need no permission, so this just checks that sound arrives.
2. **Desktop integration:** GNOME: enable the extension, which needs one log-out. KDE: approve the portal dialogs.
   sway/Hyprland: copy-paste bindings. This replaces the macOS Accessibility step.
3. Speech model.
4. Cleanup: Claude installed and signed in through a terminal (`ptyxis`/`kgx`/`gnome-terminal`/`konsole`/`x-terminal-emulator`),
   or S1-mini.
5. Try it.

## Phases (one milestone, one release; checkpoints are internal)

### ◐ P0: Shared groundwork, on the Mac (no Linux PC needed)

**Done (2026-09-30), with these changes from the plan:**
- ☑ The online check on Linux connects from perl (`IO::Socket::IP`, alarm caught): bash `/dev/tcp` under a perl alarm made bash
  print "Alarm clock" on a dead host. macOS keeps `nc -z -G 1` unchanged.
- ☑ `srv_running` also strips the " (deleted)" `/proc` adds after a package upgrade, so the old server is restarted, not
  started twice on one port.
- ☑ `dictate.sh` refuses a state folder owned by someone else (possible under a shared `/tmp` without `XDG_RUNTIME_DIR`), and
  finds the Linux package's helpers in `/usr/lib/openvoicetype/bin` (last on `PATH`).
- ☑ `scripts/test-dictate.sh` (new, in CI on both platforms): the refine contract with `scripts/testdata/fake-claude`, a usage
  limit, offline, the online check, the server pid check and log rotation, in a temporary HOME.
- ☑ `ovt-core`: settings are `~/.config/voice-to-text/settings.json` (not `app.json`); hotkeys are GTK accelerator strings, and
  a global one needs Alt or Super (Ctrl is Linux's app-shortcut key, as ⌘ is on macOS).
- ☐ Moved to P2: the dictation state machine and the overlay message strings and durations (they belong with the recorder
  and the UI).
- ☐ A real dictation, swap and command in the Mac app rebuilt with the new `dictate.sh` (`./scripts/build-app.sh --install`).
- Evals on the Mac after the changes: 23/25 (92%) cleanup, 19/20 (95%) Command Mode. `postgres-bluetooth` and `shorter-polite`
  failed on 2026-09-25 too; `injection-roleplay` passes 2 of 3 runs (Haiku sometimes drops the "talk like a pirate" sentence).

**The plan:**
- Copy this plan into the repo as `docs/plans/M7-linux.md`, and add an M7 row and section to `docs/ROADMAP.md`.
- **`dictate.sh` portability.** One code path where possible, a `uname` switch only for the desktop layer:
  - `is_offline`: `route -n get default` → `ip route show default` on Linux; `nc -z -G 1` → bash `/dev/tcp` under a perl alarm
    (works on both). **Without this, Linux always looks offline.**
  - `stat -f %z` / `stat -L -f '%z-%m'` (`log_maintain`, `claude_options`, `srv_signature`) → perl `stat` (both). **Without
    this, the servers restart on every dictation.**
  - `format_resets`: `date -r` → perl `POSIX::strftime` (both). **Without this, `refine` dies on a rate limit.**
  - `STATE_DIR` → `${XDG_RUNTIME_DIR}/voice-to-text` on Linux (keep `$TMPDIR` on the Mac); `LOG_DIR` → `${XDG_STATE_HOME:-~/.local/state}/voice-to-text`;
    honour `XDG_CONFIG_HOME`/`XDG_DATA_HOME`.
  - `srv_running`: on Linux compare `readlink /proc/$pid/exe` (ps `comm` is cut to 15 characters).
  - `sound` → `pw-play`/`paplay` with freedesktop sounds; `notify` → `notify-send`; `selftest` → `espeak-ng` + sox resample; CLI
    `paste_text` → `wl-copy` + `wtype`/`xdotool` when present, otherwise copy only (the app does its own pasting); `/bin/bash` → `$BASH`.
  - `install.sh`: apt/dnf hints, `sha256sum` fallback. `config.example.sh`: wording.
- `evals/run.py`: `synthesize()` uses `espeak-ng` when `say` is missing.
- **`ovt-core` crate** with unit tests, ported from Swift:
  - Settings keys and defaults (`AppSettings.swift`, stored as `~/.config/voice-to-text/settings.json`).
  - The `ScriptRun` contract, env and exit codes 0/3/4/5, and result-file decoding (`Dictation.swift:412-544`).
  - The state machine; `CommandPlanner`, `CommandSession` and `CommandPlan` (`CommandMode.swift`).
  - `PasteTarget.normalized`, `DictationHistory`, `DictionaryFile`, `RichText`, the `Modes` mapping (with Linux app ids / WM_CLASS
    tables), the `ModelManager` catalog (same URLs and SHA-256), `Updater.isNewer`, `APIKeychain.account`, `looksLikeLineCopy`, and
    the overlay message strings and durations.
  - Test vectors copied from `LogicSelfTest.swift`.
- **Checkpoint:** `shellcheck scripts/*.sh`; the Mac `selftest`; `evals/run.py` ≥ 90% on Haiku; `build-app.sh --install`
  plus a real dictation (the macOS behaviour is unchanged); `cargo test` passes.

### ☐ P1: Five one-day spikes on the Ubuntu PC (these set the final scope)
1. **GNOME extension (~200 lines):**
   - D-Bus `Focus`, `Grab/Ungrab`, `GetClipboard/SetClipboard(mime)`, `Key(chord)`, `Modifiers`, driven with `busctl`.
   - Check hold-to-talk release and the Esc grab.
   - Paste rich text into GNOME Text Editor, Chrome and a terminal.
   - Log out and back in.
   - Check whether text-input content purpose reveals password fields.
2. **KDE path** (Fedora KDE live USB, Plasma 6.5+): portal shortcut press/release, `NotifyKeyboardKeysym` with a persisted token
   that survives a reboot, clipboard snapshot/restore over ext-data-control, and a KWin script reporting the active window.
3. **AT-SPI coverage table:** role/state, selection and text before the caret in Firefox, Chrome (with and without the flag),
   VS Code, LibreOffice, Kate, a JetBrains IDE, GNOME Terminal and kitty. This decides the "unknown" fallbacks.
4. **Injection correctness:** Ctrl+V while Alt+Shift are still held, AZERTY/Dvorak, Ctrl+Shift+V in terminals.
5. **Whisper build and speed:** `build-deps.sh` for Linux in an Ubuntu 24.04 container, then time a 10 s clip on the owner's GPU
   vs CPU. Remove the Vulkan driver and check that it falls back to CPU.
- **Checkpoint:** the results are recorded in `docs/research/2026-xx-xx-linux-spikes.md`, and the parity map is updated where
  a spike changed it.

### ☐ P2: GNOME end-to-end dictation
- `scripts/build-deps.sh`, Linux branch:
  - No Metal; `GGML_VULKAN=ON` + `GGML_BACKEND_DL=ON` + `GGML_CPU_ALL_VARIANTS=ON` (shared libraries; the Vulkan module is
    skipped when no driver is present); `nproc`; an `ldd` allow-list instead of `otool`.
  - whisper and llama in separate folders (each vendors its own ggml), RPATH `$ORIGIN`, and a `bin/` folder of symlinks
    passed as `VTT_BIN_DIR`.
- The extension (full version, D-Bus API versioned, caller checked through `/proc/<pid>/exe`).
- The Rust app: GApplication single instance with actions `toggle/press/release/command/swap/cancel/settings` (also as
  `openvoicetype <verb>`); recording; running `dictate.sh` with the pre-start; overlay and indicator; paste through the
  extension; sounds; `APP …` log lines.
- **Checkpoint:** the owner uses it for daily dictation on Ubuntu; `APP TIMING` is comparable to the Mac's.

### ☐ P3: Safety and history
- Paste-target capture/check (pid, window id, normalized title; code-mode apps skip the title check), password refusal,
  "Copied: you switched to X", the clipboard snapshot with a restore generation counter and `settlePending`, rich paste
  (text/html), swap with the "still there" check, Recent Dictations and Copy Last.
- **Checkpoint:** the Linux `--logic-selftest` passes, and a manual test checklist copied from the M5.4 plan passes.

### ☐ P4: Command Mode
- A Linux `SelectionReader`: AT-SPI, then Ctrl+C with a marker (0.3 s poll, restore, re-check for late writers), then PRIMARY
  for terminals.
- `CommandPlanner` target chip on the overlay; follow-ups; re-select last dictation (AT-SPI, with `restoreCursor`); Restore
  Original; `history.invalidateLast()`.
- **Checkpoint:** `evals/run.py --command` (shared) and the Command Mode manual checklist in the apps from the P1 coverage table.

### ☐ P5: Settings, setup and the rest
- libadwaita Settings with the same 6 panes and captions:
  - Modes: "Add App…" lists installed `.desktop` apps.
  - Speech: model rows; "Show in Files".
  - Cleanup: API presets, `/models` fetch, Test Cleanup.
  - About: updater, licenses, log toggle, Open Log.
- The setup window (steps above), `ModelManager` downloads (resume, 3 retries, SHA-256), `ClaudeCLI` find/status/install in a
  terminal, Secret Service keys, autostart, microphone menu and Test Microphone, Bluetooth tip.
- `--overlay-snapshots` / `--settings-snapshots` renderers.
- **Checkpoint:** a fresh Ubuntu user account completes setup with no terminal commands.

### ☐ P6: Other desktops
- The KDE backend (portals, ext-data-control, KWin script, layer-shell overlay, ksni tray).
- wlroots (virtual keyboard, data-control, sway/Hyprland IPC, layer-shell; documented `bindsym`/`bindr` lines).
- X11 best effort (Ubuntu 24.04 "on Xorg").
- **Checkpoint:** a dictation, swap and command on Fedora KDE (Plasma 6.5+), sway and Ubuntu on Xorg.

### ☐ P7: Packaging, CI, docs, release
- `.deb` + `.rpm` (via `cargo-deb` / `cargo-generate-rpm`), built in an Ubuntu 24.04 container. They install
  `/usr/bin/openvoicetype`, `/usr/lib/openvoicetype/{whisper,llama,bin}`, `/usr/share/openvoicetype/{dictate.sh,prompts/}`
  (prompts stay next to the script), the `.desktop` file with the reverse-DNS id (needed by the portal), icons, the GNOME
  extension under `/usr/share/gnome-shell/extensions/`, and ThirdPartyLicenses. Dependencies: `perl`, `curl`, `sox`,
  `libpulse0`, `libgtk-4-1`, `libadwaita-1-0`, `libvulkan1`.
- No Flatpak/AppImage: both are sandboxed or self-contained in ways that block the host `claude` CLI and installing the
  extension. Don't publish the extension on extensions.gnome.org; it ships inside the package.
- **CI:** an `ubuntu-24.04` job in `ci.yml` (shellcheck, `cargo fmt/clippy/test`, cached Linux deps build, `.deb`/`.rpm`,
  `--logic-selftest`). `release.yml` gains a Linux job that attaches the `.deb`/`.rpm` (+ `.sha256`) to the same `v*`
  release as the DMG. `release-notes.md` gets Linux install lines.
- **Docs** (keep the numbers in sync):
  - `README.md`: a Linux section and requirements (Ubuntu 24.04+/Debian 13/Fedora 43+, GNOME 46–50, Plasma 6.5+).
  - `docs/GUIDE.md`: a Linux chapter. `docs/index.html`: "Mac and Linux". `CONTRIBUTING.md`: Linux dev setup.
  - `CLAUDE.md`: a Linux architecture section with gotchas. `docs/ROADMAP.md`: status.
  - Reply to issue #14 and ask the reporter to test a pre-release.
- **Release:** the next minor version, with the macOS and Linux assets in one release.

### Later (not needed for parity)
- An optional `openvoicetype-cuda` package that drops `libggml-cuda.so` next to the others (the backend loader picks it up),
  only if P1 shows Vulkan clearly slower than CUDA on NVIDIA.
- Wider arm64 Linux support; an AUR package.

## Reuse (don't rewrite)

- **The whole pipeline in `scripts/dictate.sh`:** `transcribe`, `refine`, `command`, `whisper-server`/`s1-server` (`srv_*`),
  `claude_prestart`/`claude_send`, `meaning_guard`, `post_process`, the result file.
- **`prompts/`, `evals/`**, and the model URLs/SHA-256 pins (`scripts/install.sh:21-31`, `ModelManager.swift:22-56`).
- **Behaviour to port 1:1 from Swift** (the spec lives in these files):
  - `Dictation.swift` (the contract, env, pre-start, watchdogs 45/75/120 s).
  - `AppDelegate.swift` `deliver`/`deliverCommand`/`swapLastPaste`/`hotKeyReleased` and the menu structure.
  - `CommandMode.swift`, `PasteTarget.normalized`, `Paster` restore generations, and `SelectionReader` marker/restore and the
    line-copy guard.
  - `AppSettings.swift` keys, `LogicSelfTest.swift` vectors, `Overlay.swift` phases/colours/timings.

## Verification

- **Every phase:** `shellcheck scripts/*.sh`; `cargo fmt --check && cargo clippy -- -D warnings && cargo test` in `linux/`.
- **Mac regression after any `dictate.sh` change:** `./scripts/dictate.sh selftest`, `evals/run.py` (≥ 90% Haiku),
  `evals/run.py --command`, `./scripts/build-app.sh --install` and a real dictation, swap and command.
- **On Ubuntu:**
  - `./scripts/dictate.sh selftest` (espeak-ng).
  - `evals/run.py --e2e --timing --jobs 1` (compare with the Mac's ~2.5 s).
  - `openvoicetype --logic-selftest`, `--recorder-selftest`.
  - Offline test (Wi-Fi on, internet cut) → S1-mini fallback.
  - The manual checklists (paste into a changed window → copied; password field → refused; terminal paste uses Ctrl+Shift+V;
    swap; Command Mode in each app from the coverage table).
- **Install tests:** the `.deb` on fresh Ubuntu 24.04 and 26.04 VMs/live USBs; the `.rpm` on Fedora 43/44 (GNOME and KDE);
  remove the Vulkan driver → CPU fallback still transcribes.

## Risks

1. **GNOME extension API churn:** each GNOME release (every 6 months) may need a metadata/API update. Mitigation: a thin
   extension, a versioned D-Bus API, and an app that works with an older extension.
2. **The first extension enable needs a log-out on Wayland.** The setup window explains it.
3. **AT-SPI gaps (Chrome/Electron, kitty):** Swap and Command Mode fall back to copy there, as on a Mac without Accessibility.
   The docs list which apps are covered.
4. **KDE needs Plasma 6.5+** for persistent consent; Kubuntu 24.04 (Plasma 5.27) is unsupported.
5. **Size:** a full-parity single release is large. Estimate **35–50 working days**; P2 is the first point where it's
   usable daily.
