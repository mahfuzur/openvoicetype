#!/usr/bin/env bash
# Tests dictate.sh's app contract on macOS, Linux and Windows (Git Bash) without a microphone, Whisper or a Claude login: `refine` with
# cleanup off, the pre-started and one-shot Claude calls (scripts/testdata/fake-claude), a usage limit, offline, the
# online check, the server pid check and log rotation. Runs in a temporary HOME, so your logs and settings are untouched.
#
#   scripts/test-dictate.sh        (CI runs it on all three; on Windows, from Git Bash)
#   DICTATE=target/debug/ovt scripts/test-dictate.sh
#                                  the same contract against the Rust pipeline (crates/ovt-pipeline); the checks of the
#                                  script's own internals (server pid files, log rotation, its online check) are skipped
set -uo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
D="$REPO_DIR/scripts/dictate.sh"
DICTATE="${DICTATE:-}"
[[ -z "$DICTATE" || "$DICTATE" == /* ]] || DICTATE="$PWD/$DICTATE"
# Runs the implementation under test: dictate.sh, or the program in $DICTATE.
dictate() { if [[ -n "$DICTATE" ]]; then "$DICTATE" "$@"; else bash "$D" "$@"; fi; }
SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT
export HOME="$SANDBOX/home" TMPDIR="$SANDBOX/tmp" XDG_RUNTIME_DIR="$SANDBOX/run"
export TEMP="$SANDBOX/tmp" APPDATA="$SANDBOX/roaming" LOCALAPPDATA="$SANDBOX/local" # Windows' folders
unset XDG_CONFIG_HOME XDG_DATA_HOME XDG_STATE_HOME VTT_CONFIG
export VTT_CLAUDE_BIN="$REPO_DIR/scripts/testdata/fake-claude" VTT_QUIET=on
case "$(uname -s)" in MINGW* | MSYS* | UCRT* | CLANG*) WINDOWS=on ;; *) WINDOWS=off ;; esac
# A native Windows program (ovt.exe) can't run a bash script: it gets the .cmd wrapper.
if [[ "$WINDOWS" == on && -n "$DICTATE" ]]; then
  VTT_CLAUDE_BIN="$(cygpath -w "$REPO_DIR/scripts/testdata/fake-claude.cmd")"
fi
# The prompts the program reads (ovt finds them next to itself in an install, not in a build folder).
export VTT_PROMPTS_DIR="$REPO_DIR/prompts"
# No connection to api.anthropic.com before each fake Claude call, and a default route even with no network (stubs in
# ~/.local/bin, which dictate.sh puts first on its PATH), so the tests also pass offline; the check itself is tested
# below with the real commands. English dates for the reset-time check.
export ONLINE_CHECK=off LC_ALL=C
mkdir -p "$HOME/.local/bin"
printf '#!/bin/sh\necho "default via 192.0.2.1 dev eth0"\n' > "$HOME/.local/bin/ip"
printf '#!/bin/sh\necho "   route to: default"\n' > "$HOME/.local/bin/route"
chmod +x "$HOME/.local/bin/ip" "$HOME/.local/bin/route"
mkdir -p "$HOME" "$TMPDIR" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
if [[ "$WINDOWS" == on ]]; then
  STATE="$TMPDIR/voice-to-text" LOGS="$LOCALAPPDATA/voice-to-text/logs"
  # Windows paths, for a native program under test (dictate.sh converts them back).
  TEMP="$(cygpath -w "$TEMP")" APPDATA="$(cygpath -w "$APPDATA")" LOCALAPPDATA="$(cygpath -w "$LOCALAPPDATA")"
elif [[ -n "$DICTATE" ]]; then
  # A program under test on macOS or Linux follows the XDG folders (ovt-core's paths).
  STATE="$XDG_RUNTIME_DIR/voice-to-text" LOGS="$HOME/.local/state/voice-to-text"
elif [[ "$(uname -s)" == Linux ]]; then
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
out="$(printf 'mail me at Bob@Example.COM today' | VTT_REFINE=off dictate refine)"
check "refine with cleanup off" "$? $out" "0 mail me at bob@example.com today"
if [[ -z "$DICTATE" ]]; then
  # Git for Windows mounts drives without POSIX permissions (noacl): every folder reads as 755.
  [[ "$WINDOWS" == on ]] || check "state folder is private" "$(mode "$STATE")" 700
  check "log in the platform's folder" "$([[ -f "$LOGS/dictate.log" ]] && echo yes)" yes
fi

# The pre-started Claude (stream-json), with the result file. Timed: the script's own cost per dictation (the fake
# Claude answers at once), to compare the platforms (process creation is slow on Windows).
rf="$STATE/result-test.json"
started="$(perl -MTime::HiRes=time -e 'printf "%d", time*1000')"
out="$(printf 'please send the report by friday' | VTT_RESULT_FILE="$rf" dictate refine)"
check "pre-started Claude" "$? $out" "0 PLEASE SEND THE REPORT BY FRIDAY"
echo "info  refine with a pre-started fake Claude took $(($(perl -MTime::HiRes=time -e 'printf "%d", time*1000') - started)) ms"
check "result file: engine" "$(json engine "$rf")" claude

# The one-shot call.
out="$(printf 'please send the report by friday' | CLAUDE_PRESTART=off dictate refine)"
check "one-shot Claude" "$? $out" "0 PLEASE SEND THE REPORT BY FRIDAY"

# The CLI's options are probed once per binary: a second run keeps the cache.
cache="$STATE/claude-options"
before="$(mtime "$cache")"
sleep 1.1
printf 'please send the report by friday' | dictate refine >/dev/null
check "claude options cached" "$(mtime "$cache")" "$before"
check "claude options found" "$(tail -n +2 "$cache" | tr '\n' ' ')" "--safe-mode --disable-slash-commands --system-prompt-file "

# A usage limit: Whisper's text, exit 3, and the reset time formatted for the overlay.
out="$(printf 'please send the report by friday' | FAKE_MODE=limit CLAUDE_PRESTART=off VTT_S1_FALLBACK=off \
  VTT_RESULT_FILE="$rf" dictate refine)"
check "usage limit" "$? $out" "3 please send the report by friday"
check "result file: error" "$(json error "$rf")" limit
resets="$(json resets "$rf")"
check "reset time formatted" "$([[ "$resets" =~ ^[A-Z][a-z]{2}\ [0-9]{1,2},\ [0-9]{1,2}:[0-9]{2}\ [AP]M$ ]] && echo ok || echo "$resets")" ok

# Offline: Claude is skipped.
out="$(printf 'please send the report by friday' | VTT_OFFLINE=on VTT_S1_FALLBACK=off VTT_RESULT_FILE="$rf" dictate refine)"
check "offline" "$? $(json error "$rf")" "3 offline"

if [[ -n "$DICTATE" ]]; then
  echo "$passed passed, $failed failed (the script's internals skipped for $DICTATE)"
  ((failed == 0))
  exit
fi

# The online check itself (needs the internet, as CI has; skipped with no network at all).
eval "$(sed -n '/^has_default_route()/,/^}/p; /^is_offline()/,/^}/p; /^tcp_reachable()/,/^}/p' "$D")"
# shellcheck disable=SC2034 # read by the functions eval'd above
OS="$(uname -s)" ONLINE_CHECK=on ONLINE_CHECK_HOST=api.anthropic.com
if has_default_route; then
  check "default route" yes yes
  is_offline && r=offline || r=online
  check "online check: reachable host" "$r" online
  is_offline 10.255.255.1 443 && r=offline || r=online
  check "online check: dead host" "$r" offline
else
  echo "skip  online check (no network)"
fi

# Linux without XDG_RUNTIME_DIR: a private folder one level deep in /tmp (the Rust app's paths::state_dir agrees).
if [[ "$(uname -s)" == Linux ]]; then
  fallback="/tmp/voice-to-text-$(id -u)"
  printf 'x' | env -u XDG_RUNTIME_DIR VTT_REFINE=off bash "$D" refine >/dev/null
  check "state folder without XDG_RUNTIME_DIR" "$(mode "$fallback")" 700
fi

# srv_running: our server's pid counts, a reused pid doesn't, and an upgraded (deleted) binary still does.
eval "$(sed -n '/^srv_file()/p; /^srv_binary()/p; /^srv_running()/,/^}/p' "$D")"
STATE_DIR="$SANDBOX/servers"
mkdir -p "$STATE_DIR"
# A process named whisper-server. Linux reads the real executable from /proc, so it needs a copy: of perl, since the
# Rust coreutils (Ubuntu 25.10+) are one program that picks its tool by name and wouldn't run as "whisper-server".
# macOS reports the path that was run, and kills a copied system binary (its signature no longer matches): a symlink.
# Windows: a copy of sleep.exe; /proc/<pid>/exe ends in .exe.
if [[ "$WINDOWS" == on ]]; then
  cp "$(command -v sleep)" "$SANDBOX/whisper-server.exe"
  "$SANDBOX/whisper-server.exe" 30 &
elif [[ "$(uname -s)" == Linux ]]; then
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
if [[ "$WINDOWS" == off ]]; then # Windows can't delete a running program
  rm "$SANDBOX/whisper-server"
  srv_running whisper-server && r=yes || r=no
  check "server pid check: binary replaced" "$r" yes
fi
kill "$server" "$other" 2>/dev/null
wait 2>/dev/null

# Log rotation at LOG_MAX_KB.
head -c 1100000 /dev/zero | tr '\0' x >"$LOGS/dictate.log"
printf 'x' | VTT_REFINE=off dictate refine >/dev/null
check "log rotated" "$([[ -f "$LOGS/dictate.log.1" ]] && echo yes)" yes

echo "$passed passed, $failed failed"
((failed == 0))
