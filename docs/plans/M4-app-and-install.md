# M4: Settings Window, First-Run Setup and DMG, Detailed Plan

**Goal:** someone with a new Mac downloads a DMG, drags the app to Applications, and is dictating within a few minutes
with no Homebrew and no Terminal commands. After that, everything is set in a proper Settings window instead of a long menu.

**Status legend:** ☐ to do · ◐ in progress · ☑ done

## 1. Scope decisions (2026-09-24)

| Question | Decision |
|---|---|
| Apple Developer account ($99 a year) | **Plan both, decide later.** Ship a DMG without Apple's signature first (signed with our own release certificate so that permissions survive updates, §3G). Developer ID signing and notarization are a switch in the release script, turned on when the account exists (roadmap decision 3 stays open). |
| A Mac with no `claude` CLI | **Offer to install it, and use S1-mini meanwhile.** Setup finds `claude`. If it's missing, one button runs Anthropic's official installer in Terminal, then asks you to sign in. Dictation works straight away with S1-mini. |
| Cleanup providers | **Claude + S1-mini only.** Codex, Gemini, Ollama and API keys move to a later milestone (M6). The Cleanup pane is laid out so that they can be added. |
| Whisper model for new users | **They choose; the default is the compressed large-v3-turbo** (574 MB). The full 1.6 GB model and a small, fast one are also offered, and it can be changed in Settings later. |

Out of scope: more providers (M6), automatic in-place updates (Sparkle), a Homebrew cask, editing mode prompts, and Command Mode.

## 2. What a new Mac is missing today

Checked on 2026-09-24 against a working install (macOS 15.6, whisper.cpp 1.9.1, Claude Code 2.1.280):

| Needed by | Today it comes from | On a new Mac | Plan |
|---|---|---|---|
| `whisper-server`, `whisper-cli` | Homebrew `whisper-cpp` | Missing. The Homebrew binaries also link libraries in `/opt/homebrew` (`libwhisper`, `libggml`), so they can't just be copied. | Build **static** binaries with Metal embedded and ship them inside the app (§3A) |
| `llama-server` (S1-mini) | Homebrew `llama.cpp` | Missing. It also links Homebrew's OpenSSL. | Static build without OpenSSL, inside the app |
| `soxi` (recording length) | Homebrew `sox` | **Missing, and this is a blocker.** `transcribe_wav()` falls back to a length of 0, so **every dictation is skipped as too short**. | Read the length from the WAV header in Perl (§3E) |
| `sox` (self-test) | Homebrew `sox` | Missing | `say --data-format=LEI16@16000` writes the 16 kHz WAV directly |
| `sox`/`rec` (recording, CLI only) | Homebrew | Missing | Not needed. The app records in Swift. Only the CLI `dictate start` still uses it. |
| Whisper model (1.6 GB) | `install.sh` | Missing | Downloaded during setup, with progress (§3C) |
| S1-mini model (484 MB) | `install.sh` | Missing | Offered during setup with its own Download button (optional) |
| `claude` CLI, signed in | The user | Usually missing | Found, or installed with the official installer; sign-in checked with `claude auth status` (§3D) |
| `bash` 3.2, `perl`, `curl`, `nc`, `route`, `say`, `osascript` | macOS | Present | No change |
| Microphone and Accessibility permissions | Granted by hand | Not granted | Guided in setup (§3B) |
| Config, dictionary | `install.sh` copies `config.example.sh` | Missing | Not needed: the app passes settings as `VTT_*` variables, and the dictionary is created by the Dictionary pane |

The bundled binaries are small: about 1 MB for `whisper-server` and 19 MB for all of Homebrew's llama.cpp. The DMG should
be **about 20–30 MB**. The models are downloaded on first run and aren't in the DMG.

## 3. Design

### A. A self-contained app bundle

```
VoiceToText.app/Contents/
  MacOS/VoiceToText
  Helpers/whisper-server  whisper-cli  llama-server     (static, Metal embedded, signed)
  Resources/dictate.sh  prompts/  ThirdPartyLicenses/   (whisper.cpp, llama.cpp, ggml: MIT)
```

