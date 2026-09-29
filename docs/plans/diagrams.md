# Diagrams of how OpenVoiceType works, Detailed Plan

**Goal:** a new contributor (or a curious user) can see how a dictation travels through the app in a few minutes, without
reading 7,500 lines of Swift and Bash. Every box and arrow is backed by a line of code.

**Status legend:** ☐ to do · ◐ in progress · ☑ done

**Status: ☑ Done on `docs/archify-diagrams` (2026-09-29).** Notes from building it are in §8.

**Why now.** The pipeline has grown well past the ASCII sketch in [GUIDE.md](../GUIDE.md#how-it-works): a pre-started
Claude, a warm whisper-server, three cleanup engines with fallbacks, a meaning guard, a paste-target check, and Command
Mode's target planner. The facts are all in CLAUDE.md and the code, but spread over many sections. A few diagrams answer
the questions people actually ask ("what leaves my Mac?", "what happens when Claude is down?") at a glance.

## 1. The tool

[Archify](https://github.com/tt-a1i/archify) (MIT) is an agent skill with a Node CLI. You write a typed JSON spec; its
`finalize` command validates it, renders a self-contained interactive HTML page (inline SVG, light and dark themes,
search, focus, route tracing, PNG/SVG export), and checks the page in a real browser. `visual-check` also captures PNG
screenshots.

| Decision | Choice |
|---|---|
| Version | Pinned to v3.0.1, commit `0e4949f` (2026-09-28). The render script checks that commit out, so a re-render is reproducible |
| Install | None in the repo: no `package.json`, no `node_modules`. The script fetches the pinned commit only (about 80 MB, 16 s; the full history is 500 MB) into a cache folder outside the repo. It has no runtime dependencies; Node ≥ 18 and a Chromium-based browser are enough |
| Network | Only the one-time clone. `ARCHIFY_UPDATE_CHECK_DISABLED=1` turns off its release check |
| Evidence | Each diagram pins `meta.repository` to the commit it was traced from (`1fe87ef`, master after v0.5.0). The architecture diagram cites file and line ranges per component, and `finalize --repo-root .` checks them against that commit |
| Quality | `--quality showcase`: all nine artifact checks, zero composition errors, zero warnings, and the browser gate |

## 2. The diagrams

| # | File | Type | Question it answers | Main sources |
|---|---|---|---|---|
| D1 | `system-overview` | architecture | What are the parts, and which run on the Mac vs online? | `AppDelegate`, `Dictation`, `Recorder`, `PasteTarget`, `Paster`, `dictate.sh` (servers, engines), `BundledHelpers`, `APIKeychain` |
| D2 | `dictation-sequence` | sequence | What happens between pressing the hotkey and the text appearing, and why is it fast? | `Dictation.start/stop/prestart`, `cmd_refine`, `claude_prestart`, `claude_send`, `transcribe_server`, `deliver` |
| D3 | `cleanup-fallback` | workflow | Which engine cleans the text up, and what happens when it fails? | `refine_text`, `try_online`, `is_offline`, `try_s1`, `meaning_guard`, `cmd_refine` exit codes |
| D4 | `dictation-states` | lifecycle | What states does a dictation go through, and how does each end? | `Dictation.State`, `Outcome`, `toggle/start/stop/cancel`, `AppDelegate.finished/deliver` |
| D5 | `privacy-dataflow` | dataflow | What data goes where: audio, transcript, keys, logs? | `Recorder`, `transcribe`, `user_message`, `refine_openai`, `openai_headers`, `APIKeychain`, `log_maintain`, `Updater` |
| D6 | `command-mode-planner` | workflow | What does a Command Mode press act on (selection, follow-up, last dictation, write, copy)? | `SelectionReader.read`, `CommandPlanner.decide`, `deliverCommand` |

Rules for every diagram:
- **Only what the code does.** Each claim is traced to its call site. An optional path (whisper-cli, the one-shot Claude
  call) is drawn as a fallback, not as the main path.
- **Names match the code** (`dictate.sh refine`, `whisper-server`, `claude -p`), so a reader can search for them.
- **English**, the default Archify theme, no motion unless it helps (D2 and D3 get the `trace` animation to show the path).

## 3. Files

```
docs/diagrams/
  README.md                    the index: what each diagram shows, a screenshot of each, how to open and re-render
  <name>.json                  the source (edit this)
  <name>.html                  the rendered page, self-contained (open it in a browser)
  images/<name>-light.png      screenshots for GitHub, which doesn't render HTML files
  images/<name>-dark.png
scripts/render-diagrams.sh     re-renders one or all diagrams: finalize + visual-check, then copies the page and images
```

Archify's receipts and sidecars (`*.finalize-summary.json`, browser receipts, contact sheets) stay in a build folder
(`app/build/diagrams/`, already ignored), not in `docs/`.

## 4. Tasks

| # | Task | Status |
|---|---|---|
| T1 | Read the codebase and record the facts for each diagram with file and line references | ☑ |
| T2 | This plan | ☑ |
| T3 | `scripts/render-diagrams.sh`: pinned clone, `finalize --quality showcase --repo-root .`, `visual-check`, copy page and PNGs; `--check` to only validate; shellcheck clean | ☑ |
| T4 | D1–D6 JSON sources, each passing `finalize` at showcase quality | ☑ |
| T5 | Look at every screenshot (light and dark) and fix what reads badly | ☑ |
| T6 | `docs/diagrams/README.md`, and links from README (Contributing), GUIDE (How it works), CONTRIBUTING and CLAUDE.md (Commands) | ☑ |
| T7 | Checks: shellcheck, every receipt 9/9 with 0 warnings and browser evidence passed, the HTML makes no network requests, a second render gives the same page | ☑ |

## 5. Acceptance criteria

- All six diagrams pass `finalize --quality showcase` (9/9 checks, 0 errors, 0 warnings, browser check passed).
- The architecture diagram's source references pass `--repo-root` against the pinned commit.
- Every screenshot has been looked at in both themes; no clipped labels, no crossing lines that hide meaning.
- The pages are self-contained: they open from disk with no network.
- `scripts/render-diagrams.sh` re-renders everything from a clean checkout, and `shellcheck scripts/*.sh` passes.
- Nothing in the diagrams contradicts CLAUDE.md or the code (checked claim by claim against the sources in §2).

## 6. Risks

| Risk | Mitigation |
|---|---|
| The diagrams go stale as the code changes | Each one names the commit it was traced from. CONTRIBUTING asks to update the JSON when a pipeline change affects it, and the script makes that a one-command re-render |
| Archify changes its schema or output | The version is pinned; upgrading is a deliberate change of `ARCHIFY_REF` in the script |
| GitHub shows `.html` files as source, not as a page | The index embeds PNG screenshots (with a dark-mode variant) and says how to open the page. GitHub Pages could serve them later (out of scope) |
| Repository size | Each page is about 770 KB (Archify's viewer and its embedded font), 4.6 MB for the six, plus 1.5 MB of PNGs. A re-render leaves unchanged pages and their screenshots alone, so only a real change adds to the history |

## 7. Out of scope (later)

- Publishing the pages with GitHub Pages.
- A CI job that re-renders the diagrams (it would need Node, a browser and the Archify clone on every run).
- Diagrams for the build and release pipeline (`build-deps.sh`, `build-app.sh`, `release.sh`) and first-run setup.

## 8. Notes from building it (2026-09-29)

- **Fitting the desktop viewport.** Showcase quality wants the whole diagram on a 1440×900 screen with text of at least
  about 7px. That set the limits: at most about 1,085 px wide, and 696 px tall at that width. The dictation sequence went
  from 18 messages to 15 (the two script launches are one message; pinning the target is part of the press) and three
  bands instead of five, because a band's label sits 22 px above it.
- **Routing.** Archify's workflow router can't untangle every graph. For the cleanup and Command Mode diagrams, a small
  script tried the combinations of lane order, column and endpoint sides, kept the ones that validate, and ranked them
  by bends and route length. The chosen layouts use only two pinned sides.
- **Simplified, not changed:** the online check is part of the "Claude or API" node (one "offline, limit, sign-in,
  timeout" edge instead of two); "Whisper's text" is an end state (it's post-processed and pasted like the rest);
  "cleanup skipped" and the 6,000-character refusal are in cards; Command Mode's "couldn't read" leaves the reader node.
- **Legends.** Workflow and data-flow legends default to Archify's own words ("Agent logic", "policy / PII"); every
  diagram that needed it renames them (`meta.legend.entries`).
- **Two wordings fixed against the code:** the API edge says "text + your key", not "HTTPS" (a local Ollama is plain
  HTTP), and "nothing is sent at all" became "none of your words are sent" (the update check still runs).
- **Screenshots aren't byte-stable:** the pages render identically, but 5 of the 12 PNGs differed on a second capture.
  The script keeps the committed screenshots when the page didn't change.
