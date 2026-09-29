#!/usr/bin/env bash
# Renders the diagrams in docs/diagrams with Archify (https://github.com/tt-a1i/archify, MIT), pinned below.
#
# Usage: scripts/render-diagrams.sh [--check] [name...]
#   (no names)  every docs/diagrams/*.json
#   name        one diagram, e.g. system-overview
#   --check     only validate (schema, layout, source references against the pinned commit); writes nothing
#
# For each diagram: `finalize --quality showcase` (validation, rendering, the artifact checks and a real-browser check),
# then `visual-check` for the light and dark screenshots. The page goes to docs/diagrams/<name>.html and the 1440-wide
# screenshots to docs/diagrams/images/. Archify's receipts stay in app/build/diagrams/<name>/.
# Needs Node >= 18 and Chrome or Chromium (ARCHIFY_CHROME=<path> picks one). Archify is cloned once into ARCHIFY_DIR.

set -euo pipefail
export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"

ARCHIFY_REPO="https://github.com/tt-a1i/archify.git"
ARCHIFY_REF="0e4949f910a8e390bd3b4933883a4dcabad571be" # v3.0.1 (2026-09-28)
ARCHIFY_DIR="${ARCHIFY_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/openvoicetype/archify}"
# No release check: the version is pinned, and a render shouldn't depend on the network.
export ARCHIFY_UPDATE_CHECK_DISABLED=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIAGRAMS="$ROOT/docs/diagrams"
BUILD="$ROOT/app/build/diagrams"

fail() { echo "render-diagrams: $*" >&2; exit 1; }

# Fetches only the pinned commit (the full history is about 500 MB) and checks it out.
fetch_archify() {
  if [[ ! -d "$ARCHIFY_DIR/.git" ]]; then
    mkdir -p "$ARCHIFY_DIR"
    git -C "$ARCHIFY_DIR" init --quiet || fail "couldn't set up $ARCHIFY_DIR"
    git -C "$ARCHIFY_DIR" remote add origin "$ARCHIFY_REPO" || fail "couldn't set up $ARCHIFY_DIR"
  fi
  if ! git -C "$ARCHIFY_DIR" cat-file -e "$ARCHIFY_REF^{commit}" 2>/dev/null; then
    echo "render-diagrams: fetching Archify ${ARCHIFY_REF:0:7} into $ARCHIFY_DIR" >&2
    git -C "$ARCHIFY_DIR" fetch --quiet --depth 1 origin "$ARCHIFY_REF" || fail "couldn't fetch Archify from $ARCHIFY_REPO"
  fi
  git -C "$ARCHIFY_DIR" -c advice.detachedHead=false checkout --quiet --detach "$ARCHIFY_REF" ||
    fail "couldn't check out Archify $ARCHIFY_REF"
}

archify() { node "$ARCHIFY_DIR/archify/bin/archify.mjs" "$@"; }

# The diagram_type field of a spec.
diagram_type() {
  node -e 'process.stdout.write(String(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).diagram_type))' "$1"
}

# The capture visual-check wrote for a theme at 1440 wide, read from its summary.
capture_path() {
  node -e '
    const summary = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
    const paths = JSON.stringify(summary).match(/"[^"]+\.png"/g) || [];
    const hit = paths.map((p) => JSON.parse(p)).find((p) => p.includes(process.argv[2]) && p.includes("1440"));
    if (!hit) process.exit(1);
    process.stdout.write(hit);' "$1" "$2"
}

render() {
  local name="$1" spec="$DIAGRAMS/$1.json" type work theme png
  [[ -f "$spec" ]] || fail "no such diagram: $spec"
  type="$(diagram_type "$spec")"
  if [[ -n "$CHECK" ]]; then
    archify validate "$type" "$spec" --quality showcase --repo-root "$ROOT" >/dev/null ||
      fail "$name: validation failed (run: node $ARCHIFY_DIR/archify/bin/archify.mjs validate $type $spec --quality showcase --repo-root $ROOT)"
    echo "$name: valid"
    return 0
  fi
  work="$BUILD/$name"
  rm -rf "$work"
  mkdir -p "$work" "$DIAGRAMS/images"
  # meta.output (docs/diagrams/<name>.html) resolves from the repository root.
  (cd "$ROOT" && archify finalize "$type" "$spec" "$work/$name.html" --repo-root "$ROOT" --quality showcase \
    --json >"$work/finalize.json") || fail "$name: finalize failed (see $work/finalize.json)"
  archify visual-check "$work/$name.html" --summary --require-provenance --out-dir "$work/captures" \
    >"$work/visual-check.json" || fail "$name: visual-check failed (see $work/visual-check.json)"
  # The page renders byte for byte the same, but browser captures differ slightly from run to run: keep the
  # screenshots of an unchanged page, so a re-render doesn't churn them.
  if cmp -s "$work/$name.html" "$DIAGRAMS/$name.html" && [[ -f "$DIAGRAMS/images/$name-light.png" &&
    -f "$DIAGRAMS/images/$name-dark.png" ]]; then
    echo "$name: unchanged ($type)"
    return 0
  fi
  cp "$work/$name.html" "$DIAGRAMS/$name.html"
  for theme in light dark; do
    png="$(capture_path "$work/visual-check.json" "$theme")" || fail "$name: no $theme capture in $work/visual-check.json"
    cp "$png" "$DIAGRAMS/images/$name-$theme.png"
  done
  echo "$name: rendered ($type)"
}

CHECK=""
names=()
for arg in "$@"; do
  case "$arg" in
    --check) CHECK=1 ;;
    -h | --help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) fail "unknown option: $arg" ;;
    *) names+=("${arg%.json}") ;;
  esac
done

command -v node >/dev/null || fail "Node.js is required (brew install node)"
fetch_archify
if ((${#names[@]} == 0)); then
  for spec in "$DIAGRAMS"/*.json; do
    [[ -f "$spec" ]] && names+=("$(basename "$spec" .json)")
  done
fi
((${#names[@]} > 0)) || fail "no diagrams in $DIAGRAMS"
for name in "${names[@]}"; do render "$name"; done
