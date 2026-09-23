# shellcheck shell=bash
# voice-to-text config, sourced by dictate.sh.
# Installed at ~/.config/voice-to-text/config.sh. Every setting is optional; defaults are shown.

# Whisper model (ggml format).
# WHISPER_MODEL="$HOME/.local/share/whisper/ggml-large-v3-turbo.bin"

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

# Log raw and cleaned text in ~/Library/Logs/voice-to-text/dictate.log
# LOG_TEXT="on"
