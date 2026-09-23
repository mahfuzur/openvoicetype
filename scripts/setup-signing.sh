#!/usr/bin/env bash
# Creates a local, self-signed code-signing identity in its own keychain, so that macOS keeps
# Microphone and Accessibility grants across rebuilds. Ad-hoc signatures are identified by the
# binary's hash, so every rebuild counts as a new app; a fixed certificate does not change.
#
# It never touches the login keychain. To undo it:
#   security delete-keychain ~/Library/Keychains/voice-to-text-signing.keychain-db
set -euo pipefail

IDENTITY="Voice to Text Local Signing"
KEYCHAIN="$HOME/Library/Keychains/voice-to-text-signing.keychain-db"
PASS_FILE="$HOME/.config/voice-to-text/signing-keychain-password"

if [[ -f "$KEYCHAIN" && -f "$PASS_FILE" ]] &&
  security find-certificate -c "$IDENTITY" "$KEYCHAIN" >/dev/null 2>&1; then
  echo "Signing identity already set up: $IDENTITY"
  exit 0
fi

mkdir -p "$(dirname "$PASS_FILE")"
(umask 077 && /usr/bin/openssl rand -hex 24 >"$PASS_FILE")
PASSWORD="$(cat "$PASS_FILE")"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

cat >"$WORK/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $IDENTITY
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF

/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$WORK/key.pem" -out "$WORK/cert.pem" -config "$WORK/cert.cnf" 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
  -out "$WORK/identity.p12" -passout "pass:$PASSWORD"

[[ -f "$KEYCHAIN" ]] && security delete-keychain "$KEYCHAIN"
security create-keychain -p "$PASSWORD" "$KEYCHAIN"
security set-keychain-settings "$KEYCHAIN" # no auto-lock timeout
security unlock-keychain -p "$PASSWORD" "$KEYCHAIN"
security import "$WORK/identity.p12" -k "$KEYCHAIN" -P "$PASSWORD" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple: -s -k "$PASSWORD" "$KEYCHAIN" >/dev/null

# Add it to the user search list (keeping the existing keychains) so codesign can find it.
existing=()
while IFS= read -r line; do
  line="${line//\"/}"
  line="${line#"${line%%[![:space:]]*}"}"
  [[ -n "$line" && "$line" != "$KEYCHAIN" ]] && existing+=("$line")
done < <(security list-keychains -d user)
security list-keychains -d user -s "${existing[@]}" "$KEYCHAIN"

echo "Created signing identity: $IDENTITY"
