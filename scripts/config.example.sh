# shellcheck shell=bash
# voice-to-text config, sourced by dictate.sh.
# Installed at ~/.config/voice-to-text/config.sh. Every setting is optional; defaults are shown.

# Whisper model (ggml format). Default: the compressed large-v3-turbo if it's there, else the full one.
# WHISPER_MODEL="$HOME/.local/share/whisper/ggml-large-v3-turbo-q5_0.bin"

# Keep Whisper loaded in a local whisper-server (faster; about 750 MB of memory while loaded, 1.7 GB with the full model)
# WHISPER_SERVER="on"      # off = load the model with whisper-cli on every dictation
# WHISPER_PORT=8179
# WHISPER_IDLE_MINUTES=10  # unload after this long unused (it loads again in about 0.6 s)

# Spoken language: en, bn, auto, ...
# LANGUAGE="en"

# Names and terms to spell exactly. Passed to Whisper as its prompt and to Claude.
# VOCAB="whisper.cpp, Claude Code, TypeScript, PostgreSQL"

# Claude cleanup
# REFINE="on"              # off = paste the raw Whisper text
# CLAUDE_MODEL="haiku"     # haiku is fastest; sonnet is better at heavy rewrites
# CLAUDE_TIMEOUT=15        # seconds before falling back to the raw text
# REFINE_MIN_WORDS=4       # shorter utterances skip Claude

# CLAUDE_THINKING_TOKENS=0 # extended thinking; 0 is much faster and the eval shows no quality loss
# CLAUDE_PRESTART="on"     # start claude while you speak, so cleanup takes about 1 s instead of 4 s
# CLAUDE_BIN="claude"      # path to the Claude Code CLI, if it isn't on PATH
# CLAUDE_USE_API_KEY="off" # on = let an exported ANTHROPIC_API_KEY bill the API instead of your Claude plan

# Cleanup engine: claude, openai (the endpoint below), or s1 to always clean up on this Mac
# CLEANUP="claude"

# An OpenAI-compatible endpoint (CLEANUP="openai"): Ollama, LM Studio, OpenAI, Groq, OpenRouter...
# OPENAI_BASE_URL="http://localhost:11434/v1"
# OPENAI_MODEL="llama3.2"
# OPENAI_API_KEY=""        # not needed for local servers; the app keeps it in the Keychain instead
# OPENAI_TIMEOUT=15

# Command Mode (edit selected text by voice): claude, or openai (the endpoint above). S1-mini can't follow instructions.
# COMMAND_ENGINE="claude"
# COMMAND_TIMEOUT=30       # seconds; rewriting a long selection takes longer than cleaning up a dictation

# Offline cleanup with S1-mini by Superwhisper (English only; install.sh --with-s1-mini downloads it)
# S1_FALLBACK="on"         # use S1-mini when the online engine is unavailable (offline, not logged in, error, timeout)
# S1_MODEL="$HOME/.local/share/s1-mini/s1-mini-q4_k_m.gguf"
# S1_PORT=8178             # local llama-server port
# S1_TIMEOUT=10            # seconds before falling back to the raw text
# S1_IDLE_MINUTES=10       # a server started for a fallback stops after this long unused
# ONLINE_CHECK="on"        # a 1 s connection test before Claude, so a dead internet line falls back quickly

# After an AI cleanup, paste Whisper's text instead if a number or a "not" went missing
# MEANING_GUARD="on"

# Writing style: default | chat | email | code | notes | raw (the app picks it from the focused app)
# MODE="default"

# Custom system prompt (replaces prompts/system.md; the mode instructions are still appended)
# PROMPT_FILE="$HOME/.config/voice-to-text/prompt.txt"

# Dictionary: terms (one per line) and replacements ("heard => wanted")
# DICTIONARY_FILE="$HOME/.config/voice-to-text/dictionary.txt"

# Whisper style prompt: a short sample in the wanted style (punctuation, digits), plus your vocabulary
# WHISPER_PROMPT="on"
# WHISPER_STYLE="Okay, here is the update. The meeting is on Thursday, March 3, 2026, at 2:30 PM, ..."

# Output
# PASTE="on"               # off = only copy to the clipboard
# RESTORE_CLIPBOARD="on"   # put your previous clipboard text back after pasting
# SOUNDS="on"

# Recording limits (seconds)
# MAX_SECONDS=300
# MIN_SECONDS=0.5

# Log raw and cleaned text in ~/Library/Logs/voice-to-text/dictate.log (for debugging). Off keeps timings only,
# and removes text lines an earlier version wrote.
# LOG_TEXT="off"
# LOG_MAX_KB=1024          # the log rotates at this size (one previous file is kept)
