#!/usr/bin/env bash
# Builds self-contained whisper-server, whisper-cli and llama-server for the app bundle: static libraries, Metal
# shaders embedded (compiled at load time, so Xcode's Metal compiler isn't needed), no OpenSSL or curl.
# Output: app/build/deps/<whisper tag>-<llama tag>/bin. Needs cmake (brew install cmake) and git.
set -euo pipefail

export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"

WHISPER_TAG="${WHISPER_TAG:-v1.9.1}"
LLAMA_TAG="${LLAMA_TAG:-v0.4.1}"

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEPS_DIR="$REPO_DIR/app/build/deps"
OUT="$DEPS_DIR/$WHISPER_TAG-$LLAMA_TAG"
SRC="$DEPS_DIR/src"
JOBS="$(sysctl -n hw.ncpu)"

if [[ -x "$OUT/bin/whisper-server" && -x "$OUT/bin/whisper-cli" && -x "$OUT/bin/llama-server" ]]; then
  echo "$OUT/bin"
  exit 0
fi
command -v cmake >/dev/null || { echo "cmake not found (brew install cmake)" >&2; exit 1; }

COMMON=(
  -DCMAKE_BUILD_TYPE=Release
  -DCMAKE_OSX_ARCHITECTURES=arm64
  -DCMAKE_OSX_DEPLOYMENT_TARGET=13.3
  -DBUILD_SHARED_LIBS=OFF
  -DGGML_METAL=ON
  -DGGML_METAL_EMBED_LIBRARY=ON
  -DGGML_NATIVE=OFF
  -DGGML_OPENMP=OFF
  -DGGML_CCACHE=OFF
)

# fetch <name> <url> <tag>: a shallow clone of one tag into $SRC/<name>-<tag>.
fetch() {
  local dir="$SRC/$1-$3"
  [[ -d "$dir" ]] || git clone -q --depth 1 --branch "$3" "$2" "$dir"
  echo "$dir"
}

mkdir -p "$SRC" "$OUT/bin" "$OUT/licenses"

whisper="$(fetch whisper.cpp https://github.com/ggml-org/whisper.cpp.git "$WHISPER_TAG")"
cmake -S "$whisper" -B "$whisper/build" "${COMMON[@]}" \
  -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_EXAMPLES=ON -DWHISPER_BUILD_SERVER=ON -DWHISPER_SDL2=OFF -DWHISPER_CURL=OFF >/dev/null
cmake --build "$whisper/build" -j "$JOBS" --target whisper-server whisper-cli >/dev/null
cp "$whisper/build/bin/whisper-server" "$whisper/build/bin/whisper-cli" "$OUT/bin/"
cp "$whisper/LICENSE" "$OUT/licenses/whisper.cpp.txt"

llama="$(fetch llama.cpp https://github.com/ggml-org/llama.cpp.git "$LLAMA_TAG")"
cmake -S "$llama" -B "$llama/build" "${COMMON[@]}" \
  -DLLAMA_CURL=OFF -DLLAMA_OPENSSL=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=ON >/dev/null
cmake --build "$llama/build" -j "$JOBS" --target llama-server >/dev/null
cp "$llama/build/bin/llama-server" "$OUT/bin/"
cp "$llama/LICENSE" "$OUT/licenses/llama.cpp.txt"

# Only system libraries may be linked, or the app won't run on a Mac without Homebrew.
for bin in "$OUT"/bin/*; do
  if otool -L "$bin" | tail -n +2 | grep -vE '^\s*(/usr/lib/|/System/)'; then
    echo "$bin links non-system libraries (above)" >&2
    exit 1
  fi
done
echo "$OUT/bin"
