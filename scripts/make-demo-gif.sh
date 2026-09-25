#!/usr/bin/env bash
# Makes the README demo GIF (docs/images/demo.gif) from a screen recording.
#
#   scripts/make-demo-gif.sh [seconds] [x,y,w,h]   records the screen (a region, or the whole main display), then converts
#   scripts/make-demo-gif.sh --from <video.mov>     converts a recording you made yourself (⌘⇧5 → Record Selected Portion)
#
# Recording needs Screen Recording permission for your terminal (System Settings → Privacy & Security). Converting needs
# ffmpeg (brew install ffmpeg). What to record is in docs/ARTWORK.md ("Demo GIF").
# WIDTH (default 800) and FPS (default 12) set the output; aim for under 5 MB so GitHub shows it quickly.
set -euo pipefail

export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${OUT:-$REPO_DIR/docs/images/demo.gif}"
WIDTH="${WIDTH:-800}"
FPS="${FPS:-12}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

command -v ffmpeg >/dev/null || { echo "ffmpeg not found (brew install ffmpeg)" >&2; exit 1; }

if [[ "${1:-}" == --from ]]; then
  VIDEO="${2:?usage: make-demo-gif.sh --from <video.mov>}"
  [[ -f "$VIDEO" ]] || { echo "No such file: $VIDEO" >&2; exit 1; }
else
  SECONDS_TO_RECORD="${1:-15}"
  VIDEO="$WORK/recording.mov"
  region=()
  [[ -n "${2:-}" ]] && region=(-R "$2")
  for count in 3 2 1; do
    printf 'Recording %s s in %s…\r' "$SECONDS_TO_RECORD" "$count"
    sleep 1
  done
  echo "Recording for $SECONDS_TO_RECORD s: dictate now.        "
  # -v video, -V length, -C shows the cursor; -x no sound effects.
  # Without Screen Recording permission it fails with "capture error": don't let set -e hide the hint below.
  screencapture -x -v -C -V "$SECONDS_TO_RECORD" "${region[@]+"${region[@]}"}" "$VIDEO" || true
  [[ -s "$VIDEO" ]] || {
    echo "Nothing was recorded: grant Screen Recording to the app this terminal runs in (Terminal, iTerm, VS Code…)," >&2
    echo "quit and reopen it, then try again. Or record with ⌘⇧5 and use --from <video.mov>." >&2
    exit 1
  }
fi

# Two passes: build a palette from the whole clip, then map every frame to it. Much sharper and smaller than one pass.
filters="fps=$FPS,scale=$WIDTH:-1:flags=lanczos"
ffmpeg -v error -y -i "$VIDEO" -vf "$filters,palettegen=stats_mode=diff" "$WORK/palette.png"
ffmpeg -v error -y -i "$VIDEO" -i "$WORK/palette.png" \
  -lavfi "$filters [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" -loop 0 "$OUT"

size="$(stat -f %z "$OUT")"
echo "Wrote ${OUT#"$REPO_DIR"/} ($((size / 1024)) KB)"
if ((size > 5 * 1024 * 1024)); then
  echo "Over 5 MB: try a shorter clip, WIDTH=640 or FPS=10." >&2
fi
echo "Then uncomment the demo line near the top of README.md."
