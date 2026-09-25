# Research: Command Mode and the "use your own CLI" selling point (2026-09-25)

What this covers: how other dictation apps use the user's own AI CLI (our main selling point), what the vendors' terms say,
how to make our Claude call faster and sturdier, and how the leading apps do voice editing ("Command Mode"). The findings
feed [plans/M5.5-command-mode-context-snippets.md](../plans/M5.5-command-mode-context-snippets.md).

**Method.** Vendor docs, changelogs and help centers; the source code of the open-source apps (VoiceInk, OpenWhispr,
TypeWhisper, VoiceTypr, talon-ai-tools, SelectedTextKit, Easydict); GitHub issues; Reddit (through an archive), Hacker News and
review sites. We only read GPL and AGPL projects to learn how they work, and copied none of their code or prompts. Local
checks: `claude --help` (2.1.280) and `gemini --help` (0.46.0). No prompts were sent. "Claimed" marks a vendor's marketing or a
single user's report.

## 1. Summary

- **Using the user's own CLI is no longer unique to us.** At least 10 dictation apps or projects do it:
  - VoiceInk (since v1.73, 2026-04-10);
  - TypeWhisper, VoiceTypr, WhisperBar, dikte, SimpleWhisper, mutterbox and others;
  - Raycast (v2.3, 2026-09-11, "Bring Your Own Subscription").
- **What's still ours is speed and isolation.** Every competitor we found starts a *cold* CLI process for each dictation.
  VoiceTypr designed a warm session and dropped it. We're the only one that pre-starts Claude while you speak: about 1 s
  after the transcript, against about 2.7–16 s cold. Most also run Claude without `--tools ""`, a strict MCP config or a
  neutral working directory. That makes them slower, and Claude can act on the transcript.
- **We're behind on breadth:** every rival also offers Codex, and most offer Gemini/Antigravity, Pi or Copilot. The app is
  also macOS only.
- **Terms.** Anthropic still allows a user to sign in to the real Claude Code with their own plan, and forbids apps from
  handling its credentials. Spawning the user's unmodified CLI has drawn no objection, but it's a grey area. OpenAI openly
  welcomes third-party use of Codex with ChatGPT plans. **Google ended Gemini CLI for personal accounts (2026-06-18), and
  Antigravity's terms forbid third-party tools.**
- **Command Mode is a proven feature, but no one does it reliably.**
  - Wispr stopped developing its Command Mode ("shipped as a prototype") after months of "servers are busy" complaints.
    It replaced it with Transforms: preset hotkeys, not voice.
  - Aqua (July 2026) and Typeless have the best-reviewed designs.
  - Nobody publishes latency.

## 2. Apps that use the user's CLI

| App | CLIs | How it's called | Latency | License | Since |
|---|---|---|---|---|---|
| VoiceInk | claude, codex, pi, copilot | cold `zsh -lc "<template>"` per dictation; prompt in env vars and stdin; 45 s timeout | not published | GPL-3 | v1.73, 2026-04-10 |
| TypeWhisper | claude, codex, opencode, antigravity | argument arrays, stdin, JSON schema, temporary working directory; `--safe-mode` | n/a | GPL-3 | plugin 2026-09-03 |
| VoiceTypr | claude, codex, pi, omp, opencode | cold, stdin, temporary working directory, 20 s | about 2.7 s cold with Haiku (their spec; 16 s without isolation flags) | AGPL-3 | 2.0.6-beta, 2026-08-27 |
| WhisperBar | claude, codex, gemini | templates | "much faster than on-device" | closed | v1.14, May 2026 |
| dikte | claude, codex, agy | cold `-p`, flags almost like ours | "a few extra seconds" | GPL-3 | 2026-09-24 |
| SimpleWhisper | claude, codex, gemini, agy | `claude -p --setting-sources ""` | about 4 s with Haiku, about 10 s with defaults | MIT | 2026-09 |
| mutterbox, claude-dictation, friendly | claude (+ gemini, codex) | `-p` / `exec` | n/a | MIT / none | 2026-06 to 07 |
| OpenWhispr | claude, codex (agent only, PR not merged) | stream-json | n/a | MIT | not merged |
| Raycast (adjacent) | claude, codex | the CLIs' own login | n/a | closed | 2026-09-11 |
| **OpenVoiceType** | claude (+ S1-mini offline) | **pre-started stream-json during recording** | **about 1 s after the transcript; 2.7–5.7 s stop to paste** | MIT | 2026 |

