#!/usr/bin/env bash
# Builds VoiceToText.app with SwiftPM (no Xcode needed) and, with --install, copies it to
# /Applications/OpenVoiceType.app (where the DMG puts it, so there's only ever one copy) and launches it. The app bundles its own whisper-server, whisper-cli and llama-server
# (scripts/build-deps.sh, cached after the first build), so it runs on a Mac without Homebrew.
#
#   VERSION=0.2.0      sets the version (default: the one in app/Info.plist); release.sh passes the tag
#   BUNDLE_DEPS=off    skips the bundled helpers (the script then uses Homebrew's)
#   SIGN_IDENTITY=...  signs with this identity (release.sh); default: the local one from setup-signing.sh
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="VoiceToText"
BUNDLE_ID="io.github.mahfuzur.openvoicetype"
PRODUCT_NAME="OpenVoiceType" # the name users see: the installed app, the DMG
PKG_DIR="$REPO_DIR/app"
OUT="$PKG_DIR/build/$APP_NAME.app"
INSTALL_DIR="/Applications"
BUNDLE_DEPS="${BUNDLE_DEPS:-on}"

# Prerequisites first, with what to do about them (a fresh Mac has neither).
if ! xcode-select -p >/dev/null 2>&1 || ! command -v swift >/dev/null; then
  echo "The Xcode Command Line Tools are missing. Install them with: xcode-select --install" >&2
  exit 1
fi
if [[ "$BUNDLE_DEPS" == on ]] && ! "$REPO_DIR/scripts/build-deps.sh" --check >/dev/null 2>&1 && ! command -v cmake >/dev/null; then
  echo "cmake is needed to build whisper.cpp and llama.cpp into the app: brew install cmake" >&2
  echo "(Just want to use the app? Download the DMG from the latest release instead: see the README.)" >&2
  exit 1
fi

echo "Building the app..."
swift build -c release --package-path "$PKG_DIR"
BIN_DIR="$(swift build -c release --package-path "$PKG_DIR" --show-bin-path)"

rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS" "$OUT/Contents/Resources/ThirdPartyLicenses"
cp "$BIN_DIR/$APP_NAME" "$OUT/Contents/MacOS/$APP_NAME"
cp "$PKG_DIR/Info.plist" "$OUT/Contents/Info.plist"
cp "$REPO_DIR/scripts/dictate.sh" "$OUT/Contents/Resources/dictate.sh"
cp -R "$REPO_DIR/prompts" "$OUT/Contents/Resources/prompts"
cp "$PKG_DIR/Resources/AppIcon.icns" "$OUT/Contents/Resources/AppIcon.icns"
chmod +x "$OUT/Contents/Resources/dictate.sh"
cp "$REPO_DIR/LICENSE" "$OUT/Contents/Resources/ThirdPartyLicenses/voice-to-text.txt"

if [[ -n "${VERSION:-}" ]]; then
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString ${VERSION#v}" "$OUT/Contents/Info.plist"
  # A build number that grows with every release: the commit count.
  /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $(git -C "$REPO_DIR" rev-list --count HEAD)" "$OUT/Contents/Info.plist"
fi

if [[ "$BUNDLE_DEPS" == on ]]; then
  DEPS_BIN="$("$REPO_DIR/scripts/build-deps.sh" | tail -1)"
  mkdir -p "$OUT/Contents/Helpers"
  cp "$DEPS_BIN"/whisper-server "$DEPS_BIN"/whisper-cli "$DEPS_BIN"/llama-server "$OUT/Contents/Helpers/"
  cp "$DEPS_BIN"/../licenses/*.txt "$OUT/Contents/Resources/ThirdPartyLicenses/"
fi

# Sign with a stable identity so permission grants survive rebuilds and updates: SIGN_IDENTITY (release.sh), or the
# local one from setup-signing.sh. Falls back to ad-hoc signing, where every rebuild needs Accessibility granted again.
# The helpers are signed first: a bundle's signature covers the code inside it.
SIGN_ARGS=()
if [[ -n "${SIGN_IDENTITY:-}" ]]; then
  SIGN_ARGS=(--sign "$SIGN_IDENTITY" ${SIGN_KEYCHAIN:+--keychain "$SIGN_KEYCHAIN"})
  # Developer ID builds need the hardened runtime (for notarization), and the app then needs the mic entitlement.
  if [[ "$SIGN_IDENTITY" == "Developer ID"* ]]; then
    SIGN_ARGS+=(--options runtime --timestamp)
  fi
else
  IDENTITY="Voice to Text Local Signing"
  KEYCHAIN="$HOME/Library/Keychains/voice-to-text-signing.keychain-db"
  PASS_FILE="$HOME/.config/voice-to-text/signing-keychain-password"
  if [[ -f "$KEYCHAIN" && -f "$PASS_FILE" ]]; then
    security unlock-keychain -p "$(cat "$PASS_FILE")" "$KEYCHAIN"
    # A maintainer's keychain also holds the release certificate (setup-signing.sh): sign like a release, so a local
    # build and a downloaded release are the same app to macOS and keep the same permission grants.
    if security find-certificate -c "Voice to Text Release" "$KEYCHAIN" >/dev/null 2>&1; then
      IDENTITY="Voice to Text Release"
    fi
    SIGN_ARGS=(--sign "$IDENTITY" --keychain "$KEYCHAIN")
  else
    echo "warning: no stable signing identity (run scripts/setup-signing.sh); using ad-hoc signing" >&2
    SIGN_ARGS=(--sign -)
  fi
fi
sign_failed() {
  echo "Signing $1 failed (above). Check the Command Line Tools with: xcode-select -p && xcrun --find codesign_allocate" >&2
  echo "If they look broken, reinstall them: sudo rm -rf /Library/Developer/CommandLineTools && xcode-select --install" >&2
  exit 1
}
for helper in "$OUT"/Contents/Helpers/*; do
  [[ -f "$helper" ]] || continue
  codesign --force "${SIGN_ARGS[@]}" "$helper" || sign_failed "$helper"
done
codesign --force "${SIGN_ARGS[@]}" --entitlements "$PKG_DIR/VoiceToText.entitlements" --identifier "$BUNDLE_ID" "$OUT" ||
  sign_failed "$OUT"
echo "Built $OUT"

# Installed under the product name, like the DMG does, replacing any older copy (including one named VoiceToText.app).
# A copy under the old name, "Voice to Text.app", is left for the app to offer to trash (Migration.swift).
if [[ "${1:-}" == "--install" ]]; then
  INSTALLED="$INSTALL_DIR/$PRODUCT_NAME.app"
  pkill -x "$APP_NAME" 2>/dev/null || true
  mkdir -p "$INSTALL_DIR"
  rm -rf "${INSTALL_DIR:?}/$APP_NAME.app" "$INSTALLED"
  cp -R "$OUT" "$INSTALLED"
  echo "Installed $INSTALLED"
  open "$INSTALLED"
fi