- `scripts/build-deps.sh` builds whisper.cpp and llama.cpp at **pinned tags**, using CMake with `BUILD_SHARED_LIBS=OFF`,
  `GGML_METAL=ON` and `GGML_METAL_EMBED_LIBRARY=ON`. The Metal shaders are compiled at runtime, so the Metal compiler from
  Xcode isn't needed and Command Line Tools stay enough. It uses `LLAMA_CURL=OFF` and no OpenSSL. The only new build
  tool is `cmake` (Homebrew locally, preinstalled on CI). The output is cached in `app/build/deps/<tag>/`.
- The check is `otool -L` on each helper: only `/usr/lib` and `/System` may appear.
- `build-app.sh` copies the helpers into `Contents/Helpers` and signs them before the app itself.
- The app sets `VTT_BIN_DIR=<bundle>/Contents/Helpers`, and `dictate.sh` puts it **first** on `PATH`. The bundled
  versions always win, and CLI users with Homebrew are unaffected.
- **Paths stay the same:** `~/.local/share/whisper/`, `~/.local/share/s1-mini/` and `~/.config/voice-to-text/`. The CLI and the app
  share models and the dictionary, existing installs keep working, and the app passes `WHISPER_MODEL` explicitly.

### B. First-run setup (onboarding)

A window with six steps. It opens on first launch, and again from the menu with "Set Up…". Each step shows ✓ once it's done,
so running it again just confirms everything.

1. **Move to Applications.** If the app runs from the DMG, Downloads or an App Translocation path, it offers to copy itself to
   `/Applications` and relaunch. Permissions are tied to the location, and a translocated app loses them.
2. **Microphone:** request access (`AVCaptureDevice.requestAccess`), with a button to open System Settings if it was denied.
3. **Accessibility:** explain why (pasting), open the Accessibility pane, and check `AXIsProcessTrusted()` every second until it's on.
   There's a "Paste stopped working?" helper for the stale-entry case after an update: remove the entry, then add it again.
4. **Speech model:** Compressed (574 MB, recommended), Full (1.6 GB) or Fast (147 MB, `base.en`, less accurate). An existing
   model file is detected and used. The download runs in the background, so the next steps can go on meanwhile.
5. **Cleanup:**
   - `claude` found and signed in → ✓ "Claude (your subscription)".
   - Found but not signed in → a **Sign In** button (`claude auth login`).
   - Missing → an **Install Claude Code** button that runs Anthropic's official installer visibly in Terminal (the app writes a
     `.command` file and opens it, so no Automation permission is needed), then signs in. A **Check Again** button re-checks.
   - S1-mini (484 MB), offered with its own Download button. If Claude isn't ready yet and S1-mini is installed, it's
     selected so dictation works now.
6. **Try it:** shows the hotkey (and lets you change it) and a text box to dictate into. The result shows which engine cleaned it and how long it took.

### C. Model manager

- `ModelManager.swift`: a list of known models (name, URL, size, SHA-256), and `URLSession` download tasks with progress, a `.part`
  file, resume after a network drop, and a SHA-256 check before the final rename. Hugging Face serves LFS files with the
  SHA-256 as the ETag, so the check needs no extra download.
- Used by both setup and the Speech pane. The Speech pane shows the installed models with their size, and has **Use**, **Download** and **Delete** buttons.
- Before making the compressed model the default, check it against the full one on the e2e eval (spike S4).

### D. Finding and checking the Claude CLI

- `ClaudeCLI.swift`: resolve `claude` through the login shell (`zsh -lc 'command -v claude'`), then check the known locations
  (`~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`). The result is passed to `dictate.sh` as `CLAUDE_BIN`, so an npm or nvm
  install also works.
- Status: `claude auth status --json` gives `loggedIn`, `authMethod` and `subscriptionType`. The app shows "Signed in (team)" and
  never reads or stores the login details.
- Install: the official native installer, run in Terminal so that the user sees it.
- Check the version (`claude --version`) and show a clear error if it's older than the version whose stream-json format we tested.

### E. Script changes (`dictate.sh`)

- **Recording length without `soxi`:** read the `data` chunk size and byte rate from the WAV header in Perl. It keeps
  `soxi` as a fallback for WAVs that aren't plain PCM, and has a test with the app's own WAVs.
