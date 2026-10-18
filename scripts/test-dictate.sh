#!/usr/bin/env bash
# Tests dictate.sh's app contract on macOS and Linux without a microphone, Whisper or a Claude login: `refine` with
# cleanup off, the pre-started and one-shot Claude calls (scripts/testdata/fake-claude), a usage limit, offline, the
# online check, Apple's on-device engines (scripts/testdata/fake-ovt-apple), the server pid check and log rotation. Runs in a temporary HOME, so your logs and settings are untouched.
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
# No Apple helper (a build of it in the repo would otherwise be found), except in the Apple tests below.
export VTT_APPLE_HELPER=off
FAKE_APPLE="$REPO_DIR/scripts/testdata/fake-ovt-apple"
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

# Apple's on-device engines (macOS only; on Linux they're never available).
if [[ "$(uname -s)" == Darwin ]]; then
  # The availability check is cached for a minute; each test fakes a different state.
  apple() { VTT_APPLE_HELPER="$FAKE_APPLE" VTT_RESULT_FILE="$rf" APPLE_STATUS_TTL=0 "$@"; }
  out="$(printf 'please send the report by friday' | apple env VTT_CLEANUP=apple bash "$D" refine)"
  check "apple cleanup" "$? $out" "0 please send the report by friday (apple)"
  check "apple cleanup: result engine" "$(json engine "$rf")" apple
  grep -q 'apple=[0-9]*ms refine=apple' "$LOGS/dictate.log" && r=yes || r=no
  check "apple cleanup: logged" "$r" yes
  out="$(printf 'please send the report by friday' | apple env VTT_CLEANUP=apple FAKE_APPLE_LLM=appleIntelligenceNotEnabled \
    bash "$D" refine)"
  check "apple cleanup unavailable" "$? $out $(json error "$rf")" "3 please send the report by friday config"
  out="$(printf 'please send the report by friday' | apple env VTT_CLEANUP=apple FAKE_APPLE_FAIL=refusal bash "$D" refine)"
  check "apple cleanup refused" "$? $(json error "$rf")" "3 error"
  out="$(printf 'please send the report by friday' | apple env VTT_CLEANUP=apple VTT_MODE=code bash "$D" refine)"
  check "apple cleanup skips code mode" "$? $out" "0 please send the report by friday"
  # The fallback: Apple's model replaces Claude (exit 4), and the online engine's reason is kept for the overlay.
  out="$(printf 'please send the report by friday' | apple env FAKE_MODE=limit CLAUDE_PRESTART=off bash "$D" refine)"
  check "apple fallback" "$? $out" "4 please send the report by friday (apple)"
  check "apple fallback: result" "$(json engine "$rf") $(json error "$rf")" "apple limit"
  out="$(printf 'please send the report by friday' | apple env FAKE_MODE=limit CLAUDE_PRESTART=off FAKE_APPLE_FAIL=refusal \
    bash "$D" refine)"
  check "apple fallback failed, no S1-mini" "$? $(json error "$rf")" "3 limit"
  out="$(printf 'please send the report by friday' | apple env FAKE_MODE=limit CLAUDE_PRESTART=off FALLBACK_ENGINE=s1 \
    bash "$D" refine)"
  check "fallback forced to S1-mini (not installed)" "$?" 3
  out="$(printf 'please send the report by friday' | apple env FAKE_MODE=limit CLAUDE_PRESTART=off VTT_S1_FALLBACK=off \
    bash "$D" refine)"
  check "fallback off" "$?" 3
  # Speech: a finished WAV (one second of silence; the fake doesn't listen), and the live stream.
  wav="$SANDBOX/one-second.wav"
  perl -e 'print pack("A4VA4A4VvvVVvvA4V", "RIFF", 36 + 32000, "WAVE", "fmt ", 16, 1, 1, 16000, 32000, 2, 16, "data", 32000), "\0" x 32000' >"$wav"
  out="$(apple env VTT_SPEECH_ENGINE=apple bash "$D" transcribe "$wav")"
  check "apple transcribe" "$? $out" "0 Hello from Apple."
  grep -q 'TRANSCRIBE audio=1[.0]*s apple=' "$LOGS/dictate.log" && r=yes || r=no
  check "apple transcribe: logged" "$r" yes
  apple env VTT_SPEECH_ENGINE=apple FAKE_APPLE_FAIL=speech bash "$D" transcribe "$wav" >/dev/null 2>&1
  grep -q 'WARN Apple speech unavailable or failed, using Whisper' "$LOGS/dictate.log" && r=yes || r=no
  check "apple transcribe falls back to Whisper" "$r" yes
  rm -f "$STATE/apple-status"
  apple env VTT_SPEECH_ENGINE=apple FAKE_APPLE_SPEECH=supported bash "$D" transcribe "$wav" >/dev/null 2>&1
  check "apple transcribe: model not downloaded" "$(grep -c 'WARN Apple speech unavailable or failed' "$LOGS/dictate.log")" 2
  out="$(head -c 64000 /dev/zero | apple bash "$D" transcribe-stream)"
  check "apple stream" "$? $out" "0 Hello from the stream."
  grep -q 'TRANSCRIBE audio=2.00s apple=42ms stream=on' "$LOGS/dictate.log" && r=yes || r=no
  check "apple stream: logged" "$r" yes
  out="$(head -c 8000 /dev/zero | apple bash "$D" transcribe-stream)"
  check "apple stream: too short" "$? [$out]" "0 []"
  out="$(head -c 64000 /dev/zero | apple env FAKE_APPLE_SPEECH=none bash "$D" transcribe-stream)"
  check "apple stream unavailable" "$? [$out]" "2 []"
  out="$(head -c 64000 /dev/zero | apple env FAKE_APPLE_FAIL=speech bash "$D" transcribe-stream)"
  check "apple stream failed" "$? [$out]" "2 []"
  out="$(apple env FAKE_APPLE_LLM=modelNotReady bash "$D" apple status)"
  check "apple status" "$(printf '%s' "$out" | perl -MJSON::PP -0777 -ne 'print decode_json($_)->{llm}{reason}')" modelNotReady
else
  out="$(printf 'please send the report by friday' | VTT_APPLE_HELPER="$FAKE_APPLE" VTT_CLEANUP=apple VTT_RESULT_FILE="$rf" \
    bash "$D" refine)"
  check "apple cleanup unavailable on Linux" "$? $(json error "$rf")" "3 config"
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
