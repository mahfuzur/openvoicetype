#!/usr/bin/env bash
# Installs the `dictate` command-line tool (for scripting, Shortcuts or a hotkey daemon). You don't need this for the
# app: the DMG has everything, and its setup window downloads the models.
#
#   scripts/install.sh [--with-s1-mini] [--full-model]
#
# Links `dictate` into ~/.local/bin, creates the config, downloads the compressed Whisper model (574 MB, the app's
# default; the same file the app uses, so it's downloaded once), and runs the self-test. --with-s1-mini also installs
# llama.cpp and S1-mini (484 MB) for offline cleanup; --full-model downloads the full 1.6 GB model instead.
# Models that are already there aren't downloaded again. Needs Homebrew's sox and whisper-cpp.
set -euo pipefail

export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/voice-to-text"
MODEL_DIR="$HOME/.local/share/whisper"
S1_DIR="$HOME/.local/share/s1-mini"
# Pinned revisions and checksums, the same as the app's model manager (app/Sources/VoiceToText/ModelManager.swift).
WHISPER_REPO="https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1"
S1_REPO="https://huggingface.co/superwhisper/s1-mini-GGUF/resolve/34add00a48a2e5d24e5a4ee5405a99620a3a240c"

with_s1=off
model_name=ggml-large-v3-turbo-q5_0.bin model_sha=394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2
model_size="574 MB"
for arg in "$@"; do
  case "$arg" in
    --with-s1-mini) with_s1=on ;;
    --full-model)
      model_name=ggml-large-v3-turbo.bin model_sha=1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69
      model_size="1.6 GB"
      ;;
    -h | --help) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown option: $arg (see --help)" >&2; exit 2 ;;
  esac
done
[[ "${INSTALL_S1:-}" == on ]] && with_s1=on # the old switch

missing=()
for cmd in rec sox whisper-cli; do
  command -v "$cmd" >/dev/null || missing+=("$cmd")
done
if ((${#missing[@]})); then
  echo "Missing: ${missing[*]}. Install them with: brew install sox whisper-cpp" >&2
  echo "(Only the command-line tool needs them. For the app, download the DMG instead: see the README.)" >&2
  exit 1
fi
command -v claude >/dev/null ||
  echo "Note: the Claude Code CLI isn't installed, so cleanup falls back to Whisper's text (or S1-mini)." >&2

# download <url> <file> <sha256> <label>: skipped if the file is there; checked before it gets its final name.
download() {
  local url="$1" file="$2" sha="$3" label="$4"
  [[ -e "$file" ]] && { echo "Already there: $file"; return 0; }
  mkdir -p "$(dirname "$file")"
  echo "Downloading $label..."
  # A finished .part from an interrupted run is used as is (resuming it would get HTTP 416); a broken one starts over.
  if [[ ! -f "$file.part" || "$(shasum -a 256 "$file.part" | cut -d' ' -f1)" != "$sha" ]]; then
    curl -L --fail --continue-at - -o "$file.part" "$url" || { rm -f "$file.part"; curl -L --fail -o "$file.part" "$url"; }
  fi
  if [[ "$(shasum -a 256 "$file.part" | cut -d' ' -f1)" != "$sha" ]]; then
    echo "The download of $label is damaged (checksum mismatch). Run the script again." >&2
    rm -f "$file.part"
    exit 1
  fi
  mv "$file.part" "$file"
}

mkdir -p "$BIN_DIR" "$CONFIG_DIR"
chmod +x "$REPO_DIR/scripts/dictate.sh"
ln -sf "$REPO_DIR/scripts/dictate.sh" "$BIN_DIR/dictate"
echo "Linked $BIN_DIR/dictate"

if [[ ! -f "$CONFIG_DIR/config.sh" ]]; then
  cp "$REPO_DIR/scripts/config.example.sh" "$CONFIG_DIR/config.sh"
  echo "Created $CONFIG_DIR/config.sh"
fi

# Either large-v3-turbo file will do (dictate.sh prefers the compressed one when both are there).
if [[ "$model_name" == ggml-large-v3-turbo-q5_0.bin && -e "$MODEL_DIR/ggml-large-v3-turbo.bin" ]]; then
  echo "Already there: $MODEL_DIR/ggml-large-v3-turbo.bin (the full model)"
else
  download "$WHISPER_REPO/$model_name" "$MODEL_DIR/$model_name" "$model_sha" "the Whisper model ($model_size)"
fi

if [[ "$with_s1" == on ]]; then
  if ! command -v llama-server >/dev/null; then
    echo "Installing llama.cpp (runs S1-mini for offline cleanup)..."
    brew install llama.cpp
  fi
  download "$S1_REPO/s1-mini-q4_k_m.gguf" "$S1_DIR/s1-mini-q4_k_m.gguf" \
    3b41ebe2502cbd03e811d5d16b022f5ab551eda58d62597d152f89535003c634 "S1-mini by Superwhisper (484 MB)"
fi

echo "Running self-test (the first run loads Whisper and compiles its Metal shaders, so it's slower)..."
"$BIN_DIR/dictate" selftest
