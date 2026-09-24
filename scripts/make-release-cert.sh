#!/usr/bin/env bash
# Creates the self-signed "Voice to Text Release" code-signing certificate for releases without an Apple Developer
# account. Every release signed with it has the same identity, so macOS keeps Microphone and Accessibility grants
# across updates (ad-hoc signing loses them). Users still click Open Anyway once on first launch.
#
# Run it once, then add the printed values as GitHub Actions secrets. Keep the output folder private and backed up:
# a new certificate means every user grants permissions again.
set -euo pipefail

IDENTITY="Voice to Text Release"
OUT="${1:-$HOME/.config/voice-to-text/release-cert}"

if [[ -f "$OUT/release.p12" ]]; then
  echo "Already exists: $OUT/release.p12 (delete it to make a new one; users would re-grant permissions)" >&2
  exit 1
fi
mkdir -p "$OUT"
chmod 700 "$OUT"
PASSWORD="$(/usr/bin/openssl rand -hex 24)"

cat >"$OUT/cert.cnf" <<EOF
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

/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 7300 \
  -keyout "$OUT/key.pem" -out "$OUT/cert.pem" -config "$OUT/cert.cnf" 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey "$OUT/key.pem" -in "$OUT/cert.pem" \
  -out "$OUT/release.p12" -passout "pass:$PASSWORD"
rm -f "$OUT/key.pem" "$OUT/cert.cnf"
(umask 077 && printf '%s\n' "$PASSWORD" >"$OUT/release.p12.password")
(umask 077 && base64 <"$OUT/release.p12" | tr -d '\n' >"$OUT/release.p12.base64")

cat <<EOF
Created $OUT/release.p12 (valid 20 years).

Add these GitHub Actions secrets (Settings → Secrets and variables → Actions):
  SIGN_P12           contents of $OUT/release.p12.base64
  SIGN_P12_PASSWORD  contents of $OUT/release.p12.password
  SIGN_IDENTITY      $IDENTITY

Then run ./scripts/setup-signing.sh, so your local builds are signed the same way (switching between a local
build and a downloaded release then keeps the Accessibility permission).

With gh:  gh secret set SIGN_P12 <"$OUT/release.p12.base64"
          gh secret set SIGN_P12_PASSWORD <"$OUT/release.p12.password"
          gh secret set SIGN_IDENTITY --body "$IDENTITY"
EOF