**VoiceInk details.**
- Armin Ronacher asked for it in [#591](https://github.com/Beingpax/VoiceInk/issues/591) (2026-03-16); it was built in #630.
- Its default Claude template is `claude -p "$VOICEINK_FULL_PROMPT"`, with no tool or MCP isolation and no neutral working directory.
- An unreleased change (2026-09-22) moves it to `--model claude-sonnet-5 --effort low`. As our own testing showed, `--effort low`
  doesn't turn off thinking.
- User reports:
  - failures were silently discarded ([#830](https://github.com/Beingpax/VoiceInk/issues/830));
  - chat-like answers instead of the cleaned text ([#838](https://github.com/Beingpax/VoiceInk/issues/838)).

**Demand** is real but small in votes: VoiceInk #591, FluidVoice #199 and #311 ("people pay for subscriptions already"),
TypeWhisper #961, OpenWhispr #1319, #1467 and #1468, and Handy discussion #168.

**Wispr Flow, Superwhisper, Spokenly, Typeless, MacWhisper and Aqua don't do this.** They use their own plans or API keys.
Claude Code's own `/voice` (since 2026-03) dictates into Claude Code only.

## 3. Terms (checked 2026-09-25)

**Anthropic.** No change since `docs/TERMS.md` (2026-09-24).
- **Allowed.** The [Legal and compliance](https://code.claude.com/docs/en/legal-and-compliance) page (updated 2026-08-21) says:
  - it doesn't "prevent an end user from signing in to the unmodified Claude Code binary with their own Claude subscription";
  - "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and the Agent SDK."
- **Not allowed:**
  - offering Claude.ai login in your own app, or routing requests through users' plan credentials;
  - "collect, store, or intermediate Claude.ai credentials or session tokens";
  - Claude Code or Anthropic names or logos in a product name.
  - Enforcement "may [happen] without prior notice".
- **Grey:**
  - "Developers building products or services that interact with Claude's capabilities, including those using the Agent SDK,
    should use API key authentication."
  - The [headless docs](https://code.claude.com/docs/en/headless) call `claude -p` "the Agent SDK via the CLI".
  - The [Agent SDK overview](https://code.claude.com/docs/en/agent-sdk/overview) says third parties may not "offer claude.ai login
    or rate limits" without approval.
  - [Consumer Terms](https://www.anthropic.com/legal/consumer-terms) forbid "automated or non-human means… except… where we
    otherwise explicitly permit it". Anthropic's own docs describe scripted `-p` use on a subscription.
- **Closest to approval:**
  - [@ClaudeDevs, 2026-05-13](https://x.com/ClaudeDevs/status/2054610157364289906): "third-party tools built on the Agent SDK
    like Conductor and OpenClaw work with your Claude plan".
  - The [Help Center](https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan)
    (2026-06-16): moving `-p` to a separate credit is paused, and "`claude -p`, and third-party app usage still draw from your
    subscription's usage limits… we'll share it before anything takes effect".
- **2026 timeline:**
  - January: blocks on spoofed OAuth clients.
  - March: OpenCode removed Claude OAuth after legal requests.
  - April 4: OpenClaw-style harnesses cut off.
  - May: a separate Agent SDK credit announced.
  - June 15: that change paused.
  - Throughout, enforcement targeted **token reuse and client spoofing, never spawning the real CLI**.
- **Technical risk:** the headless docs say "`--bare`… will become the default for `-p` in a future release". Bare mode never
  reads OAuth, so our call would stop using the subscription.
- **Our own gap:** if `ANTHROPIC_API_KEY` is set, `-p` always uses it, so a user who exports one pays API charges without knowing.

**OpenAI Codex (ChatGPT plan): allowed and encouraged.**
- The [Codex SDK](https://learn.chatgpt.com/docs/codex-sdk) and [app-server](https://learn.chatgpt.com/docs/app-server) are
  documented for integrating Codex into your own product, with ChatGPT login.
- "`codex exec` reuses saved CLI authentication by default."
- Codex for Open Source says developers should use "Codex, OpenCode, Cline, pi, OpenClaw, or something else".
- Caveats: API keys are still "the recommended default for automation", and the Terms of Use forbid circumventing rate limits.

**Google.**
- Since 2026-06-18, Gemini CLI no longer accepts "Login with Google"
  ([notice](https://developers.google.com/gemini-code-assist/resources/privacy-notice-gemini-code-assist-individuals)).
  It works only with paid API keys, Vertex or Code Assist Standard/Enterprise.
- The successor's [Antigravity terms](https://antigravity.google/terms) §6: "Using third party software, tools, or services to
  access the Service… is a breach."
- **Treat Antigravity and Gemini as off-limits except with an API key.**

**GitHub Copilot CLI: allowed.** `copilot -p` and ACP are documented "in any third-party tools… or automation systems".
Programmatic use consumes AI credits, and inputs are used for training unless the user opts out.

## 4. Making the Claude call faster and sturdier

Sources: local `claude --help` (2.1.280) and the [CLI reference](https://code.claude.com/docs/en/cli-reference),
[env vars](https://code.claude.com/docs/en/env-vars) and [changelog](https://code.claude.com/docs/en/changelog).

1. **`--system-prompt` doesn't stop CLAUDE.md loading.**
   - The [memory docs](https://code.claude.com/docs/en/memory) say CLAUDE.md "is delivered as a user message after the system
     prompt".
   - Running from `/tmp` avoids the *project* file only. The user's `~/.claude/CLAUDE.md`, auto memory, skills, plugins and
     `SessionStart` hooks likely reach every dictation. That costs tokens and time, and it's an injection path.
   - **`--safe-mode`** (verified in `--help`) disables CLAUDE.md, skills, plugins, hooks, MCP and output styles, and keeps
     auth and model selection working. It's the bare mode that keeps the subscription.
   - To check: compare `usage.input_tokens` in the `result` event with and without it.
2. **Why a call failed.** stream-json reports:
   - `rate_limit_event` (`status` `allowed|allowed_warning|rejected`, `resetsAt`, `utilization`);
   - `assistant.error` (`rate_limit`, `authentication_failed`, `billing_error`…);
   - `result.api_error_status`;
   - `system/api_retry`.
   
   Today `claude_send` turns any error into exit 1. The app could show "Claude limit reached · resets 3:45 PM" and fall back
   to S1-mini right away.
3. **Fail fast:**
   - `CLAUDE_CODE_MAX_RETRIES=1` (the default is 10);
   - `CLAUDE_CODE_STARTUP_FAILURE_RESULTS=1`;
   - `DISABLE_AUTOUPDATER=1`;
   - `ENABLE_CLAUDEAI_MCP_SERVERS=false`.
   
   Try `DISABLE_TELEMETRY` and `DISABLE_ERROR_REPORTING` behind the eval.
4. **`--include-partial-messages`:**
   - streams tokens, so the overlay can show the text as it arrives;
   - a missing `message_start` about 3 s after sending means a stall, so fall back early;
   - don't paste progressively: it breaks undo and the clipboard restore.
5. **Smaller changes:**
   - `--system-prompt-file` keeps the prompt out of `ps`;
   - `--disable-slash-commands`;
   - `--effort low` for non-Haiku models;
   - log `ttft_ms` and `duration_api_ms`.
   
   Prompt caching doesn't apply: our prompt is below Haiku's 4,096-token minimum.
6. **Pre-start is the right design.** The TypeScript Agent SDK's `startup()` pre-warms the CLI the same way. Claude has no server
   or ACP mode that would be faster. A warm *spare* process (recycled every 10 minutes or on a mode change) would also cover
   short dictations, where the 2.5 s startup isn't done when the transcript is.
7. **For M6:**
   - **Codex:** `codex app-server` (JSON-RPC over stdio, experimental) keeps one process and opens an ephemeral thread per
     dictation. It reports `UsageLimitExceeded` and has `account/rateLimits/read`. `codex exec --ephemeral
     --skip-git-repo-check --ignore-user-config --sandbox read-only -c model_reasoning_effort="low" --json -` is the one-shot
     form. Users report 10–20 s cold starts.
   - **Gemini:** paid or enterprise only (`-e none`, `GEMINI_SYSTEM_MD`, `--acp`).

## 5. Command Mode in other apps

| App | Trigger | Output | No selection | Undo and follow-ups | Notes |
|---|---|---|---|---|---|
| **Wispr Flow** Command Mode | its own key, Fn+Ctrl, hold | direct replace | does nothing | none | **Development stopped** ([staff, Apr 2026](https://reddit.com/r/WisprFlow/comments/1syvshk/boostingunderstanding_command_mode_capabilities/oizhofv/)); "servers are busy"; fails silently; English only; paid |
| Wispr **Transforms** (May 2026) | preset hotkeys (⌥1, ⌥2, 8 custom), no voice | replace; **View Diff** (⌥O) with Copy, Retry, Undo | n/a | Undo, Retry, re-transform | 1–1,000 words; up to 5 writing samples per Transform |
| **Typeless** Speak to Edit / Ask Anything | its own key, Fn+Space | editable: replace; **read-only: answer card, selection untouched** | question: a card; since v2.8.0 (2026-09-22), writes text | none documented | free on every plan; praised for summarizing web pages |
| **Aqua** Edit Mode (July 2026) | **the dictation key; a selection switches it to editing** | direct replace; a chip shows "12 words selected" | normal dictation | **stacked undo by voice**, and history of original, result and instruction | over 6,000 characters falls back to dictation; a dictated corrected version counts as the replacement |
| **VoiceInk** Rewrite / Assistant | a mode | paste over the selection / answer in its panel | polishes the dictation | Assistant keeps a session | no target check; silent failures (#968, #973); a Haiku refusal pasted (#349) |
| **OpenWhispr** Voice Agent | its own key | replace; otherwise the cursor, or a panel if no field | at the cursor if editable, else panel and clipboard | none | the most careful: re-reads the selection before pasting, same app and identical text; 5-minute single-use session; 6,000-character cap; refuses terminals; guards against line copies |
| **Willow** Scribe | Fn+Ctrl | replace | drafts at the cursor, reads the thread | not documented | users call it faster and better than Wispr's |
| Superwhisper | modes (Super Mode) | paste | dictation | none | its docs advise against rewrite, summarize or expand |
| Apple Writing Tools | menu or typed | full support: inline with versions and Revert; limited: a panel with Copy and Replace | n/a | Previous/Next, Revert | not voice |
| talon-ai-tools | `model <prompt> this` | replace, above, below, clipboard, window, **chain** | n/a | `model and…` continues, `chain` selects the result | MIT |

Non-voice tools (Raycast, PopClip, RewriteBar, Elephas, Grammarly) show a preview or choices, because you're already at the
keyboard. **Voice tools replace directly.**

**What users expect:**
- a clear message for every failure, never a silent one;
- one-step undo, and the original kept;
- seeing that a selection was captured before speaking;
- follow-ups ("shorter", "no, more formal", "go back");
- read-only text gives a copyable answer, never a paste;
- no selection writes at the cursor;
- tone, translation, lists, spelled-out names ("T-O-N-I") and numbers;
- a clear refusal over the size limit.

**Openings for us:**
- **Reliability:** no shared "servers are busy" queue on your own Claude. Verify the target, keep the original, restore the
  clipboard even on failure.
- **Speed:** the pre-started Claude. Publish the latency; nobody else does.
- **Follow-ups and the last dictation:** nobody does these well (Aqua's voice undo and edit-the-last-dictation on iOS come closest).
- **Privacy you can check:** audio stays local, context is opt-in and visible. Typeless's context-harvesting reports and Wispr's
  sub-processors are weak points for them.
- **Free, and in any language:** Wispr's is paid and English only.

## 6. macOS implementation findings

- **Reading the selection.** Accessibility first:
  - `kAXSelectedTextAttribute` on the focused element;
  - for WebKit and Chromium web text, `AXSelectedTextMarkerRange` plus `AXStringForTextMarkerRange`
    ([SelectedTextKit](https://github.com/tisfeng/SelectedTextKit), MIT).
  
  Where AX fails:
  - "success but empty": VS Code, JetBrains, iTerm2;
  - unsupported: Word, Pages, Keynote, Numbers, Firefox, Sublime;
  - Chromium, Slack and Claude Desktop return -25212 either way
    ([Easydict](https://github.com/tisfeng/Easydict), OpenWhispr).
- **The copy fallback.**
  - Press **Edit ▸ Copy through Accessibility** (`AXPress`, found by `AXIdentifier == "copy:"`). It sends no key event, so
    there's no beep and no held modifiers.
  - If the item is disabled, nothing is selected, and we stop.
  - Use ⌘C only when there's no menu item.
  - Write a unique marker to the clipboard first, so stale contents can't pass for the selection.
  - Poll `changeCount` every 5 ms for up to 0.25 s (0.4 s in Safari, 0.5 s in Word).
  - **VS Code, Cursor, JetBrains, Sublime and Xcode copy the whole line when nothing is selected**
    (`editor.emptySelectionClipboard`). A single line ending in a newline from those apps means no selection.
  - Muting the alert through System Events needs the Automation permission; the menu check avoids it.
- **Held modifiers leak.** `Paster` posts ⌘V from a `.combinedSessionState` source, so with the ⌃⌥ hotkey still held it can
  arrive as ⌃⌥⌘V. Wait until `CGEventSource.flagsState` shows the modifiers released (up to 1 s), or use the menu item.
  **This affects Hold to Talk dictation today.**
- **Choosing ⌘C and ⌘V key codes.** Pick them from the active keyboard layout (Dvorak, AZERTY), as
  [FluidVoice #387](https://github.com/altic-dev/FluidVoice/pull/387) does.
- **Checking the target before pasting.** Keep the pid, the focused window and element, the range and the selected text. Use
  `CFEqual` to compare `AXUIElement`s (Hammerspoon does). Web apps can rebuild their elements, so the same pid, window and
  selected text is enough. Set `AXUIElementSetMessagingTimeout` to 0.3–0.5 s, and make AX calls off the main thread.
- **Restoring the clipboard.** Restore only after the paste has landed: OpenWhispr's fixed 450 ms timer pasted the *old*
  clipboard in slow apps ([#2251](https://github.com/OpenWhispr/openwhispr/pull/2251)). Mark our writes with
  `org.nspasteboard.TransientType`, `AutoGeneratedType` and `source` ([nspasteboard.org](http://nspasteboard.org)) so Maccy,
  Raycast and translate-on-copy tools ignore them. `Paster.swift` doesn't do this today.
- **Pasteboard privacy.** A future macOS will alert when an app programmatically reads the general pasteboard
  ([AppKit updates](https://developer.apple.com/documentation/updates/appkit#macOS-pasteboard-privacy)). The copy fallback and
  `Paster`'s clipboard snapshot both read it. Test with `defaults write io.github.mahfuzur.openvoicetype
  EnablePasteboardPrivacyDeveloperPreview -bool yes`. Check `accessBehavior` at runtime; it's not in the 14.2 SDK headers.
- **Electron and Chrome.**
  - `AXManualAccessibility` works on a 2 s debounce, and in VS Code it switches on Screen Reader Optimized mode
    ([vscode #282290](https://github.com/microsoft/vscode/issues/282290)). Set it when an Electron app becomes active, skip
    VS Code, and make it optional.
  - **Never** `AXEnhancedUserInterface`: it breaks window managers.
  - Chrome turns on basic AX when any client reads its role. The first query can be empty, so retry after 150 ms.
- **Terminals.** Pasting runs newlines, so there's nothing to replace. Copy the result instead.