- Put `VTT_BIN_DIR` first on `PATH`, and read `CLAUDE_BIN` from the environment (already a setting).
- `selftest`: use `say --data-format=LEI16@16000 -o x.wav` instead of `say` then `sox`.
- `install.sh` stays for CLI users. It no longer needs `sox` for the app, only for `dictate start`.

### F. Settings window

An `NSWindow` with an `NSTabViewController` in toolbar style (the standard macOS settings look), with one SwiftUI view per pane in
an `NSHostingController`. This works on macOS 13 with Swift 5.9 and Command Line Tools. The SwiftUI `Settings` scene needs the
SwiftUI app lifecycle, which this app doesn't use.

| Pane | Contents |
|---|---|
| **General** | Hotkey recorder (any key combination), Press to Start and Stop or Hold to Talk, microphone, overlay on/off and position, sounds, launch at login, auto-paste, rich paste |
| **Speech** | Model manager (§3C), vocabulary (terms Whisper and Claude should spell correctly) |
| **Cleanup** | Clean up text on/off. Engine: Claude or S1-mini. Claude model (Haiku or Sonnet) and CLI status (path, version, signed-in account, Install and Sign In buttons). S1-mini: installed or download, use it when Claude is unavailable. A **Test** button that runs a sample transcript and shows the result, the engine used and the time. |
| **Dictionary** | A table of replacements ("cloud" → "Claude"), written to `dictionary.txt`, with add and remove |
| **Modes** | App → mode mapping: add the frontmost app or pick one from Applications. Prompts stay as they are (editing them is later). |
| **About** | Version, update check, credits ("S1-mini by Superwhisper", whisper.cpp, llama.cpp), licenses, log folder |

- **One settings store:** move the UserDefaults-backed properties out of `AppDelegate` (723 lines) into `AppSettings: ObservableObject`
  with `@Published` properties. The menu, the panes and `Dictation` all read it, so a change anywhere shows everywhere. The `@Observable`
  macro needs macOS 14, so it isn't used.
- **Hotkey recorder:** a small `NSView` that captures the next key combination (with at least one modifier, or a function key),
  shows it as ⌃⌥Space, and stores `keyCode` and `modifiers`. The current Carbon hotkey is unregistered while it records.
  The old `hotKeyIndex` preset is migrated.
- **The menu becomes short:** status, Start Dictation, Mode ▸, Microphone ▸ (quick switching stays handy), Clean Up With ▸
  (Claude or S1-mini), Settings… (⌘,), Set Up…, and Quit.
- An accessory app has to call `NSApp.activate(ignoringOtherApps:)` when it opens a window, so the window comes to the front.

### G. Signing, DMG and releases

Three levels. The release script picks the highest one whose secrets are available:

| Level | How it's signed | First launch after download | Permissions after an update |
|---|---|---|---|
| 0. Ad-hoc | `codesign -s -` | System Settings → Privacy & Security → **Open Anyway** (on macOS 15 a right-click → Open no longer skips the warning) | **Lost** (the identity is the cdhash) |
| 1. **Release certificate** (the default for now) | A self-signed "Voice to Text Release" certificate, kept as a CI secret, the same way `setup-signing.sh` does locally | Open Anyway, once | **Kept**: the designated requirement is identifier + certificate, which doesn't change between versions (to confirm in spike S2) |
| 2. Developer ID + notarized | Apple Developer account, hardened runtime, audio-input entitlement, `notarytool` + `stapler` | Double-click, no warning | Kept. Switching from level 1 to 2 changes the certificate, so users grant permissions once more. |

