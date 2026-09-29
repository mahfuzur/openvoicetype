# Diagrams of how OpenVoiceType works, Detailed Plan

**Goal:** a new contributor (or a curious user) can see how a dictation travels through the app in a few minutes, without
reading 7,500 lines of Swift and Bash. Every box and arrow is backed by a line of code.

**Status legend:** ☐ to do · ◐ in progress · ☑ done

**Status: ◐ in progress on `docs/archify-diagrams` (2026-09-29).**

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
| Install | None in the repo: no `package.json`, no `node_modules`. The script clones Archify into a cache folder outside the repo. It has no runtime dependencies; Node ≥ 18 and a Chromium-based browser are enough |
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
| T3 | `scripts/render-diagrams.sh`: pinned clone, `finalize --quality showcase --repo-root .`, `visual-check`, copy page and PNGs; `--check` to only validate; shellcheck clean | ☐ |
| T4 | D1–D6 JSON sources, each passing `finalize` at showcase quality | ☐ |
| T5 | Look at every screenshot (light and dark) and fix what reads badly | ☐ |
| T6 | `docs/diagrams/README.md`, and links from README (Contributing), GUIDE (How it works), CONTRIBUTING and CLAUDE.md (Commands) | ☐ |
| T7 | Checks: shellcheck, every receipt 9/9 with 0 warnings and browser evidence passed, the HTML makes no network requests, a second render gives the same page | ☐ |

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
| Repository size | The pages and PNGs are checked for size before committing; PNGs are the 1440-wide captures only |

## 7. Out of scope (later)

- Publishing the pages with GitHub Pages.
- A CI job that re-renders the diagrams (it would need Node, a browser and the Archify clone on every run).
- Diagrams for the build and release pipeline (`build-deps.sh`, `build-app.sh`, `release.sh`) and first-run setup.
