#!/usr/bin/env bash
# Tests dictate.sh's app contract on macOS and Linux without a microphone, Whisper or a Claude login: `refine` with
# cleanup off, the pre-started and one-shot Claude calls (scripts/testdata/fake-claude), a usage limit, offline, the
# online check, the server pid check and log rotation. Runs in a temporary HOME, so your logs and settings are untouched.
#
#   scripts/test-dictate.sh        (CI runs it on both platforms)
set -uo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
D="$REPO_DIR/scripts/dictate.sh"
SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT
export HOME="$SANDBOX/home" TMPDIR="$SANDBOX/tmp" XDG_RUNTIME_DIR="$SANDBOX/run"
unset XDG_CONFIG_HOME XDG_DATA_HOME XDG_STATE_HOME VTT_CONFIG
export VTT_CLAUDE_BIN="$REPO_DIR/scripts/testdata/fake-claude" VTT_QUIET=on
mkdir -p "$HOME" "$TMPDIR" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
if [[ "$(uname -s)" == Linux ]]; then
  STATE="$XDG_RUNTIME_DIR/voice-to-text" LOGS="$HOME/.local/state/voice-to-text"
else
  STATE="$TMPDIR/voice-to-text" LOGS="$HOME/Library/Logs/voice-to-text"
fi

passed=0 failed=0
check() {
  if [[ "$2" == "$3" ]]; then
    echo "ok    $1"
    passed=$((passed + 1))
  else
    echo "FAIL  $1: got [$2], want [$3]"
    failed=$((failed + 1))
  fi
}
json() { perl -MJSON::PP -0777 -ne "print decode_json(\$_)->{$1}" "$2"; }
mode() { perl -e 'printf "%o", (stat $ARGV[0])[2] & 07777' "$1"; }
mtime() { perl -e 'print ((stat $ARGV[0])[9])' "$1"; }

# Cleanup off: only post-processing (email addresses are lower-cased).
out="$(printf 'mail me at Bob@Example.COM today' | VTT_REFINE=off bash "$D" refine)"
check "refine with cleanup off" "$? $out" "0 mail me at bob@example.com today"
check "state folder is private" "$(mode "$STATE")" 700
check "log in the platform's folder" "$([[ -f "$LOGS/dictate.log" ]] && echo yes)" yes

# The pre-started Claude (stream-json), with the result file.
rf="$STATE/result-test.json"
out="$(printf 'please send the report by friday' | VTT_RESULT_FILE="$rf" bash "$D" refine)"
check "pre-started Claude" "$? $out" "0 PLEASE SEND THE REPORT BY FRIDAY"
check "result file: engine" "$(json engine "$rf")" claude

# The one-shot call.
out="$(printf 'please send the report by friday' | CLAUDE_PRESTART=off bash "$D" refine)"
check "one-shot Claude" "$? $out" "0 PLEASE SEND THE REPORT BY FRIDAY"

# The CLI's options are probed once per binary: a second run keeps the cache.
cache="$STATE/claude-options"
before="$(mtime "$cache")"
sleep 1.1
printf 'please send the report by friday' | bash "$D" refine >/dev/null
check "claude options cached" "$(mtime "$cache")" "$before"
check "claude options found" "$(tail -n +2 "$cache" | tr '\n' ' ')" "--safe-mode --disable-slash-commands --system-prompt-file "

# A usage limit: Whisper's text, exit 3, and the reset time formatted for the overlay.
out="$(printf 'please send the report by friday' | FAKE_MODE=limit CLAUDE_PRESTART=off VTT_S1_FALLBACK=off \
  VTT_RESULT_FILE="$rf" bash "$D" refine)"
check "usage limit" "$? $out" "3 please send the report by friday"
check "result file: error" "$(json error "$rf")" limit
resets="$(json resets "$rf")"
check "reset time formatted" "$([[ "$resets" =~ ^[A-Z][a-z]{2}\ [0-9]{1,2},\ [0-9]{1,2}:[0-9]{2}\ [AP]M$ ]] && echo ok || echo "$resets")" ok

# Offline: Claude is skipped.
out="$(printf 'please send the report by friday' | VTT_OFFLINE=on VTT_S1_FALLBACK=off VTT_RESULT_FILE="$rf" bash "$D" refine)"
check "offline" "$? $(json error "$rf")" "3 offline"

# The online check itself (needs the internet, as CI has).
eval "$(sed -n '/^has_default_route()/,/^}/p; /^is_offline()/,/^}/p; /^tcp_reachable()/,/^}/p' "$D")"
# shellcheck disable=SC2034 # read by the functions eval'd above
OS="$(uname -s)" ONLINE_CHECK=on ONLINE_CHECK_HOST=api.anthropic.com
has_default_route && r=yes || r=no
check "default route" "$r" yes
is_offline && r=offline || r=online
check "online check: reachable host" "$r" online
is_offline 10.255.255.1 443 && r=offline || r=online
check "online check: dead host" "$r" offline

# srv_running: our server's pid counts, a reused pid doesn't, and an upgraded (deleted) binary still does.
eval "$(sed -n '/^srv_file()/p; /^srv_binary()/p; /^srv_running()/,/^}/p' "$D")"
STATE_DIR="$SANDBOX/servers"
mkdir -p "$STATE_DIR"
# A process named whisper-server. Linux reads the real executable from /proc, so it needs a copy: of perl, since the
# Rust coreutils (Ubuntu 25.10+) are one program that picks its tool by name and wouldn't run as "whisper-server".
# macOS reports the path that was run, and kills a copied system binary (its signature no longer matches): a symlink.
if [[ "$(uname -s)" == Linux ]]; then
  cp "$(command -v perl)" "$SANDBOX/whisper-server"
  "$SANDBOX/whisper-server" -e 'sleep 30' &
else
  ln -s "$(command -v sleep)" "$SANDBOX/whisper-server"
  "$SANDBOX/whisper-server" 30 &
fi
server=$!
sleep 30 &
other=$!
echo "$server" >"$STATE_DIR/whisper-server.pid"
srv_running whisper-server && r=yes || r=no
check "server pid check" "$r" yes
echo "$other" >"$STATE_DIR/whisper-server.pid"
srv_running whisper-server && r=yes || r=no
check "server pid check: another process" "$r" no
echo "$server" >"$STATE_DIR/whisper-server.pid"
rm "$SANDBOX/whisper-server"
srv_running whisper-server && r=yes || r=no
check "server pid check: binary replaced" "$r" yes
kill "$server" "$other" 2>/dev/null
wait 2>/dev/null

# Log rotation at LOG_MAX_KB.
head -c 1100000 /dev/zero | tr '\0' x >"$LOGS/dictate.log"
printf 'x' | VTT_REFINE=off bash "$D" refine >/dev/null
check "log rotated" "$([[ -f "$LOGS/dictate.log.1" ]] && echo yes)" yes

echo "$passed passed, $failed failed"
((failed == 0))
