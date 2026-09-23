#!/usr/bin/env bash
# Builds VoiceToText.app with SwiftPM (no Xcode needed) and, with --install, copies it to
# ~/Applications and launches it.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="VoiceToText"
BUNDLE_ID="io.github.mahfuzur.voicetotext"
PKG_DIR="$REPO_DIR/app"
OUT="$PKG_DIR/build/$APP_NAME.app"
INSTALL_DIR="$HOME/Applications"

swift build -c release --package-path "$PKG_DIR"
BIN_DIR="$(swift build -c release --package-path "$PKG_DIR" --show-bin-path)"

rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS" "$OUT/Contents/Resources"
cp "$BIN_DIR/$APP_NAME" "$OUT/Contents/MacOS/$APP_NAME"
cp "$PKG_DIR/Info.plist" "$OUT/Contents/Info.plist"
cp "$REPO_DIR/scripts/dictate.sh" "$OUT/Contents/Resources/dictate.sh"
cp -R "$REPO_DIR/prompts" "$OUT/Contents/Resources/prompts"
chmod +x "$OUT/Contents/Resources/dictate.sh"

# Sign with the stable local identity from setup-signing.sh so permission grants survive rebuilds.
# Falls back to ad-hoc signing, where every rebuild needs Accessibility granted again.
IDENTITY="Voice to Text Local Signing"
KEYCHAIN="$HOME/Library/Keychains/voice-to-text-signing.keychain-db"
PASS_FILE="$HOME/.config/voice-to-text/signing-keychain-password"
if [[ -f "$KEYCHAIN" && -f "$PASS_FILE" ]]; then
  security unlock-keychain -p "$(cat "$PASS_FILE")" "$KEYCHAIN"
  codesign --force --sign "$IDENTITY" --keychain "$KEYCHAIN" --identifier "$BUNDLE_ID" "$OUT"
else
  echo "warning: no stable signing identity (run scripts/setup-signing.sh); using ad-hoc signing" >&2
  codesign --force --sign - --identifier "$BUNDLE_ID" "$OUT"
fi
echo "Built $OUT"

if [[ "${1:-}" == "--install" ]]; then
  pkill -x "$APP_NAME" 2>/dev/null || true
  mkdir -p "$INSTALL_DIR"
  rm -rf "${INSTALL_DIR:?}/$APP_NAME.app"
  cp -R "$OUT" "$INSTALL_DIR/"
  echo "Installed $INSTALL_DIR/$APP_NAME.app"
  open "$INSTALL_DIR/$APP_NAME.app"
fi
