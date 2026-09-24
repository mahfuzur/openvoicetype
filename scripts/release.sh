#!/usr/bin/env bash
# Builds a release DMG: scripts/release.sh v0.2.0 -> dist/OpenVoiceType-0.2.0.dmg (+ .sha256).
#
# Signing uses the highest level whose secrets are set (see docs/plans/M4-app-and-install.md §3G):
#   SIGN_P12 (base64 .p12), SIGN_P12_PASSWORD, SIGN_IDENTITY (its certificate name):
#       "Developer ID Application: …" -> hardened runtime, and notarized if NOTARY_APPLE_ID, NOTARY_TEAM_ID and
#       NOTARY_PASSWORD (an app-specific password) are set too. Users open it with a double-click.
#       "Voice to Text Release" (scripts/make-release-cert.sh) -> users click Open Anyway once; permissions
#       survive updates because every release has the same certificate.
#   none -> the local identity from setup-signing.sh, or ad-hoc (for trying the DMG locally).
set -euo pipefail

export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"

VERSION="${1:?usage: scripts/release.sh <version, e.g. v0.2.0>}"
VERSION="${VERSION#v}"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$REPO_DIR/dist"
APP="$REPO_DIR/app/build/VoiceToText.app"
DMG="$DIST/OpenVoiceType-$VERSION.dmg"
WORK="$(mktemp -d)"
KEYCHAIN=""
SEARCH_LIST=()

cleanup() {
  if [[ -n "$KEYCHAIN" ]]; then
    ((${#SEARCH_LIST[@]})) && security list-keychains -d user -s "${SEARCH_LIST[@]}"
    security delete-keychain "$KEYCHAIN" 2>/dev/null || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# A release certificate from CI secrets goes into a temporary keychain, deleted on exit.
if [[ -n "${SIGN_P12:-}" ]]; then
  : "${SIGN_IDENTITY:?set SIGN_IDENTITY to the certificate name}"
  KEYCHAIN="$WORK/release.keychain-db"
  KEYCHAIN_PASSWORD="$(/usr/bin/openssl rand -hex 16)"
  printf '%s' "$SIGN_P12" | base64 --decode >"$WORK/cert.p12"
  security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
  security set-keychain-settings "$KEYCHAIN"
  security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
  security import "$WORK/cert.p12" -k "$KEYCHAIN" -P "${SIGN_P12_PASSWORD:-}" -T /usr/bin/codesign >/dev/null
  security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null
  # codesign only finds an identity in a keychain on the search list, even with --keychain. Restored on exit.
  while IFS= read -r line; do
    line="${line//\"/}"
    line="${line#"${line%%[![:space:]]*}"}"
    [[ -n "$line" ]] && SEARCH_LIST+=("$line")
  done < <(security list-keychains -d user)
  security list-keychains -d user -s "${SEARCH_LIST[@]}" "$KEYCHAIN"
  export SIGN_KEYCHAIN="$KEYCHAIN"
  export SIGN_IDENTITY
fi

VERSION="$VERSION" "$REPO_DIR/scripts/build-app.sh"
codesign --verify --strict --deep "$APP"

# The DMG window (background with an arrow, big icons, no toolbar) is written by dmgbuild, which needs no Finder
# scripting, so it also works in CI. It's installed once into a virtualenv under app/build.
mkdir -p "$DIST"
rm -f "$DMG"
VENV="$REPO_DIR/app/build/dmgbuild-venv"
if [[ -x "$VENV/bin/dmgbuild" ]] || { python3 -m venv "$VENV" && "$VENV/bin/pip" install -q --disable-pip-version-check "dmgbuild==1.6.7"; }; then
  "$VENV/bin/dmgbuild" -s "$REPO_DIR/scripts/dmg-settings.py" -D app="$APP" \
    -D background="$REPO_DIR/app/Resources/dmg-background.tiff" -D icon="$REPO_DIR/app/Resources/AppIcon.icns" \
    "OpenVoiceType" "$DMG" >/dev/null
else
  echo "warning: couldn't install dmgbuild; building a plain DMG without the window layout" >&2
  mkdir -p "$WORK/dmg"
  cp -R "$APP" "$WORK/dmg/OpenVoiceType.app"
  ln -s /Applications "$WORK/dmg/Applications"
  hdiutil create -quiet -volname "OpenVoiceType" -srcfolder "$WORK/dmg" -ov -format UDZO "$DMG"
fi

if [[ "${SIGN_IDENTITY:-}" == "Developer ID"* ]]; then
  codesign --force --sign "$SIGN_IDENTITY" ${SIGN_KEYCHAIN:+--keychain "$SIGN_KEYCHAIN"} --timestamp "$DMG"
  if [[ -n "${NOTARY_APPLE_ID:-}" && -n "${NOTARY_TEAM_ID:-}" && -n "${NOTARY_PASSWORD:-}" ]]; then
    echo "Notarizing (a few minutes)…"
    xcrun notarytool submit "$DMG" --apple-id "$NOTARY_APPLE_ID" --team-id "$NOTARY_TEAM_ID" \
      --password "$NOTARY_PASSWORD" --wait
    xcrun stapler staple "$DMG"
  else
    echo "warning: Developer ID signed but not notarized (NOTARY_* not set); Gatekeeper will still warn" >&2
  fi
fi

# The app icon on the .dmg file itself, after signing (it's Finder metadata, not part of the signed data). Only local
# copies keep it: a download carries just the file's contents, so users see the icon on the opened disk instead.
osascript -l JavaScript -e "ObjC.import('AppKit');
  \$.NSWorkspace.sharedWorkspace.setIconForFileOptions(
    \$.NSImage.alloc.initWithContentsOfFile('$REPO_DIR/app/Resources/AppIcon.icns'), '$DMG', 0)" >/dev/null ||
  echo "warning: couldn't set the DMG's file icon" >&2

(cd "$DIST" && shasum -a 256 "$(basename "$DMG")" >"$(basename "$DMG").sha256")
echo "Built $DMG ($(du -h "$DMG" | cut -f1)), signed as: $(codesign -dvv "$APP" 2>&1 | sed -n 's/^Authority=//p' | head -1)"
