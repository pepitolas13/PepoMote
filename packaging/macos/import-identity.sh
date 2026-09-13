#!/usr/bin/env bash
# CI (macOS): importa la identidad de firma del secret MACOS_SIGN_P12_B64
# (un .p12 en base64, contraseña en MACOS_SIGN_P12_PASSWORD) en un llavero
# temporal y deja su nombre en IDENTITY ($GITHUB_ENV). Sin secret (forks,
# PRs): IDENTITY=- (firma ad hoc). Cómo se crea el certificado: docs/RELEASE.md.
set -euo pipefail

if [[ -z "${MACOS_SIGN_P12_B64:-}" ]]; then
    echo "IDENTITY=-" >> "$GITHUB_ENV"
    echo "Sin MACOS_SIGN_P12_B64: firma ad hoc (los permisos no se conservan entre versiones)"
    exit 0
fi

KC="$RUNNER_TEMP/pepomote-sign.keychain-db"
KCPW="$(uuidgen)"
P12="$RUNNER_TEMP/sign.p12"
CER="$RUNNER_TEMP/sign.cer"
echo "$MACOS_SIGN_P12_B64" | base64 -d > "$P12"

security create-keychain -p "$KCPW" "$KC"
security set-keychain-settings -lut 21600 "$KC"
security unlock-keychain -p "$KCPW" "$KC"
security import "$P12" -k "$KC" -P "$MACOS_SIGN_P12_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security
# El certificado es autofirmado: hay que confiar en él para firmar código
openssl pkcs12 -in "$P12" -passin "pass:$MACOS_SIGN_P12_PASSWORD" -nokeys -clcerts -out "$CER" -legacy 2>/dev/null \
    || openssl pkcs12 -in "$P12" -passin "pass:$MACOS_SIGN_P12_PASSWORD" -nokeys -clcerts -out "$CER"
sudo security add-trusted-cert -d -r trustRoot -p codeSign -k "$KC" "$CER"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KCPW" "$KC" >/dev/null
security list-keychains -d user -s "$KC" $(security list-keychains -d user | tr -d '"')

IDENTITY="$(security find-identity -v -p codesigning "$KC" | awk -F'"' 'NR==1{print $2}')"
if [[ -z "$IDENTITY" ]]; then
    echo "::error::el .p12 no contiene una identidad de firma válida"
    security find-identity "$KC" || true
    exit 1
fi
echo "IDENTITY=$IDENTITY" >> "$GITHUB_ENV"
echo "Identidad de firma: $IDENTITY"
rm -f "$P12" "$CER"
