# M3: Native Pipeline and Speed, Detailed Plan

**Goal:** text is pasted **≤ 3 s** after you stop speaking, for 15 s of speech with Claude cleanup, and **≤ 1.5 s** with
S1-mini. Today it takes about 6 s with Claude. People stop using a dictation tool that makes them wait.

**Status legend:** ☐ to do · ◐ in progress · ☑ done

## 1. Baseline (2026-09-24)

Measured on an M3 Pro (18 GB) with `say` recordings of 8.5 s and 14.9 s, whisper.cpp 1.9.1 and Claude Code 2.1.280.

| Stage | Today | How |
|---|---|---|
| Whisper | 1.6–2.0 s | `whisper-cli` loads the 1.6 GB model on every call |
| Claude cleanup | 3.4–4.1 s (up to 8 s in daily use) | A new `claude -p` process per dictation; most of it is CLI startup |
| S1-mini cleanup | 0.1–0.3 s warm, about 1 s cold | Already a local server (M2.5) |
| Paste | < 0.3 s | No change needed |
| **Stop → pasted, Claude** | **6.0 s** (self-test) | |
| **Stop → pasted, S1-mini** | **2.7 s** (self-test, cold server) | |

## 2. Spike results (2026-09-24)

These measurements decide the design, so they were done before writing this plan.

| Experiment | Result |
|---|---|
| `whisper-server`, model kept loaded | **0.81–0.91 s** per request (vs 1.6–2.0 s). Ready 0.6 s after launch. 1.9 GB RSS. The per-request `prompt` field works ("AWS" recognized). |
| One long-lived `claude -p --input-format stream-json`, 5 dictations | First 2.65 s (includes startup), then **0.84–0.94 s** each. **But the history grows** by about 80 input tokens per dictation: every earlier transcript stays in the conversation. |
| A `claude` process **started ahead of time and used once** | **0.96–1.03 s** after a 3–6 s head start, with a fresh context every time (2,164 input tokens, no history). |
| An idle `claude` process | 231 MB RSS. |

**Conclusion:** the CLI's startup cost (about 2.5 s) can be moved off the critical path. Start the `claude` process when
recording **starts**. By the time recording and transcription finish, it's ready, and each process serves exactly one
dictation. That gives the speed of a persistent session without its history (no earlier dictation can leak into a later
one, and the context never grows).

## 3. Design

```
hotkey down ─► record ─────────────────────────────► stop ─► whisper-server (warm) ─► transcript ─┐
         └─► pre-start: whisper-server (if stopped) + `dictate.sh refine` with its claude process    │
                                                         claude ready and waiting ◄──────────────────┘
                                                         ─► cleaned text ─► post-process ─► paste
```

### Decision: keep the bash pipeline

