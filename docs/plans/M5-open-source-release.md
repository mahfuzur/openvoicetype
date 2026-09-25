# M5: Open-Source Release, Detailed Plan

**Goal:** the project is ready to be found, trusted and contributed to. It has a unique name (**OpenVoiceType**), says
honestly how it uses Claude Code and what Anthropic's terms allow, has a demo, and has the usual community files. The first
release under the new name (v0.2.0) moves existing users over without losing their settings.

**Status legend:** ☐ to do · ◐ in progress · ☑ done

**Status: ☑ Done. v0.2.0 was released on 2026-09-25** ([release](https://github.com/mahfuzur/openvoicetype/releases/tag/v0.2.0)).
Deferred: the demo GIF. Still to do: the second-Mac install test (M4.11) and, optionally, asking Anthropic about the terms.
These are tracked in the roadmap's "Open items".

## 1. Scope decisions (2026-09-24)

| Question | Decision |
|---|---|
| Name | **OpenVoiceType.** "Voice to Text" is too generic to search for. Checked on 2026-09-24: no GitHub repo, app or website uses the name (a clash check, not a trademark search). Names built on "whisper" were rejected: FreeWhisper and OpenWhisper exist, and the space is crowded (Superwhisper, MacWhisper, TypeWhisper). |
| What the rename covers | **Visible names only:** the app name, bundle ID `io.github.mahfuzur.openvoicetype`, the installed `OpenVoiceType.app`, the DMG, every UI string, the docs and the repo links. **Unchanged:** `~/.config/voice-to-text`, `~/Library/Logs/voice-to-text`, `$TMPDIR/voice-to-text`, the `dictate` CLI command, the SwiftPM target and executable (`VoiceToText`), and the signing certificate names (so the CI secrets stay valid). |
| Claude's terms | **Disclose and soften.** A `docs/TERMS.md` page explains what the app does, using the terms' own wording. The tagline changes from "uses the AI subscription you already have" to "works with your own Claude Code". S1-mini is pointed out as the fully local option. |
| Demo GIF | **A script; the maintainer records.** `scripts/make-demo-gif.sh` records a screen region and converts it to an optimized GIF. The README gets a slot for it. |
| Publishing | **Prepare only.** Everything lands on the `m5-release` branch for review. No tag, release, repo rename or settings change is made here; §7 lists the exact steps and commands. |
| Apple Developer account | Still open (roadmap decision 3). The release stays signed with the self-signed release certificate. |

## 2. Already done before M5

| Roadmap item | Status |
|---|---|
| License | ☑ MIT (`LICENSE`), no GPL code or prompt text |
| Public repo, CI, release workflow | ☑ `ci.yml` (lint, build, helper check, snapshots) and `release.yml` (DMG from a `v*` tag); v0.1.0 and v0.1.1 published |
| Distribution | ☑ DMG from M4, self-signed release certificate, update check |
| Toolchain | ☑ Settled in M4: Command Line Tools only, locally and in CI (the `macos-14` runner). Installing Xcode isn't needed |
| Docs | ◐ README (privacy section, troubleshooting), CONTRIBUTING, CHANGELOG, a bug-report template. Missing: the demo GIF, a feature-request template, a PR template and SECURITY.md (all but the GIF added in R6) |
| Personal paths | ◐ The repo is clean apart from `install.sh`, which links Superwhisper's model folder (against the project rule "never hardcode another app's model folder"; fixed in R7) |
| Second-Mac install test (M4.11) | ☐ Needs a real second Mac; stays with the maintainer (§7) |

## 3. The terms check (2026-09-24)

Sources: Anthropic's [Claude Code legal and compliance page](https://code.claude.com/docs/en/legal-and-compliance)
and the [Consumer Terms](https://www.anthropic.com/legal/consumer-terms). This is a summary for the project, not legal advice.

| The terms say | This app |
|---|---|
| OAuth (subscription) sign-in is "designed to support ordinary use of Claude Code and other native Anthropic applications" | Runs the user's **own, unmodified** `claude` binary, installed with Anthropic's installer |
| "Nor does it prevent an end user from signing in to the unmodified Claude Code binary with their own Claude subscription" | Sign-in happens only through `claude auth login`, in Terminal, through Anthropic's flow |
| Developers may not "collect, store, or intermediate Claude.ai credentials or session tokens" | The app never reads, stores or sends a token; it only reads `claude auth status --json` (signed in: yes or no) |
| Third-party developers may not "route requests through Free, Pro, or Max plan credentials on behalf of their users" | There's no server: every request comes from the user's own Mac, for the user's own dictation. **This is the grey area:** a distributed app that starts the user's CLI for them |
| Usage limits "assume ordinary, individual usage" | One short request per dictation, by one person |
| Don't use Anthropic's names in a way "that suggests Anthropic built, endorses, or is partnered with your product" | The name has no "Claude" in it; the README says the project isn't affiliated with Anthropic |
| Early 2026: subscriptions stopped covering third-party tools such as OpenClaw | Those tools used subscription OAuth tokens themselves; this app doesn't |

**What changes:** a `docs/TERMS.md` page with the table above in plain words, linked from the README, About and the Cleanup
pane. The marketing gets softer: "works with your own Claude Code" instead of "uses the subscription you already have",
and "no word limits" goes, because the plan's own limits apply. The README says that users are responsible for following
Anthropic's terms, and that S1-mini keeps everything on the Mac. The maintainer may still ask Anthropic for written
confirmation (§7).

## 4. Design

### A. Rename to OpenVoiceType

| Where | Change |
|---|---|
| `Info.plist` | `CFBundleIdentifier` `io.github.mahfuzur.openvoicetype`, `CFBundleName` and `CFBundleDisplayName` "OpenVoiceType", the microphone usage text |
| `build-app.sh` | `BUNDLE_ID`; `--install` writes `/Applications/OpenVoiceType.app` |
| `release.sh`, `dmg-settings.py`, `release.yml` | `dist/OpenVoiceType-<version>.dmg`, volume name "OpenVoiceType", "OpenVoiceType.app" in the DMG window, the release title and notes |
| `make-artwork.swift` | The DMG caption "Drag OpenVoiceType to Applications"; regenerate `dmg-background.tiff` |
| Swift UI strings | Menu ("Quit OpenVoiceType"), setup ("Welcome to OpenVoiceType"), Settings, Accessibility hints, notifications, the log and error texts |
| `Updater.repository` | `mahfuzur/openvoicetype` (GitHub redirects the old name, so v0.1.x still finds v0.2.0) |
| `BundledHelpers` | Copies go to `~/Library/Application Support/OpenVoiceType/Helpers`; the old `Voice to Text` folder is deleted |
| Docs | README, CONTRIBUTING, CLAUDE.md, ARTWORK.md, ROADMAP, CHANGELOG, bug template; the README screenshots that show the name are regenerated |

**Moving existing users over** (new file `Migration.swift`, once, on the first launch):
1. **Settings:** the old app's preferences (`io.github.mahfuzur.voicetotext`) are copied into the new domain if it has none
   of its own: hotkey, modes, dictionary choices, models, and a finished setup.
2. **The old app:** if `Voice to Text.app` is in `/Applications` or `~/Applications`, an alert explains the rename and
   offers **Move to Trash**. That quits the old app if it's running (two apps would fight over the hotkey), moves it to the
   Trash, and removes its stale Accessibility entry (`tccutil reset Accessibility io.github.mahfuzur.voicetotext`).
   **Keep** leaves it alone and doesn't ask again.
3. **Permissions:** a new bundle ID is a new app to macOS, so Microphone and Accessibility have to be granted once more.
   This can't be avoided, and it's what the release notes lead with. The existing launch flow already asks for both.
4. **Open at login** belongs to the old app and can't be carried over. The alert says to turn it on again in Settings.

### B. Terms and wording

- `docs/TERMS.md` (§3), linked from the README (the Privacy section and a new "Claude Code and Anthropic's terms"
  paragraph), the About pane and the Cleanup pane.
- The tagline, the intro, the GitHub description (§7) and `NSHumanReadableCopyright` stay factual: on-device Whisper,
  polished by your own Claude Code CLI, no API keys, no servers.

### C. Demo GIF

- `scripts/make-demo-gif.sh [seconds] [x,y,w,h]` records a screen region with `screencapture -v -V <s> -R <region>`,
  converts it with `ffmpeg` (palette, 12 fps, 800 px wide) to `docs/images/demo.gif`, and prints the size (the aim is under 5 MB).
  It needs Screen Recording permission for the terminal, and `brew install ffmpeg`.
- The README keeps an HTML comment where the GIF goes, so no broken image shows until the file exists. ARTWORK.md
  describes what to record: a Slack or Notes window, the hotkey, a 10 s dictation with a self-correction, then the pasted text.

### D. Community files

- `.github/ISSUE_TEMPLATE/feature_request.md` and `config.yml` (links to Troubleshooting and the security policy).
- `.github/pull_request_template.md`: the CONTRIBUTING checklist (shellcheck, build, self-test, eval numbers, snapshots).
- `SECURITY.md`: report privately through GitHub's security advisories. It says what counts: anything that could leak audio
  or text, run a transcript as an instruction, or tamper with the helpers or downloads.

### E. Cleanups

- `install.sh`: drop the Superwhisper model link and always download. Existing links keep working.
- Personal-info audit: the repo contains only the maintainer's public name and GitHub handle (bundle ID, copyright, repo
  links), which is intended. It has no e-mail addresses (other than examples), no home paths and no tokens.

### F. Release prep (v0.2.0, not published here)

- `CHANGELOG.md`: a 0.2.0 section (the rename, the permission note, terms wording, community files).
- Release notes for `release.yml`, written for users of v0.1.x.

## 5. Tasks

| # | Task | Status |
|---|---|---|
| R1 | Rename: `Info.plist`, `build-app.sh`, `release.sh`, `dmg-settings.py`, `release.yml`, `ci.yml` paths; UI strings; `Updater.repository`; `BundledHelpers` folder | ☑ The local v0.2.0 DMG holds `OpenVoiceType.app` with the new ID, and its signature verifies |
| R2 | `Migration.swift`: copy settings, offer to move the old app to the Trash (quit it, clean its Accessibility entry), open-at-login note | ☑ The settings copy was tested on throwaway preference domains (6 checks). Old-app detection was run read-only on this Mac: it finds only `/Applications/Voice to Text.app`, not the build folder or mounted DMGs. **The Trash flow wasn't run**: the only old app here is the maintainer's real one. Developer modes (`--settings-snapshots` …) skip the migration |
| R3 | Artwork: the DMG caption, regenerated background; README screenshots that show the name | ☑ The background is regenerated, and the maintainer retook `docs/images/dmg-window.png` from the local v0.2.0 DMG. The other screenshots don't show the name |
| R4 | `docs/TERMS.md`; README tagline, intro, Privacy and License wording; About and Cleanup links | ☑ The app's "your subscription" labels are softened too (the Cleanup picker, setup, the Claude status line) |
| R5 | `scripts/make-demo-gif.sh`; README slot; ARTWORK.md recording notes | ☑ `--from` conversion tested (10 s → 2.2 MB); recording itself needs Screen Recording, so not run here |
| R6 | Community files: feature request, `config.yml`, PR template, `SECURITY.md` | ☑ Also a version line in the bug template. Private vulnerability reporting must be switched on (§7 step 3) |
| R7 | `install.sh` without the Superwhisper link | ☑ |
| R8 | Docs: README, CONTRIBUTING, CLAUDE.md, ARTWORK.md, ROADMAP, CHANGELOG 0.2.0 | ☑ Plus `scripts/release-notes.md` for the GitHub release body |
| R9 | Checks: shellcheck, app build, `--settings-snapshots`, `--overlay-snapshots`, self-test, migration test | ☑ All pass; the self-test ran with its own state directory and ports |
| R10 | Maintainer: record the GIF, rename the repo, update its description and topics, tag v0.2.0, test on a second Mac (§7) | ◐ Repo renamed, description, topics and private vulnerability reporting set, v0.2.0 released (2026-09-25). The GIF is deferred; the second-Mac test is still to do |

## 6. Acceptance criteria

- A build shows "OpenVoiceType" everywhere a user can see a name: menu, setup, Settings, alerts, the DMG, Finder,
  Login Items and the permission prompts. `git grep "Voice to Text"` only finds the migration code, the signing certificate
  names, history notes ("was called Voice to Text") and the old plans.
- On a Mac with v0.1.x settings, the first launch keeps the hotkey, models and modes, offers to trash the old app, and
  after the two permission grants, dictation works.
- `docs/TERMS.md` exists and is linked from the README, About and Cleanup; no text promises "no limits".
- `scripts/make-demo-gif.sh` produces a GIF under 5 MB from a 15 s recording. (Conversion tested; the GIF itself is deferred.)
- shellcheck, the build and the self-test pass, with no new warnings.

## 7. Publishing steps (for the maintainer, after review)

1. ☑ (2026-09-24) **Try the migration yourself** on this Mac: `./scripts/build-app.sh --install` installs `/Applications/OpenVoiceType.app`
   next to the old app. On its first launch it should keep your settings, offer to trash Voice to Text, then ask for
   Microphone and Accessibility.
2. ☑ (2026-09-24) Merge the PR, then rename the repo (GitHub keeps redirecting the old URL, so v0.1.x update checks still work):
   `gh repo rename openvoicetype`
3. ☑ (2026-09-25) Description, topics and private security reports:
   `gh repo edit --description "Free, open-source macOS dictation: on-device Whisper, polished by your own Claude Code CLI. No API keys." --add-topic macos --add-topic menu-bar-app --add-topic claude-code --add-topic whisper-cpp`
   and `gh api -X PUT repos/mahfuzur/openvoicetype/private-vulnerability-reporting` (the SECURITY.md link needs it).
4. ☑ (2026-09-24) Screenshots: open `dist/OpenVoiceType-0.2.0.dmg` (`./scripts/release.sh v0.2.0`), press ⌘⇧4, then Space, and click the
   window. Save it as `docs/images/dmg-window.png`.
5. Deferred (2026-09-25): record the GIF (`scripts/make-demo-gif.sh`, see docs/ARTWORK.md), uncomment its line in the README,
   and commit. The first try failed with `screencapture: capture error`: the terminal's app had no Screen Recording
   permission. Grant it and relaunch that app, or record with ⌘⇧5 and convert with `--from`.
6. ☑ (2026-09-25) Change the CHANGELOG heading to the release date, then tag and publish: `git tag v0.2.0 && git push origin v0.2.0`
   (`release.yml` builds the DMG and uses `scripts/release-notes.md` as the release text).
7. ☐ Install the published DMG on a second Mac without Homebrew or Claude (M4.11).
8. ☐ Optional: ask Anthropic ([contact](https://www.anthropic.com/contact-sales)) to confirm that a free, local app starting the
   user's own Claude Code CLI is fine with a subscription.

## 8. Risks

| Risk | Mitigation |
|---|---|
| Every existing user has to grant both permissions again | Unavoidable with a new bundle ID. The release notes and the migration alert say so first. This is the only time: later updates keep the release certificate and the ID |
| The old and new app run at the same time and fight over the hotkey | The migration alert quits the old app and offers to trash it; the hotkey error message names the other app |
| A build from this branch checks for updates at a repo name that doesn't exist yet | It gets a 404 and shows nothing; the rename (§7 step 2) happens before the release |
| Anthropic changes or tightens the terms | TERMS.md is dated, the S1-mini fallback works with no Claude at all, and M6 adds other providers |
| The name turns out to be taken in some register | This was a clash check, not a trademark search; renaming again later costs the same one-time permission reset |
