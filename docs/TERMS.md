# OpenVoiceType, Claude Code and Anthropic's terms

*Last checked: 2026-09-25. This page explains how OpenVoiceType uses Claude Code. It is not legal advice, and Anthropic's
terms are what count. Anthropic's policies for third-party tools changed several times in 2026, and may change again.*

OpenVoiceType transcribes your speech on your Mac with Whisper. When cleanup with Claude is on, it asks **your own Claude
Code CLI** to tidy up the text (remove filler words, fix punctuation, format lists). It never talks to Anthropic itself.

## What the app does and doesn't do

- **It runs the `claude` program you installed**, unmodified: Anthropic's own installer puts it on your Mac, and the app
  only starts it in print mode (`claude -p`), the documented way to script it. That's the same as typing `claude -p` in
  Terminal.
- **You sign in through Anthropic's own flow.** Setup opens Terminal with `claude auth login`. The app never sees your
  password.
- **It never reads, stores or sends your login tokens.** It only runs `claude auth status --json` to show whether you're
  signed in.
- **It has no servers.** Each request goes from your Mac, through your Claude Code, for your own dictation. Nobody else's
  requests go through your account, and yours don't go through anyone else's.
- **Each request starts with you.** One short request per dictation, when you press the hotkey. Nothing runs in the
  background, on a timer or in batches.
- **Only text is sent**, never audio: the transcript while Claude is the selected cleanup, and for **Command Mode** the
  text you selected plus your spoken instruction, only when you press its key. Claude runs with no
  tools, no MCP servers and `--safe-mode` (so your own CLAUDE.md, memory, skills and hooks aren't sent either). It's told
  to treat the transcript as text to tidy, never as instructions.
- **Your plan, not the API.** If `ANTHROPIC_API_KEY` or `ANTHROPIC_AUTH_TOKEN` is set in your environment, `claude -p` would
  use it and bill the API. OpenVoiceType removes both from its calls, unless you set `CLAUDE_USE_API_KEY=on` in
  `config.sh`.

## What Anthropic says

**Allowed.** The [Claude Code legal and compliance page](https://code.claude.com/docs/en/legal-and-compliance) (updated
2026-08-21):
- The terms don't prevent "an end user from signing in to the unmodified Claude Code binary with their own Claude
  subscription".
- "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and the Agent SDK."

**Not allowed**, from the same page:
- Third-party developers may not "offer Claude.ai login into their own applications", or "route requests through Free,
  Pro, or Max plan credentials on behalf of their users".
- They may not "collect, store, or intermediate Claude.ai credentials or session tokens".
- They may not use the Claude Code or Anthropic names or logos in their own product's name.
- Anthropic "reserves the right to take measures to enforce these restrictions and may do so without prior notice".

**The grey areas:**
- The same page says: "Developers building products or services that interact with Claude's capabilities, including those
  using the Agent SDK, should use API key authentication."
- The [headless docs](https://code.claude.com/docs/en/headless) (updated 2026-09-24) describe `claude -p` as using the
  Agent SDK through the CLI.
- The [Agent SDK overview](https://code.claude.com/docs/en/agent-sdk/overview) (updated 2026-09-21) says third parties may
  not offer "claude.ai login or rate limits" for their products unless approved.
- The [Consumer Terms](https://www.anthropic.com/legal/consumer-terms) (effective 2025-10-08) forbid accessing the service
  "through automated or non-human means, whether through a bot, script, or otherwise", "except when you are accessing our
  Services via an Anthropic API Key or where we otherwise explicitly permit it". Anthropic's own documentation describes
  scripted `claude -p` use with a subscription, and each OpenVoiceType request is one you start yourself. No page addresses
  an app like this one directly.

**Closest to a yes:**
- On 2026-05-13, [@ClaudeDevs](https://x.com/ClaudeDevs/status/2054610157364289906) wrote that "third-party tools built on
  the Agent SDK like Conductor and OpenClaw work with your Claude plan".
- The [Help Center](https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan)
  (updated 2026-06-16) says a planned change that would move `claude -p` and third-party app usage to a separate monthly
  credit is paused: "nothing has changed: Claude Agent SDK, `claude -p`, and third-party app usage still draw from your
  subscription's usage limits… When we have an update, we'll share it before anything takes effect." If that change comes
  back, dictation cleanup would draw from that credit.

**What the enforcement has targeted:** in 2026, Anthropic acted against tools that reused Claude Code's login tokens or
pretended to be Claude Code. We found no case against a tool that runs the user's own, unmodified CLI.

OpenVoiceType is built to stay on the allowed side: the unmodified CLI, your own sign-in, no tokens and no servers. Still,
this isn't a case the terms spell out, so **it's your call**. Your use of Claude Code is governed by Anthropic's
[Consumer Terms](https://www.anthropic.com/legal/consumer-terms) (Free, Pro and Max) or
[Commercial Terms](https://www.anthropic.com/legal/commercial-terms) (Team, Enterprise and API), and you're responsible for
following them. Your plan's usage limits apply to dictation cleanup like any other Claude Code use. When you reach a limit,
the app says so ("Claude limit reached · resets …") and cleans up with S1-mini or leaves Whisper's text.

## A technical change to watch

Claude Code's docs say `--bare` "will become the default for `-p` in a future release". Bare mode never reads the
subscription login. If that happens, cleanup with Claude stops working until OpenVoiceType adapts. The app checks for it:
when `claude -p` says you aren't signed in but `claude auth status` says you are, it reports that Claude Code changed how
scripts sign in.

## If you'd rather not use Claude

- **An OpenAI-compatible API (Settings → Cleanup):** Ollama or LM Studio on your Mac, or OpenAI, Groq, OpenRouter and
  others, with your own key (kept in your Keychain). That service's terms apply.
- **S1-mini:** a small cleanup model that runs on your Mac. With it, nothing leaves your Mac at all.
- **Cleanup off:** pastes Whisper's text as it is.
- More providers (Codex with a ChatGPT plan first) are planned: see the [roadmap](ROADMAP.md) (M6). Gemini CLI no longer
  accepts personal Google accounts (since 2026-06-18), and its successor's terms forbid third-party tools, so Gemini will
  only work with an API key.

OpenVoiceType is not affiliated with or endorsed by Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic.
