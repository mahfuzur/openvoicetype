# Security

OpenVoiceType records your voice, checks which app is in front, pastes into other apps and runs your Claude Code CLI, so
security problems matter even in a small project.

## Reporting a problem

Please report it **privately** through GitHub:
[Security → Report a vulnerability](https://github.com/mahfuzur/openvoicetype/security/advisories/new).
Don't open a public issue. The maintainer aims to reply within a week and to release a fix as soon as possible, with
credit if you'd like it.

## What counts

- Audio or text leaving the Mac other than as described in the README's Privacy section (only the transcript, only to
  Claude Code, only while Claude cleanup is selected).
- A dictated transcript making Claude act or answer instead of just tidying the text (prompt injection), or reaching
  tools or MCP servers.
- Tampering with the bundled helpers, the downloaded models (they're checked against pinned SHA-256 hashes) or the update
  check.
- Anything that reads, stores or exposes your Claude login. The app must never touch it (see [docs/TERMS.md](docs/TERMS.md)).
- Local privilege problems: files written with the wrong permissions, or commands built from untrusted text.

## Supported versions

Only the latest release gets fixes. The app checks for updates once a day (**Settings → About**).
