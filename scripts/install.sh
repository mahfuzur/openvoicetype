#!/usr/bin/env bash
# Installs the dictation POC: links `dictate` into ~/.local/bin, seeds the config, links the model.
set -euo pipefail

export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/voice-to-text"
MODEL_DIR="$HOME/.local/share/whisper"
MODEL="$MODEL_DIR/ggml-large-v3-turbo.bin"
# Reuse an existing copy from Superwhisper if it is installed (saves a 1.6 GB download).
SUPERWHISPER_MODEL="$HOME/Library/Application Support/superwhisper/ggml-large-v3-turbo.bin"

missing=()
for cmd in rec sox soxi whisper-cli claude; do
  command -v "$cmd" >/dev/null || missing+=("$cmd")
done
if ((${#missing[@]})); then
  echo "Missing: ${missing[*]}. Install with: brew install sox whisper-cpp (and the Claude Code CLI)." >&2
  exit 1
fi

mkdir -p "$BIN_DIR" "$CONFIG_DIR" "$MODEL_DIR"

chmod +x "$REPO_DIR/scripts/dictate.sh"
ln -sf "$REPO_DIR/scripts/dictate.sh" "$BIN_DIR/dictate"
echo "Linked $BIN_DIR/dictate"

if [[ ! -f "$CONFIG_DIR/config.sh" ]]; then
  cp "$REPO_DIR/scripts/config.example.sh" "$CONFIG_DIR/config.sh"
  echo "Created $CONFIG_DIR/config.sh"
fi

if [[ ! -e "$MODEL" ]]; then
  if [[ -f "$SUPERWHISPER_MODEL" ]]; then
    ln -sf "$SUPERWHISPER_MODEL" "$MODEL"
    echo "Linked model from Superwhisper"
  else
    echo "Downloading ggml-large-v3-turbo.bin (1.6 GB)..."
    curl -L --fail -o "$MODEL" \
      https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin
  fi
fi

echo "Running self-test..."
"$BIN_DIR/dictate" selftest
