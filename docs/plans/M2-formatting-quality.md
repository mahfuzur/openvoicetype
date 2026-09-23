# M2: Formatting Quality, Detailed Plan

**Goal:** dictated text should come out the way a careful person would have typed it. That means digits for numbers,
real email addresses and URLs, self-corrections applied, lists and paragraphs where they belong, and the right style for the
app you're typing into. It must never change the meaning or invent values.

**Status legend:** ☐ to do · ◐ in progress · ☑ done

## 1. Baseline: what's wrong today (2026-09-23 test run)

These are real test dictations, all recorded through Bluetooth earbuds (16 kHz call-quality audio). The "Layer" column
shows where each error came from: the transcription (ASR, Whisper) or the cleanup model (LLM, Claude).

| # | Test | Raw Whisper → cleaned output | Problems | Layer |
|---|---|---|---|---|
| 1 | Postgres/AWS | `fifteen thousand four hundred dollars` stayed as words | Numbers not converted to digits | LLM |
| 1 | Postgres/AWS | `Posters` → Postgres ✓, `via slow` → Slack ✓, `leads` → needs ✓ | Fixed by Claude, but fragile | ASR |
| 1 | Postgres/AWS | `development` (spoken: deployment) | A real mis-hearing that can't be recovered | ASR |
| 2 | Meeting | `Friday. Know what? Make that Thursday` → `Friday. Actually, make that Thursday` | Self-correction not applied | LLM |
| 2 | Meeting | `half past 2 pm` kept | Time not converted to 2:30 PM | LLM |
| 2 | Meeting | `42. No, sorry, 43.` kept | Self-correction not applied | LLM |
| 2 | Meeting | `superheeshpert.com` | Name misheard; needs vocabulary | ASR |
| V1 | Emails/URLs | *(no transcript, the recording was cancelled)* | Retest; "at"/"dot" handling is unverified | ? |
| V2 | Code | `process underscore order` → `process_order` ✓, `through` → throw ✓ | Good | ✓ |
| V3 | Corrections | → `three MacBooks and four iPads for the sales team by Tuesday morning` ✓ | Good (inconsistent with #2) | ✓ |
| V4 | Numbers | `Monday 12th 2027` (spoken: May 12th) kept, `9:30 am` in lower case | Date misheard; AM/PM inconsistent | ASR + LLM |
| log | Grocery list | `Make a grocery list: 1 kg banana, 1 kg apple, 1 kg egg.` | Should be a list | LLM |
| log | 81 s dictation | One huge paragraph | Should be split into paragraphs | LLM |
| log | Claude | `cloud` | Name misheard; needs vocabulary | ASR |

**Takeaways**
1. About half the errors come from Whisper on low-quality audio. Better input (vocabulary, a style prompt, the built-in mic)
   is cheaper than asking the LLM to guess.
2. The LLM prompt is too short. It has no rules for numbers, self-corrections, spoken symbols, lists or paragraphs,
   and no examples, so its behaviour is inconsistent from run to run.
3. There's no way to measure quality, so every prompt change is guesswork. **The eval harness comes first.**

## 2. Research notes

- **OpenAI Whisper prompting guide:** the `--prompt` text steers *style* (punctuation, capitalization, digits) and
  *spelling of names and jargon*. Only the last 224 tokens count, natural sentences work better than bare lists,
  and it can't change what was actually said.
  Risk: on silent audio, Whisper sometimes outputs the prompt itself. The hallucination filter and `-sns` guard against that.
- **VoiceInk (GPL-3) cleanup prompt:** its useful ideas are: keep meaning and tone; apply clear self-corrections and drop the
  correction words; apply spoken formatting cues; digits for numbers with standard forms for dates, times, currency,
  email addresses, URLs and code, and **never guess unclear values**; paragraphs of at most about 3 sentences;
  vertical lists for clear enumerations but prose for ordinary mentions; a custom vocabulary context; few-shot examples.
  *License note:* our prompt is written from scratch, and no VoiceInk text is copied.
- **Wispr Flow / Superwhisper:** context awareness (the app you're in, names on screen), a style per app category
  (formal or casual; casual drops the trailing period), and "backtrack" phrases ("scratch that", "actually").

## 3. Design: three layers

```
audio ─► [A] Whisper + style/vocabulary prompt ─► raw
raw   ─► [B] Claude with prompt v2 (rules + mode + context + examples) ─► text
text  ─► [C] deterministic post-processing (dictionary replacements, output filter) ─► [D] paste (rich text for lists)
```

### A. Transcription (Whisper)
- **Style prompt:** a short fake transcript in the style we want (full punctuation, digits, $, %, "2:30 PM", an email address)
  followed by the vocabulary as a natural sentence. Kept under about 200 tokens.
- **Vocabulary** comes from the dictionary file (below).
- Stays on the built-in mic by default; the Bluetooth quality warning is already in the menu.

### B. Cleanup (Claude): prompt v2
Prompt files move out of the bash script into `prompts/`, bundled with the app:
- `prompts/system.md`: the core rules (meaning, cleanup, self-corrections, numbers, spoken symbols, spoken commands,
  structure, output format) and about 8 few-shot examples. The examples deliberately differ from the eval cases,
  so passing the eval isn't just memorisation.
- `prompts/modes/{default,chat,email,code,notes}.md`: style instructions added for each mode.
- The user message becomes `<context app="Slack" mode="chat"/>`, then `<vocabulary>…</vocabulary>`, then `<transcript>…</transcript>`.
- The model stays **Haiku** by default. The eval compares it with Sonnet.

### C. Deterministic post-processing (in the script)
- **Dictionary replacements** from `~/.config/voice-to-text/dictionary.txt`, applied case-insensitively on whole words, after Claude
  (and to the raw text when cleanup is off).
- **Output filter:** strip wrapping quotes or code fences the model adds, stray `<transcript>` tags, and leading
  "Here is…" lines. Normalize whitespace and keep at most one blank line between blocks.

### D. Paste (the app)
- **Rich paste:** if the text contains `- ` or `1. ` list lines, the clipboard also gets HTML (`<ul>` / `<ol>`), so Notes,
  Mail, Google Docs and Slack show real lists. Terminals and code editors get plain text only.

### Modes (app-aware)

| Mode | Apps (bundle IDs) | Style |
|---|---|---|
| `chat` | Slack, Teams, Discord, WhatsApp, Messages, Telegram | Casual; one-liners have no trailing period; lists only if dictated |
| `email` | Mail, Outlook, Spark, Superhuman | Full sentences and paragraphs; a dictated greeting and sign-off go on their own lines |
| `code` | VS Code, Cursor, Xcode, JetBrains, Terminal, iTerm2, Warp, Ghostty | Exact identifiers, file names and commands; technical casing; Markdown lists allowed |
| `notes` | Notes, Notion, Obsidian, Bear, Pages, Word | Lists and paragraphs; headings only if dictated |
| `default` | Everything else, including browsers | Balanced |
| `raw` | Chosen manually | Skip Claude |

The menu gets **Mode ▸ Auto (by app) / Default / Chat / Email / Code / Notes / Raw**. Auto is the default.
Browsers stay `default`, because we can't see which web app is open.

### Dictionary file (`~/.config/voice-to-text/dictionary.txt`)
```
# One term per line: it's passed to Whisper and Claude as spelling guidance.
Claude Code
Superwhisper
PostgreSQL
# Replacements: "heard => wanted", applied after cleanup.
cloud code => Claude Code
super whisper => Superwhisper
```
The existing `VOCAB` config variable still works and is merged in.

## 4. Eval harness

- `evals/cases.json` holds about 20 cases. Each has `id`, `mode`, `input` (a realistic raw Whisper transcript), an optional `say`
  (text to synthesize for the end-to-end run), an optional `vocabulary`, and checks:
  `contains`, `not_contains`, `regex`, `min_list_items`, `min_paragraphs`, `max_sentences_per_paragraph`.
- `evals/run.py [--model haiku|sonnet] [--case ID] [--e2e] [--runs N] [--jobs 4]`:
  - The default mode feeds `input` to `dictate.sh refine`, which tests the LLM layer on its own.
  - `--e2e` synthesizes `say` → WAV, then runs `dictate.sh transcribe` and `refine`, which tests layers A to C together.
  - It prints a pass/fail table with latency, shows what failed for each case, and saves a JSON report in `evals/results/`
    (not committed to git).
- Cases come from the baseline table (real transcripts) plus coverage for each rule: emails and URLs, spoken
  commands, lists, paragraphs, prompt injection, vocabulary, chat and email modes, and "don't over-format two items".

## 5. Tasks

| ID | Task | Status |
|---|---|---|
| M2.1 | Eval harness: `evals/cases.json` (about 20 cases) and `evals/run.py`; record a baseline with the current prompt | ☑ |
| M2.2 | Move prompts to `prompts/`; the script finds them in the repo and in the app bundle; the app bundles them | ☑ |
| M2.3 | Prompt v2: core rules and examples; iterate until the eval pass rate is ≥ 90% on Haiku | ☑ |
| M2.4 | Mode prompts and the `VTT_MODE` / `VTT_APP` context in the script | ☑ |
| M2.5 | Dictionary file: vocabulary into the Whisper prompt and the Claude context; replacements in post-processing | ☑ |
| M2.6 | Whisper style prompt; compare end-to-end runs with and without it | ☑ |
| M2.7 | Output filter and whitespace normalization | ☑ |
| M2.8 | App: detect the frontmost app and pick the mode, plus a Mode menu; pass `VTT_MODE` and `VTT_APP` | ☑ |
| M2.9 | App: rich paste (HTML lists), and plain text in code mode | ☑ |
| M2.10 | App: log each outcome (pasted, cancelled, no speech, failed) with mode and app | ☑ |
| M2.11 | Docs (README, CLAUDE.md, ROADMAP.md) and final eval numbers | ☑ |

## 5b. Results (2026-09-23)

| Run | Pass rate | Median time |
|---|---|---|
| Baseline (old one-paragraph prompt) | 6/20 (30%) | 9.3 s |
| Prompt v2 with default thinking | 11/20 (55%), many time-outs | 14.1 s |
| + `MAX_THINKING_TOKENS=0` | 17/20 (85%) | 6.1 s |
| + email lower-casing, chat period rule, paragraph example | 19/20 (95%) | 6.3 s |
| + paragraph guard, 3 runs × 20 | 59/60 (98%) | 5.4 s |
| **Final** | **20/20 (100%)** | **4.9 s** |
| Sonnet, for comparison | 19/20 (95%) | 5.5 s |
| End-to-end (`say` → Whisper → cleanup) | 5/6; remaining miss: Whisper hears "john dot doe" as "john.do" | 9–11 s |

**Lessons**
- **Extended thinking was the biggest problem.** Haiku spent up to 2,500 thinking tokens on a cleanup (30 s).
  Turning it off made it faster *and* better, because there were no more time-outs falling back to raw text.
- Rules the model follows unreliably are enforced in code: email lower-casing, the chat trailing period, and paragraph splitting.
- The Whisper style prompt fixed punctuation on fast speech and got "AWS" recognized; without it, the correction test came out as
  "We should order. Um. 5 laptops. No. Wait."

## 6. Acceptance criteria

- **Quality:** eval pass rate of **≥ 90% on Haiku** (for example 18 of 20), with every "must never" check passing in every run:
  no answering the transcript, no invented values, no meaning changes.
- **Your test cases:** 1, 2, V1 to V4, the grocery list and the long dictation all produce the expected output.
- **Latency:** the median Claude time rises by no more than about 15% over the baseline (the prompt gets longer).
- **Speed for short dictations:** under 4 words, cleanup is still skipped.

## 7. Risks

| Risk | Mitigation |
|---|---|
| The longer prompt makes it slower | Measure it in the eval; keep the examples short; M3 (a persistent session) removes most CLI overhead anyway |
| Over-formatting (lists or paragraphs where prose was meant) | "Two items stay in prose" and "short dictations stay one paragraph" cases in the eval |
| The model guesses values (a month, an amount) | A "never guess" rule, and eval checks that nothing is invented |
| The Whisper prompt leaks onto silent audio | Hallucination filter, `-sns`, and an eval case with silence |
| Overfitting the prompt to the eval | Examples differ from the cases; add new real dictations to the eval over time |

## 8. Out of scope (later)
- Selected-text and clipboard context, and on-screen names (Accessibility read). Planned for M4.
- A dictionary that learns from your edits.
- Per-app custom prompts in a settings window. Planned for M4.