- **DMG:** built with `dmgbuild` (Python, installed into a virtualenv by `release.sh`): a 660 × 400 window with a background
  and an arrow, "Voice to Text.app" on the left and an `Applications` link on the right, big icons and no toolbar or sidebar.
  It writes Finder's `.DS_Store` directly, so it needs no Finder scripting and works in CI (plain `hdiutil` is the fallback).
  The app icon and the background come from `scripts/make-artwork.swift` (original drawing; SF Symbols can't be used in app icons).
  The release notes and README explain the Open Anyway step with a screenshot while there's no notarization.
- **Release workflow** (`.github/workflows/release.yml`) on a `v*` tag: build the dependencies (cached), build and sign the app, create the DMG,
  notarize if the Apple secrets exist, and attach the DMG and its SHA-256 to a GitHub Release. CI keeps building on every PR as now.
- **Update check:** once a day, the GitHub Releases API (`/repos/…/releases/latest`). If there's a newer version, the menu shows
  "Update Available (v0.2)…", which opens the release page. Updating is dragging the new app over the old one. Sparkle comes later.
- The version comes from the tag and is written into `CFBundleShortVersionString` and `CFBundleVersion` by `build-app.sh`.

## 4. Spikes (before building)

These decide parts of the design, so they're done first, like the M3 spike:

| ID | Question | Pass condition |
|---|---|---|
| S1 | Can whisper.cpp and llama.cpp be built static, with Metal embedded, using Command Line Tools + `cmake`? | `otool -L` shows only system libraries; speed matches the Homebrew build (whisper-server 0.8–0.9 s, S1-mini warm 0.1–0.3 s); S1-mini's `--jinja --chat-template-kwargs` still works at the pinned tag |
| S2 | Does a DMG signed with a self-signed certificate behave on macOS 15 when downloaded from GitHub (quarantined)? | Open Anyway works once, the helpers launch from inside the bundle, and after installing a newer build signed with the **same** certificate, Microphone and Accessibility still work |
| S3 | Can the app run `claude auth login` itself, or does it need Terminal? | Sign-in finishes, and `claude auth status --json` then shows `loggedIn: true` |
| S4 | Is the compressed large-v3-turbo (q5_0) as good as the full model? | Same pass rate on `evals/run.py --e2e`, similar speed, lower memory |

**Where to test a clean install:** a macOS 15 virtual machine (for example [Tart](https://tart.run), free for personal use, built on Apple's
Virtualization framework), with no Homebrew, no `claude` and no models. A second user account on this Mac isn't enough, because it
still sees `/opt/homebrew`. Paste and the microphone are checked on real hardware.

## 4b. Spike results (2026-09-24)

| ID | Result |
|---|---|
| S1 | **Pass.** `build-deps.sh` builds all three in about 2 minutes with Command Line Tools + `cmake`; only system libraries are linked. Speed matches Homebrew: whisper-server 0.81–0.84 s per request, S1-mini 0.08–0.09 s warm, same transcripts and output. **But** the first launch compiles the Metal shaders: 10 s (Whisper) and 19 s (llama.cpp). macOS then caches them (0.3–0.6 s, faster than Homebrew's 0.8–1.1 s), per binary, so every re-signed build pays it once. Handled by warming both servers after a download or an update, and a 30 s start timeout. Two build notes: Homebrew's `ccache` was broken on this Mac (`GGML_CCACHE=OFF`), and Accelerate's BLAS needs macOS 13.3, so the minimum version is now 13.3. The Command Line Tools SDK here is 14.2, which doesn't declare one Metal method ggml uses for multi-GPU events; it isn't used with one GPU, but test on macOS 13/14 (M4.11). |
| S2 | **Found a blocker in the release review (§5d), then fixed.** The self-signed release certificate from CI secrets works: the designated requirement is `identifier "io.github.mahfuzur.voicetotext" and certificate leaf = H"…"`, which doesn't depend on the build, so permission grants should survive updates. Gatekeeper rejects the app (`spctl`: rejected, origin=Voice to Text Release), as expected without notarization. Still to do on a clean Mac: download from GitHub, Open Anyway, and an update keeping the permissions. |
| S3 | **Decided without a live test:** `claude auth login --claudeai` opens a browser and may ask to paste a code, so it runs in Terminal (a `.command` file). It wasn't run here because it would change this Mac's login. `claude auth status --json` works (`loggedIn`, `subscriptionType`). |
| S4 | **Pass.** e2e eval, 2 runs each: full 10/12, compressed 10/12, the same failing case (email addresses), with near-identical transcripts ("March 3rd" vs "March 3"). whisper-server 0.84 s either way, **750 MB** of memory instead of 1.7 GB. The compressed model is the default for new users; existing installs keep the full one. |

## 5. Tasks

| ID | Task | Status |
|---|---|---|
| M4.1 | Spikes S1–S4; record the results in §4b | ◐ S1 and S4 pass; S2 and S3 half done, the rest needs a real download (M4.11). See §4b |
| M4.2 | `build-deps.sh`: static `whisper-server`, `whisper-cli` and `llama-server` at pinned tags; `build-app.sh` bundles and signs them in `Contents/Helpers`; `VTT_BIN_DIR` first on `PATH` | ☑ DMG 11 MB; helpers 3–18 MB each |
| M4.3 | `dictate.sh` without Homebrew: WAV length without `soxi`, `selftest` without `sox`, `CLAUDE_BIN` from the app; shellcheck; eval unchanged | ☑ Also: a server restarts when its model changes |
| M4.4 | `AppSettings` store; `AppDelegate` and `Dictation` read it; migrate the hotkey preset | ☑ |
| M4.5 | Settings window: General (with the hotkey recorder), Speech, Cleanup (with Test), Dictionary, Modes (app mapping), About | ☑ Vocabulary is in the Dictionary pane (the script already merges dictionary terms into the vocabulary) |
| M4.6 | `ModelManager`: download with progress, resume and SHA-256; Use and Delete; default to the compressed model (if S4 passes) | ☑ Download, progress and the checksum check tested, and a real download from Settings (Fast model); resume not tested on a real dropped connection |
| M4.7 | `ClaudeCLI`: find it, version, `auth status`, Install (official installer in Terminal), Sign In | ☑ Found, version and signed-in state tested; Install and Sign In not run (they'd change this Mac's login) |
| M4.8 | First-run setup: move to Applications, microphone, Accessibility, model, cleanup, try it | ☑ Built and rendered; not yet run on a clean Mac |
| M4.9 | Short menu; "Update Available" check against GitHub Releases | ☑ |
| M4.10 | Release: versioning from the tag, DMG, release certificate signing, optional notarization, `release.yml`, third-party licenses | ☑ `release.sh` tested locally with both the local identity and a throwaway release certificate; `release.yml` runs on the first tag |
| M4.11 | Clean-install test in a VM: DMG → Open Anyway → setup → dictation with S1-mini → install Claude → dictation with Claude → update to a new build keeps permissions | ◐ The release DMG built with `release.sh` and the release certificate installs and works on the development Mac; still to do: a real download on a second Mac |
| M4.12 | Docs: README (download and install, Open Anyway, what's downloaded and why), CLAUDE.md, ROADMAP, CHANGELOG, overlay and setup screenshots | ☑ Plus a README logo header and screenshots, and [docs/ARTWORK.md](../ARTWORK.md) |
| M4.13 | Artwork (added in review): app icon, DMG window, menu-bar icon | ☑ See §5c |

**Checked after building:** shellcheck; clean build with no warnings; `codesign --verify --strict`; `dictate.sh selftest` with only
the bundled helpers on `PATH` (2.4 s warm); `evals/run.py` 20/20 on Haiku; the installed app starts its bundled servers with the
chosen model; Settings and setup rendered with `--settings-snapshots`; the model download with a good and a bad checksum.

**Estimate:** about 7–9 days. Build and signing (M4.2, M4.10, M4.11) are about 3 days, the Settings window and store about 2.5, and setup, models and the Claude CLI about 2.5.

## 5c. Added after the first build (review, 2026-09-24)

Trying the first build led to these changes; the design is documented in [docs/ARTWORK.md](../ARTWORK.md).

| Change | Why |
|---|---|
| **App icon:** a waveform in the overlay's blue and violet above two lines of text, on a dark rounded square, drawn in code (`scripts/make-artwork.swift`) | The app had macOS's placeholder icon. Original artwork: SF Symbols can't be used in app icons |
| **DMG window:** background with an arrow, 128 pt icons, no toolbar or sidebar, the app named "Voice to Text.app", the icon on the disk and the `.dmg` file (`dmgbuild`) | A plain `hdiutil` DMG opened as a bare Finder window. dmgbuild writes the layout without Finder scripting, so it works in CI |
| **Menu-bar icon:** still waveform bars (template, white on a dark menu bar) with a red dot while working | The first version animated the bars with your voice and rippled while processing; it duplicated the overlay and felt busy, so it was simplified |
| **One install location:** `build-app.sh --install` writes `/Applications/Voice to Text.app`, like the DMG, and removes older copies | Dev builds went to `~/Applications/VoiceToText.app`, so a DMG install left two copies in Launchpad |
| **Servers restart when their binary changes**, not only their model | After an update or a move, servers from the old copy kept running (even from a deleted copy) and were reused |
| **Shader warm-up also after a move** (the check includes the app's path) | The Metal shader cache is per location, so moving the app from the DMG to Applications meant a slow first dictation |
| Downloaded models get normal permissions (0644) | `URLSession` leaves them owner-only (0600), unlike the other model files |

## 5d. Release-readiness review (2026-09-24)

Before tagging v0.1.0: a dry run of `release.sh` with the real release certificate (passed), a simulated new user (an empty
home folder via `CFFIXED_USER_HOME`), a scan of the repository for personal data (clean), a simulated download, and an
independent code review of the app. Fixed:

| Severity | Problem | Fix |
|---|---|---|
| **Blocker** | In a downloaded DMG every helper is quarantined. Open Anyway approves the app only, so running `whisper-server` hung in Gatekeeper's check (confirmed: a quarantined copy hung, an unquarantined one ran in 0.03 s). The app can't clear the flag in its own bundle (App Management) | `BundledHelpers` copies the helpers to Application Support at launch and runs them from there |
| **Blocker** | A resumed download finishes with HTTP 206, which was treated as a failure, so resuming never worked | Any 2xx is success; retries reset per download; the progress bar stays up while it waits to resume |
| Should-fix | Closing a window while the hotkey recorder waited left the hotkey unregistered and swallowed keys | One shared recorder that stops on window close or resign-key |
| Should-fix | Move and Reopen quit even when the copy didn't open | It quits only once the copy is running; otherwise it shows the error |
| Should-fix | Running setup again could switch an existing user to S1-mini | Setup only chooses the engine before setup is completed |
| Should-fix | S1-mini could be deleted while it was the cleanup engine | No delete button while it's in use |
| Minor | The Claude check could hang on a shell profile's background program; nvm, Volta, npm-global, Bun installs and non-zsh login shells weren't found; an offline install looked like success | Output to a file with a hard timeout; the user's own shell and those folders; the installer is downloaded first |
| Minor | A hotkey like ⌘V or ⌘Q could be recorded | Hotkeys need ⌃ or ⌥, or a function key |
| Minor | A model downloaded from its own row wasn't selected; the mic list went stale; the setup window could be taller than a small screen; cancelling during the checksum check still installed the file | Each fixed |

## 6. Risks

| Risk | Mitigation |
|---|---|
| A static build is slower, or the Metal backend fails at runtime without the Metal compiler | Spike S1 compares speed, and `GGML_METAL_EMBED_LIBRARY` compiles the shaders from source at load time. If it fails, fall back to shipping the dylibs in `Contents/Frameworks` with `@rpath`. |
| Users are put off by Open Anyway | Clear screenshots in the README and release notes. The Apple Developer account removes it entirely (decision 3). |
| Permissions lost after an update | Release-certificate signing (level 1) keeps the identity; spike S2 confirms it. The setup helper covers the stale Accessibility entry. |
| The official Claude installer or `auth` commands change | The app only opens the installer and reads `auth status`; the Check Again button and a link to Anthropic's install page cover changes. Setup never blocks on Claude, because S1-mini works. |
| A 574 MB–1.6 GB download on first run | Background download with resume; the compressed model by default; the rest of setup continues meanwhile. |
| The settings refactor breaks existing behaviour (hotkey, modes, pre-start) | The store keeps the same UserDefaults keys; overlay snapshots, the self-test and a round of real dictations after M4.4. |
| Bundled binaries fall behind Homebrew's (fixes, speed) | Pinned tags bumped deliberately, with the e2e eval run each time. |

## 7. After M4

- **M5 (v0.1 release):** name and trademark check, provider terms check, demo GIF, the first tagged DMG from `release.yml`.
- **M6 (more providers):** a `Refiner` protocol with Codex CLI, Gemini CLI, Ollama and an OpenAI-compatible API (key in Keychain),
  shown in the Cleanup pane.
- Later: Sparkle updates, a Homebrew cask (check Homebrew's current rules first; its official repository has been moving away from
  apps that fail Gatekeeper, so it will probably need notarization), editing mode prompts, and dictation history.
