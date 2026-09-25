# OpenVoiceType guide

The details the [README](../README.md) leaves out: modes, the dictionary, cleanup engines, the command line,
configuration, how it works, and troubleshooting.

## Using it

| Action | How |
|---|---|
| Start dictating | **⌃⌥Space** (change it in **Settings → General**; any combination with ⌃ or ⌥ works) |
| Stop and paste | **⌃⌥Space** again |
| Hold to talk instead | **Settings → General → When you press it**: hold ⌃⌥Space while you speak, release to paste |
| Cancel | **Esc** while recording |
| Command Mode | **⌃⌥⇧Space**: select text, press it, say how to change it (see [Command Mode](#command-mode)) |
| Swap the last paste | **⌃⌥Z**: undoes the paste and puts Whisper's own text there instead, or back to the cleaned text |
| Copy an earlier dictation | Menu → **Recent Dictations** (the last 10, kept in memory only) or **Copy Last** |
| Change settings | Menu → **Settings…** (⌘,) |

- **Where the text goes.** It goes to the app and window you started dictating in. If you switch apps or windows (or Slack
  channels) before the text is ready, it's copied instead of pasted, and the overlay says so: press ⌘V where you want it.
- **Password fields.** Dictation doesn't start in one, and text is never pasted into one.
- **Bluetooth earbuds.** Wait for the start sound before speaking. They take about 1.5 s to switch into headset mode.

## Command Mode

Select some text, press **⌃⌥⇧Space** (change it in **Settings → General**), say what to do, and press it again (or hold it
while you speak). Before you speak, the overlay says what the command will act on:

| The overlay says | What happens |
|---|---|
| **12 words selected** | The answer replaces the selection. ⌘Z brings the original back |
| **Follow-up · 12 words** | You're changing the last result ("shorter still", "no, keep the first sentence", "go back to the original") |
| **Last dictation** | Nothing was selected, but you just dictated: it edits that ("make that Thursday") |
| **Write at cursor** | Nothing selected: it writes new text at the cursor ("write a two-line thank-you to the team") |
| **Copy only** | Text you can't edit (a web page, a PDF), a terminal, or no text field: the answer goes to the clipboard |

- Things to say: "make this shorter and more polite", "turn this into bullet points", "change 5 PM to 6 PM", "translate
  to Spanish", "it's T-O-N-I" (a spelled-out name fixes it), "fix the grammar", "summarize this" (on a web page).
- **Follow-ups** work for a minute after an edit: in most native apps just press the key again; elsewhere select the
  result first. **Menu → Restore Original Text** puts back the text from before the first edit, for 5 minutes.
- **It never guesses where to paste.** If the selection changed or you switched apps while it worked, the answer is copied
  instead ("Selection changed: result copied"). Password fields are refused.
- **What's sent:** the selected text and your spoken instruction, to Claude (or the API, in **Settings → Cleanup →
  Command Mode uses**), only when you press the key. S1-mini can't follow instructions, so Command Mode doesn't use it,
  and it doesn't work offline.
- **Up to 6,000 characters** (about 1,000 words) of selection.
- **Reading the selection.** It tries Accessibility first. Where that doesn't work, it presses the app's own Edit ▸ Copy
  (no beep), and puts your clipboard back afterwards. In a few apps (canvas editors, some terminals) it can't read the
  selection: the overlay then says **Write at cursor**, so you know before you speak.

## Modes

The mode is chosen from the app you're typing into (**Auto**). You can also fix a mode in the menu under **Mode**, and
choose the mode for any app yourself in **Settings → Modes**.

| Mode | Apps | Style |
|---|---|---|
| Chat | Slack, Teams, Discord, WhatsApp, Messages | Casual; no period at the end of one-liners |
| Email | Mail, Outlook, Spark, Superhuman | Paragraphs; dictated greetings and sign-offs on their own lines |
| Code | VS Code, Cursor, Xcode, JetBrains, Terminal, iTerm2, Warp, Ghostty | Exact file names, identifiers and commands |
| Notes | Notes, Notion, Obsidian, Bear, Pages, Word | Lists and short paragraphs |
| Default | Everything else, including browsers | Balanced |
| Raw | Chosen manually | No AI cleanup |

Lists are pasted as rich text, so apps like Notes, Mail and Slack show real bullets. Code mode always pastes plain text.

## Dictionary

Add words in **Settings → Dictionary**. It's saved in `~/.config/voice-to-text/dictionary.txt`, which you can also edit by hand:

```
# One term per line: Whisper and the cleanup will spell it exactly like this
Kubernetes
PostgreSQL

# Replacements, applied after cleanup: heard => wanted
cloud code => Claude Code
```

## Cleanup engines

Pick one in **Settings → Cleanup** or the menu's **Clean Up With**.

| Engine | Where it runs | Notes |
|---|---|---|
| **Claude** (default) | Your own Claude Code CLI, signed in with your Claude account | Best quality in our eval. Uses your plan's normal usage limits. See [TERMS.md](TERMS.md) |
| **An OpenAI-compatible API** | Ollama or LM Studio on your Mac, or OpenAI, Groq, OpenRouter… with your key | Same prompt as Claude. Quality depends a lot on the model: small local models (3–4B) do much worse (see the eval in the README). The key is kept in your Keychain |
| **S1-mini** | On your Mac (llama.cpp) | 0.6B, English only, no internet. It can't use your vocabulary or format code, so code mode keeps Whisper's text |
| Off | | Whisper's text as is (the dictionary still applies) |

With Claude or an API selected, **S1-mini is the fallback** when the engine can't be used: no internet, signed out, a usage
limit, an error or a timeout. The overlay says why ("Claude limit reached · resets 3:45 PM · cleaned offline").

**The meaning guard.** After any AI cleanup, every number and every negation ("not", "never", "can't", "without"…) in
Whisper's text must still be there. If one is missing and you didn't correct yourself ("no, I mean…"), Whisper's text is
pasted instead, and the overlay says what went missing. **⌃⌥Z** swaps to the cleaned version if you want it anyway. It's a
safety net for the kind of error that matters most, not a guarantee: a model can still change a word.

**The transcript is data.** The prompt tells the model to clean up what you said and never to act on it: "write me a poem"
comes back as that sentence. That's a mitigation, not a guarantee. The eval has cases for it (below), and the model has no
tools, so the worst case is wrong text, which ⌃⌥Z or ⌘Z undoes.

## Command line

The app is built around the script `scripts/dictate.sh`, which also works on its own. Install it as `dictate` with
`brew install sox whisper-cpp` and `./scripts/install.sh` (add `--with-s1-mini` for offline cleanup):

```bash
dictate            # toggle: start recording / stop and paste
dictate cancel     # stop and discard
dictate selftest   # synthetic speech through the whole pipeline (no mic, no paste)
dictate file x.wav # transcribe and clean up an existing recording
```

To use a hotkey without the app, bind `~/.local/bin/dictate` to a keyboard shortcut in the Shortcuts app or with
[skhd](https://github.com/koekeishiya/skhd). Give Microphone and Accessibility permission to whichever app runs it.

## Configuration

`~/.config/voice-to-text/config.sh` holds the Whisper model, language, vocabulary, cleanup engine, timeouts, and on/off
switches. Every option is documented in [scripts/config.example.sh](../scripts/config.example.sh). Everyday settings are in
the Settings window, and they win over the config file.

## How it works

```
hotkey ─► note where the text should go (app, window, mode)
       ─► record (AVAudioEngine, 16 kHz WAV)
       ─► transcribe on-device (whisper.cpp kept loaded in a local whisper-server + a style prompt and your vocabulary)
       ─► clean up: claude -p (pre-started while you speak; no tools, no MCP, --safe-mode, extended thinking off),
          or an OpenAI-compatible endpoint, or S1-mini through a local llama-server
       ─► meaning guard (numbers and negations kept, else Whisper's text) and post-processing (dictionary, output filter)
       ─► paste at the cursor if focus didn't move (⌘V; your clipboard comes back once the app has read the paste)
```

- **Prompts** live in [`prompts/`](../prompts/): the core rules are in `system.md`, and each mode has a file in `modes/`.
- **Claude** runs as your own `claude` CLI in print mode, from a neutral folder, with no tools, no MCP servers and
  `--safe-mode`. That keeps your personal CLAUDE.md, memory, skills and hooks out of every dictation. An exported
  `ANTHROPIC_API_KEY` is ignored, so dictation never quietly bills the API (set `CLAUDE_USE_API_KEY=on` if you want that).
  One Claude process handles one dictation, so nothing carries over between them.
- **Memory:** `whisper-server` uses about 750 MB with the compressed model (1.7 GB with the full one). It's loaded when you
  press the hotkey and unloaded after 10 idle minutes. S1-mini uses about 1 GB: kept loaded while it's selected, and stopped
  10 minutes after a fallback.

## Logs and privacy

- `~/Library/Logs/voice-to-text/dictate.log` records timings and outcomes: the engine, why it failed, and where the text went.
- **Dictated text isn't logged** unless you turn on **Settings → About → Keep dictated text in the log** (or set
  `LOG_TEXT=on`). With it off, text an older version logged is removed.
- The logs rotate at 1 MB (one previous file is kept), and only you can read them.

## Troubleshooting

| Problem | Fix |
|---|---|
| "OpenVoiceType can't be opened" / "Apple could not verify" | Expected the first time, because the app isn't notarized: **System Settings → Privacy & Security → Open Anyway**. |
| "Copied: you switched to …" | Focus moved while the text was being prepared. Press ⌘V where you want it. |
| "Copied. Press ⌘V", or Accessibility is switched on but Settings says it's needed | The switch belongs to an older copy with a different signature (turning it off and on doesn't help). **Settings → General → Permissions → Reset…**, then switch OpenVoiceType on in the list that opens. |
| "Pasted Whisper's text: the cleanup dropped …" | The meaning guard caught a missing number or "not". ⌃⌥Z swaps to the cleaned text if it was right. |
| "Claude limit reached · resets …" | Your Claude plan's usage limit. S1-mini cleans up until then if it's installed, or pick another engine. |
| "Claude isn't signed in" | **Settings → Cleanup → Sign In…** |
| "Claude Code changed how scripts sign in" | `claude -p` refused your login while `claude auth status` says you're signed in. Claude Code has probably changed its print-mode defaults: update OpenVoiceType, and check the log. |
| "No speech detected" every time | Microphone access is missing, or the wrong mic is selected. Try **Microphone ▸ Test Microphone…**. |
| Bluetooth mic says "No audio from …" | Pick the built-in mic under **Microphone**, or reconnect the earbuds. |
| "Pasted without cleanup" | The engine failed and S1-mini isn't installed. **Settings → Cleanup** has a **Test Cleanup** button that says why. |
| The first dictation after installing or updating is slow | The app prepares the speech engine for your Mac once (10–20 s) in the background after launch. |
| Came from **Voice to Text** 0.1.x, and the hotkey or pasting doesn't work | The app was renamed, which macOS treats as a new app. Grant **Microphone** and **Accessibility** again, and quit the old app if it's still in the menu bar. |
| Something else | Check `~/Library/Logs/voice-to-text/dictate.log`, and open an issue. |
