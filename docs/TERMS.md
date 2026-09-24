# OpenVoiceType, Claude Code and Anthropic's terms

*Last checked: 2026-09-24. This page explains how OpenVoiceType uses Claude Code. It is not legal advice, and Anthropic's
terms are what count. If they change, this page may be out of date.*

OpenVoiceType transcribes your speech on your Mac with Whisper. When cleanup with Claude is on, it asks **your own Claude
Code CLI** to tidy up the text (remove filler words, fix punctuation, format lists). It never talks to Anthropic itself.

## What the app does and doesn't do

- **It runs the `claude` program you installed**, unmodified: Anthropic's own installer puts it on your Mac, and the app
  only starts it. That's the same as typing `claude -p` in Terminal.
- **You sign in through Anthropic's own flow.** Setup opens Terminal with `claude auth login`. The app never sees your
  password.
- **It never reads, stores or sends your login tokens.** It only runs `claude auth status --json` to show whether you're
  signed in.
- **It has no servers.** Each request goes from your Mac, through your Claude Code, for your own dictation. Nobody else's
  requests go through your account, and yours don't go through anyone else's.
- **Only the transcript text is sent**, never audio, and only while cleanup with Claude is selected. Claude runs with no
  tools and no MCP servers, and is told to treat the transcript as text to tidy, never as instructions.
- **The usage is light:** one short request per dictation, by one person.

## What Anthropic's terms say

From Anthropic's [Claude Code legal and compliance page](https://code.claude.com/docs/en/legal-and-compliance):

- Subscription (OAuth) sign-in "is designed to support ordinary use of Claude Code and other native Anthropic applications",
  and the plans' usage limits "assume ordinary, individual usage".
- The terms don't prevent "an end user from signing in to the unmodified Claude Code binary with their own Claude
  subscription".
- Third-party developers may not "offer Claude.ai login into their own applications", "route requests through Free, Pro,
  or Max plan credentials on behalf of their users", or "collect, store, or intermediate Claude.ai credentials or session
  tokens".

OpenVoiceType is built to stay on the allowed side: the unmodified CLI, your own sign-in, no tokens and no servers. Still,
an app that starts your Claude Code for you isn't a case the terms spell out, so **it's your call**. Your use of Claude Code
is governed by Anthropic's [Consumer Terms](https://www.anthropic.com/legal/consumer-terms) (Free, Pro and Max) or
[Commercial Terms](https://www.anthropic.com/legal/commercial-terms) (Team, Enterprise and API), and you're responsible for
following them. Your plan's usage limits apply to dictation cleanup like any other Claude Code use.

## If you'd rather not use Claude

- **S1-mini (Settings → Cleanup):** a small cleanup model that runs on your Mac. With it, nothing leaves your Mac at all.
- **Cleanup off:** pastes Whisper's text as it is.
- Other providers (Codex, Gemini, Ollama, API keys) are planned: see the [roadmap](ROADMAP.md) (M6).

OpenVoiceType is not affiliated with or endorsed by Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic.