The roadmap planned Swift `Transcriber`/`Refiner` protocols for M3. The spike shows they aren't needed for speed:
both wins come from keeping processes warm, which `dictate.sh` can do (it already manages S1-mini's server).
Keeping one pipeline also keeps the CLI and the app in sync, and it's the smaller change. The Swift protocols move to M4,
where the provider choice (Codex, Gemini, Ollama, APIs) actually needs them.

### A. `whisper-server` managed by `dictate.sh`

- Generalize M2.5's `s1-server` code into one helper for both servers: pid file, `--keep` flag, last-used stamp, idle
  watchdog. It adds `dictate.sh whisper-server start [--keep] | release | stop | status`.
- `whisper-server -m "$WHISPER_MODEL" --host 127.0.0.1 --port 8179 -nt -sns -l "$LANGUAGE"`.
- `transcribe()` posts the WAV to `/inference` with `response_format=text`, `temperature=0` and the same style and vocabulary
  `prompt` as today. If the server isn't healthy, it falls back to `whisper-cli`, so a dictation is never lost.
- **Memory (1.9 GB):** the server starts when you press the hotkey (it's ready in 0.6 s, well before you stop speaking) and stops
  after `WHISPER_IDLE_MINUTES` (default 10) without use. The first dictation after a pause costs nothing extra.

### B. Pre-started Claude in `dictate.sh refine`

- `refine` starts `claude -p --input-format stream-json --output-format stream-json --verbose` with today's flags
  (`--model`, `--tools ""`, `--strict-mcp-config`, `--no-session-persistence`, `--system-prompt`,
  `MAX_THINKING_TOKENS=0`, cwd `/tmp`) **before** reading the transcript from stdin. Then it waits.
- When the transcript arrives, it sends one `{"type":"user",…}` message with today's user message (context, vocabulary,
  transcript), reads events until `{"type":"result"}`, closes Claude's stdin, and runs `post_process` as today.
- Implemented with a fifo held open as fd 3 (the app runs `/bin/bash` 3.2, which has no `coproc`). Claude's output goes to a
  file that is polled 20 times a second for the `result` event; Perl encodes and decodes the JSON.
- The offline check runs when `refine` starts. If Claude is unreachable, no process is started and S1-mini is warmed instead.
- Everything else stays: the `CLAUDE_TIMEOUT` alarm (now measured from when the transcript is sent), fallback to S1-mini,
  then raw text, and exit codes 3 and 4.
- Short utterances (< 4 words) and raw mode close the unused process straight away. Cancel (Esc) or "no speech" closes
  `refine`'s stdin with no text; the script sees an empty transcript and exits, and Claude exits with it.

### C. App: start early, feed late

- **Hotkey down:** `Dictation.start()` also launches `dictate.sh whisper-server start` (async) and
  `dictate.sh refine` with stdin kept open, using the mode of the frontmost app at that moment.
- **Stop:** transcribe as today, then write the transcript to the waiting `refine` process and close its stdin.
- **Mode changed between start and stop** (you switched apps while speaking): terminate the pre-started `refine` and run a
  fresh one for the new mode. It's slower (cold), but correct. Rare.
- **App quit:** stop both servers, and terminate any waiting `refine`.
- The CLI (`dictate.sh toggle`) gets the warm `whisper-server` when it's running, and otherwise works as today.

### D. Push-to-talk

- Carbon `kEventHotKeyReleased` for the same hotkey; no new permission needed.
- Menu: **Hotkey Mode ▸ Toggle (press to start and stop) / Hold to Talk**. Toggle stays the default.
- In Hold mode, a press shorter than 0.3 s is ignored (an accidental tap), so it doesn't produce a "No speech" flash.

### E. Stage timings

- The app logs one line per dictation: `APP TIMING stop→transcript=…ms transcript→cleaned=…ms cleaned→pasted=…ms total=…ms`.
- `evals/run.py --e2e --timing` reports the median end-to-end time, so speed regressions show up like quality regressions.

## 4. Expected result

| Stage | Today | After M3 |
|---|---|---|
| Whisper (15 s of speech) | 1.6–2.0 s | about 0.9 s |
| Claude | 3.4–4.1 s | about 1.0 s |
| Paste and overheads | about 0.3 s | about 0.3 s |
| **Total, Claude** | **about 6 s** | **about 2.2 s** |
| **Total, S1-mini** | **about 2.7 s** | **about 1.4 s** |

## 5. Tasks

| ID | Task | Status |
|---|---|---|
| M3.1 | Generalize the `s1-server` lifecycle into a shared server helper (pid, keep, idle watchdog); S1-mini uses it unchanged | ☑ |
| M3.2 | `whisper-server` commands; `transcribe()` posts to `/inference`, falling back to `whisper-cli`; compare output with `whisper-cli` on the e2e eval | ☑ |
| M3.3 | `refine`: pre-started `claude` stream-json process (fifo on fd 3); timeout, fallback and exit codes unchanged; eval stays 20/20 on Haiku | ☑ |
| M3.4 | App: start `whisper-server` and `refine` on hotkey down, feed the transcript on stop, handle cancel, no speech and a mode change | ☑ |
| M3.5 | App: stop the servers and the waiting `refine` on quit | ☑ |
| M3.6 | Push-to-talk (Carbon key release) and the Hotkey Mode menu | ☑ |
| M3.7 | `APP TIMING` log line; `evals/run.py --e2e --timing` | ☑ |
| M3.8 | Measure real dictations with Claude and with S1-mini; record the results here | ☑ 9 real app dictations (below); S1-mini measured in the e2e eval only |
| M3.9 | Docs: README (timings, memory), CLAUDE.md (pipeline, gotchas), ROADMAP, CHANGELOG | ☑ |

## 5b. Results (2026-09-24)

`evals/run.py --e2e --timing --jobs 1`, which runs like the app: `refine` starts first, `say` synthesizes the speech (as long as
a short recording), then the WAV is transcribed and the transcript is fed to the waiting `refine`. Times are from "recording stops"
to "cleaned text ready"; the app adds 0.1–0.3 s for pasting.

| Run | Transcribe | Cleanup | **Total (median)** | Quality |
|---|---|---|---|---|
| Before M3 (`whisper-cli`, one-shot Claude), 6 cases | 1.9 s | 4.0 s | **5.9 s** (one 24 s outlier: cold `whisper-cli`) | 5/6 |
| M3, Claude Haiku, 12 dictations | 1.1 s | 1.3 s | **2.5 s** (2.2–3.4 s) | 10/12 |
| M3, S1-mini, 12 dictations | 1.1 s | 0.4 s | **1.5 s** (1.0–1.9 s) | 4/12 |

- The Claude failures are both the known M2 miss (Whisper hears "john dot doe" as "john.do"). S1-mini's score matches its
  M2.5 baseline (no vocabulary, code mode skipped).
- `evals/run.py` (cleanup only) is unchanged: Haiku 20/20, S1-mini 10/20.
- `whisper-server` and `whisper-cli` produced identical transcripts on both test recordings.
- **Bug found while testing:** Claude Code exports `CLAUDE_PID`, and the first version of the pre-start used that name, so an
  inherited pid was killed during cleanup (it ended the developer's Claude Code session). The script's variables are now
  `PRESTART_*`, reset at startup. A test with a decoy `CLAUDE_PID` process confirms it survives.
- Also fixed: S1-mini requests and responses were not UTF-8 safe ("café" would have come back garbled).
### Real dictations in the app (2026-09-24)

Nine dictations with a real mic, from the `APP TIMING` lines. Every one used the pre-started `refine` (`prestarted=true`);
no errors or fallbacks.

| App (mode) | Audio | Whisper | Claude | **Total** |
|---|---|---|---|---|
| VS Code (code) | 8 s | 1.0 s | 1.5 s | **2.7 s** |
| Slack (chat) | 17 s | 1.0 s | 2.3 s | **3.6 s** |
| VS Code (code) | 22 s | 1.2 s | 2.1 s | **3.5 s** |
| Slack (chat) | 18 s | 1.0 s | 4.5 s | **5.7 s** |
| Notes (notes) | 46 s | 2.2 s | 2.6 s | **5.1 s** |
| VS Code (code) | 58 s | 5.8 s | 3.1 s | **9.1 s** |
| Notes, 3 short phrases (Claude skipped) | 1.5–2.8 s | 0.9 s | – | **1.1 s** |

- **About twice as fast as before M3** for the same lengths (the M2-era log had 7.0 s for 28 s of speech and 9.0 s for 46 s).
- **The ≤ 3 s target is met for short dictations (under about 10 s), not yet for 15–20 s ones (3.5–5.7 s).** The synthetic
  eval used short sentences, which is why it reported 2.5 s.
- What's left is Claude *generating* the answer: the recordings were long enough that the process was always ready, so
  startup is gone. Generation grows with the text and the API varies (2.3 s and 4.5 s for two similar Slack messages).
- One Whisper spike: 5.8 s for 58 s of audio (46 s took 2.2 s). Audio over 30 s is decoded in windows and can be re-decoded
  at a higher temperature when Whisper is unsure. Not a regression.

### Ideas to go further (not in M3)

- A shorter system prompt for Claude (fewer input tokens; the prompt is about 2,100 tokens), checked with the eval.
- Use S1-mini for short, simple dictations and Claude for longer or code ones (S1-mini answers in 0.1–0.3 s).
- Show the text as Claude streams it (the stream-json output has partial messages), so the overlay isn't idle.
- Whisper: `--no-fallback` or a beam size of 1 for long audio, if the eval shows no loss.

- **Hardening after review:** a stale pid file can no longer make `stop` kill another process (name check), and a broken
  Claude stream is detected in 0–5 s instead of waiting the whole 15 s timeout (see Risks). The first version counted loop
  iterations for its deadlines, which ran about 60% slow (15 s would have been about 24 s); they now use the clock.

## 6. Acceptance criteria

- **Speed:** median stop → pasted **≤ 3 s** with Claude Haiku and **≤ 1.5 s** with S1-mini, over 10 dictations of about
  15 s each, measured with `APP TIMING`.
- **Quality unchanged:** `evals/run.py` stays 20/20 on Haiku; S1-mini stays at its 10/20 baseline or better; the e2e eval
  gives the same transcripts through `whisper-server` as through `whisper-cli`.
- **No history:** every Claude process serves exactly one dictation (checked in the log: input tokens don't grow).
- **Never lose a dictation:** stopping `whisper-server` or `claude` mid-dictation still pastes text (`whisper-cli` fallback,
  then S1-mini or raw text).
- **Resources:** nothing stays loaded when idle: `whisper-server` and a fallback-started S1-mini stop after their idle time,
  and no `claude` process runs without a dictation in progress.

## 7. Risks

| Risk | Mitigation |
|---|---|
| The stream-json flags or event format change in a Claude Code update | Tested on 2.1.280. **Handled:** non-JSON output or an early exit switches to the one-shot `claude -p` call at once (3.6–4 s); a renamed `result` event uses the `assistant` answer after 1.5 s; no output at all falls back to S1-mini after 5 s instead of 15 s. Tested with fake `CLAUDE_BIN` programs |
| A stale pid file (crash, reboot) points at a reused pid, and `stop` kills an unrelated process | **Handled:** `srv_running()` checks the process name (`llama-server` / `whisper-server`) before trusting a pid. Tested with a decoy process |
| A pre-started process per hotkey press wastes work on cancelled dictations | Startup uses no model tokens; the process exits as soon as its stdin closes. Worth checking that startup makes no billable call |
| `whisper-server` holds 1.9 GB | Started on hotkey press, stopped after 10 idle minutes; `WHISPER_SERVER=off` keeps `whisper-cli` |
| A port is already in use (8179, 8178) | Configurable ports; the health check fails, so it falls back to `whisper-cli` or one-shot Claude |
| `whisper-server` transcribes differently from `whisper-cli` | Same flags and prompt; the e2e eval compares both before switching |
| Very short recordings: Claude isn't ready when the transcript arrives | It's then only as slow as today; utterances under 4 words skip Claude anyway |

## 8. Out of scope (later)

- Swift `Transcriber`/`Refiner` protocols and a native pipeline: M4, with the provider choice.
- Streaming transcription (live text while speaking): not needed for speed, since cleanup waits for the whole transcript.
- Other speech models (Parakeet, Cohere Transcribe): evaluated in September 2026 and not adopted.
- A persistent multi-dictation Claude session: rejected in the spike (history grows, earlier dictations could leak).
